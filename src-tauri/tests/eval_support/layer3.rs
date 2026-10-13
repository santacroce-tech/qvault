//! Opt-in Layer 3 evaluation with real models (see eval/README.md).
//!
//!   QVAULT_EVAL_L3=1 QVAC_MODELS_DIR=/path/to/models \
//!   QVAULT_EVAL_MODELS=qwen2.5-0.5b-instruct-q4_k_m,qwen3-4b=/abs/qwen3-4b.gguf \
//!   QVAULT_EVAL_RUNS=3 QVAULT_EVAL_PROMPTS=p3,p1b QVAULT_EVAL_OUT=../eval/reports \
//!   cargo test --release --test eval_harness layer3 -- --nocapture --test-threads 1
//!
//! Model entries are catalog ids (file looked up in QVAC_MODELS_DIR) or
//! `name=/abs/path.gguf` for candidates outside the catalog (installed into
//! a throw-away models dir as a custom model, hashed like an import).
//! For each model: assessFit verdict, load time, N runs of Layer 3 over every
//! fixture page with the first prompt (raw answers recorded), peak RSS of the
//! sidecar process tree, seconds per page, P/R/F1 per language for Layer 3
//! alone and Layers 1+2+3, run-to-run variance, an ablation of every
//! post-processing heuristic replayed on the recorded answers, a per-category
//! precision table (confidence calibration), and one run per extra prompt.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use app_lib::llm::llm_scanner::{self, Layer3Engine, Layer3Error, PostConfig, PromptVersion};
use app_lib::pipeline::types::PiiEntity;
use app_lib::qvac::{catalog, QvacService, SidecarPaths};

use super::*;

fn enabled() -> bool {
    std::env::var("QVAULT_EVAL_L3").map(|v| v == "1").unwrap_or(false)
}

pub(crate) fn sidecar() -> SidecarPaths {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let program = std::env::var_os("QVAC_SIDECAR_BIN").map(PathBuf::from).unwrap_or_else(|| {
        manifest
            .join("binaries")
            .join(format!("qvac-sidecar-{}{}", env!("QVAULT_TARGET_TRIPLE"), std::env::consts::EXE_SUFFIX))
    });
    let worker = std::env::var_os("QVAC_SIDECAR_WORKER")
        .map(PathBuf::from)
        .unwrap_or_else(|| manifest.join("resources").join("qvac-worker").join("worker.js"));
    SidecarPaths::checked(program.clone(), worker.clone())
        .unwrap_or_else(|| panic!("sidecar not built: {:?} / {:?} (run `npm run sidecar:build`)", program, worker))
}

// ── engines: record live answers, replay them for ablations ─────────────

struct Recorder<'a> {
    inner: &'a dyn Layer3Engine,
    answers: RefCell<HashMap<String, String>>,
    seconds: RefCell<f64>,
    calls: RefCell<usize>,
    failures: RefCell<Vec<String>>,
}

impl Layer3Engine for Recorder<'_> {
    fn complete(&self, chunk: &str) -> Result<String, Layer3Error> {
        let t = Instant::now();
        let r = self.inner.complete(chunk);
        if std::env::var("QVAULT_EVAL_VERBOSE").is_ok() {
            eprintln!("  chunk {} words: {:.1}s {}", chunk.split_whitespace().count(), t.elapsed().as_secs_f64(), match &r { Ok(a) => format!("{} chars", a.len()), Err(e) => format!("{:?}", e) });
        }
        *self.seconds.borrow_mut() += t.elapsed().as_secs_f64();
        *self.calls.borrow_mut() += 1;
        match &r {
            Ok(a) => {
                self.answers.borrow_mut().insert(chunk.to_string(), a.clone());
            }
            Err(e) => self.failures.borrow_mut().push(format!("{:?}", e)),
        }
        r
    }
}

struct Replay<'a>(&'a HashMap<String, String>);

impl Layer3Engine for Replay<'_> {
    fn complete(&self, chunk: &str) -> Result<String, Layer3Error> {
        self.0.get(chunk).cloned().ok_or_else(|| Layer3Error::Chunk("no recorded answer".into()))
    }
}

// ── memory: peak RSS of our child process tree (the sidecar) ────────────

pub(crate) struct MemSampler {
    stop: Arc<AtomicBool>,
    peak_kb: Arc<AtomicU64>,
    handle: Option<std::thread::JoinHandle<()>>,
}

fn descendants_rss_kb(root: u32) -> u64 {
    let out = match std::process::Command::new("ps").args(["-A", "-o", "pid=,ppid=,rss="]).output() {
        Ok(o) => String::from_utf8_lossy(&o.stdout).to_string(),
        Err(_) => return 0,
    };
    let rows: Vec<(u32, u32, u64)> = out
        .lines()
        .filter_map(|l| {
            let mut it = l.split_whitespace();
            Some((it.next()?.parse().ok()?, it.next()?.parse().ok()?, it.next()?.parse().ok()?))
        })
        .collect();
    let mut set: BTreeSet<u32> = BTreeSet::new();
    set.insert(root);
    loop {
        let before = set.len();
        for (pid, ppid, _) in &rows {
            if set.contains(ppid) {
                set.insert(*pid);
            }
        }
        if set.len() == before {
            break;
        }
    }
    rows.iter().filter(|(pid, _, _)| *pid != root && set.contains(pid)).map(|r| r.2).sum()
}

impl MemSampler {
    pub(crate) fn start() -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let peak_kb = Arc::new(AtomicU64::new(0));
        let (s, p) = (stop.clone(), peak_kb.clone());
        let me = std::process::id();
        let handle = std::thread::spawn(move || {
            while !s.load(Ordering::Relaxed) {
                let kb = descendants_rss_kb(me);
                p.fetch_max(kb, Ordering::Relaxed);
                std::thread::sleep(Duration::from_millis(300));
            }
        });
        MemSampler { stop, peak_kb, handle: Some(handle) }
    }
    pub(crate) fn finish(mut self) -> u64 {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
        self.peak_kb.load(Ordering::Relaxed)
    }
}

// ── model setup ──────────────────────────────────────────────────────────

pub(crate) struct ModelSpec {
    pub(crate) id: String,
    pub(crate) path: PathBuf,
    pub(crate) custom: bool,
}

fn model_specs(models_dir: &Path) -> Vec<ModelSpec> {
    let list = std::env::var("QVAULT_EVAL_MODELS").unwrap_or_else(|_| catalog::default_llm().id.to_string());
    list.split(',')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| match s.split_once('=') {
            Some((id, path)) => ModelSpec { id: id.to_string(), path: PathBuf::from(path), custom: true },
            None => {
                let m = catalog::find(s).unwrap_or_else(|| panic!("{} is not a catalog id; use name=/path", s));
                ModelSpec { id: s.to_string(), path: models_dir.join(m.file_name), custom: false }
            }
        })
        .collect()
}

/// A throw-away models dir holding just this model (symlinked), so the
/// service loads it exactly as the app would.
pub(crate) fn install(spec: &ModelSpec, root: &Path) -> (PathBuf, String) {
    let dir = root.join(format!("models-{}", spec.id));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    assert!(spec.path.is_file(), "model file missing: {}", spec.path.display());
    if !spec.custom {
        let m = catalog::find(&spec.id).unwrap();
        std::os::unix::fs::symlink(&spec.path, dir.join(m.file_name)).unwrap();
        return (dir, spec.id.clone());
    }
    let (sha, size) = app_lib::qvac::models::hash_file(&spec.path, None).expect("hash");
    let custom = dir.join("custom");
    std::fs::create_dir_all(&custom).unwrap();
    let file_name = format!("{}.gguf", spec.id);
    std::os::unix::fs::symlink(&spec.path, custom.join(&file_name)).unwrap();
    let id = format!("custom-{}", &sha[..12]);
    let entry = json!([{
        "id": id, "name": spec.id, "fileName": file_name, "size": size, "sha256": sha,
        "acceptedAt": "eval",
    }]);
    std::fs::write(custom.join("custom.json"), serde_json::to_vec_pretty(&entry).unwrap()).unwrap();
    eprintln!("[{}] sha256 {} size {}", spec.id, sha, size);
    (dir, id)
}

// ── scoring helpers ──────────────────────────────────────────────────────

pub(crate) fn l3_preds(fixtures: &[Fixture], engine: &dyn Layer3Engine, cfg: &PostConfig, with_l12: bool) -> (Predictions, llm_scanner::DropStats, usize, usize) {
    let mut stats = llm_scanner::DropStats::default();
    let (mut chunks, mut failed) = (0, 0);
    let preds = fixtures
        .iter()
        .map(|fx| {
            fx.pages
                .iter()
                .enumerate()
                .map(|(i, p)| {
                    let l12: Vec<PiiEntity> = if with_l12 { layers12(&p.text, i as u32) } else { vec![] };
                    let ranges: Vec<(usize, usize)> = l12.iter().map(|e| (e.start, e.end)).collect();
                    let r = llm_scanner::scan_with(engine, &p.text, i as u32, &ranges, None, cfg);
                    stats.add(&r.stats);
                    chunks += r.chunks;
                    failed += r.failed_chunks;
                    let mut all: Vec<Pred> = l12.iter().map(Pred::from).collect();
                    all.extend(r.entities.iter().map(Pred::from));
                    all
                })
                .collect()
        })
        .collect();
    (preds, stats, chunks, failed)
}

fn mean_std(xs: &[f64]) -> (f64, f64) {
    let n = xs.len() as f64;
    let m = xs.iter().sum::<f64>() / n;
    let v = if xs.len() > 1 { xs.iter().map(|x| (x - m).powi(2)).sum::<f64>() / (n - 1.0) } else { 0.0 };
    (m, v.sqrt())
}

fn pm(xs: &[f64]) -> String {
    let (m, s) = mean_std(xs);
    if xs.len() > 1 { format!("{} ± {}", pct(m), pct(s)) } else { pct(m) }
}

fn secs_pm(xs: &[f64]) -> String {
    let (m, s) = mean_std(xs);
    if xs.len() > 1 { format!("{:.2} ± {:.2}", m, s) } else { format!("{:.2}", m) }
}

fn span_set(fixtures: &[Fixture], p: &Predictions) -> BTreeSet<(usize, usize, usize, usize)> {
    let mut s = BTreeSet::new();
    for (fi, fx) in fixtures.iter().enumerate() {
        for pi in 0..fx.pages.len() {
            for x in &p[fi][pi] {
                s.insert((fi, pi, x.start, x.end));
            }
        }
    }
    s
}

fn jaccard<T: Ord>(a: &BTreeSet<T>, b: &BTreeSet<T>) -> f64 {
    let i = a.intersection(b).count();
    let u = a.union(b).count();
    ratio(i, u)
}

fn f1s(r: &Report) -> (f64, f64, f64) {
    (r.overall.overlap.precision(), r.overall.overlap.recall(), r.overall.overlap.f1())
}

#[test]
fn layer3_models() {
    if !enabled() {
        eprintln!("skipping: set QVAULT_EVAL_L3=1 (and QVAC_MODELS_DIR, QVAULT_EVAL_MODELS) to run");
        return;
    }
    let mut fixtures = load_fixtures();
    if let Ok(only) = std::env::var("QVAULT_EVAL_FIXTURES") {
        let ids: Vec<&str> = only.split(',').map(|s| s.trim()).collect();
        fixtures.retain(|f| ids.contains(&f.id.as_str()));
    }
    let pages: usize = fixtures.iter().map(|f| f.pages.len()).sum();
    let models_dir = PathBuf::from(std::env::var("QVAC_MODELS_DIR").expect("QVAC_MODELS_DIR"));
    let runs: usize = std::env::var("QVAULT_EVAL_RUNS").ok().and_then(|v| v.parse().ok()).unwrap_or(3);
    let prompts: Vec<PromptVersion> = std::env::var("QVAULT_EVAL_PROMPTS")
        .unwrap_or_else(|_| llm_scanner::DEFAULT_PROMPT.id().into())
        .split(',')
        .map(|s| PromptVersion::parse(s.trim()).unwrap_or_else(|| panic!("unknown prompt {}", s)))
        .collect();
    let root = std::env::temp_dir().join(format!("qvault-eval-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let out = out_dir();
    let l12 = score(&fixtures, &run_system(&fixtures, layers12));
    let mut default_cfg = PostConfig::default();
    if let Some(n) = std::env::var("QVAULT_EVAL_CHUNK_WORDS").ok().and_then(|v| v.parse().ok()) {
        default_cfg.chunk_words = n;
        default_cfg.chunk_overlap = (n / 7).max(8);
    }

    let mut summary_rows = String::new();
    let mut summary_map = serde_json::Map::new();

    for spec in model_specs(&models_dir) {
        let (dir, id) = install(&spec, &root);
        let app_data = root.join(format!("app-{}", spec.id));
        let _ = std::fs::remove_dir_all(&app_data);
        std::fs::create_dir_all(&app_data).unwrap();
        let svc = Arc::new(QvacService::new(&app_data, Some(&dir), Some(sidecar())));
        let mem = MemSampler::start();
        let fit = svc.assess_fit(&id).map(|f| f.verdict).unwrap_or_else(|e| format!("error: {}", e));
        svc.set_layer3(true, Some(id.clone())).unwrap();
        let t = Instant::now();
        let mut session = match svc.prepare_layer3() {
            Ok(s) => s,
            Err(e) => {
                eprintln!("[{}] FAILED TO LOAD: {}", spec.id, e);
                summary_rows.push_str(&format!("| {} | {} | load failed: {} | | | | | | | |\n", spec.id, fit, e));
                summary_map.insert(spec.id.clone(), json!({"fit": fit, "load_error": e}));
                svc.shutdown();
                let _ = mem.finish();
                continue;
            }
        };
        let load_s = t.elapsed().as_secs_f64();
        eprintln!("[{}] fit={} load={:.1}s", spec.id, fit, load_s);

        let mut md = format!("# Eval: Layer 3 with `{}`\n\n", spec.id);
        md.push_str(&format!(
            "assessFit: **{}** · load {:.1} s · {} pages · {} run(s) · prompt `{}` · chunks of {} words, {} overlap · post-processing: default\n\n",
            fit, load_s, pages, runs, prompts[0].id(), default_cfg.chunk_words, default_cfg.chunk_overlap
        ));

        // N runs, first prompt, answers recorded.
        session.prompt = prompts[0];
        let mut answers: Vec<HashMap<String, String>> = Vec::new();
        let mut alone: Vec<Report> = Vec::new();
        let mut combined: Vec<Report> = Vec::new();
        let mut secs = Vec::new();
        let mut chunk_counts = (0, 0);
        let mut stats_runs = Vec::new();
        let mut engine_errors: Vec<String> = Vec::new();
        let mut unparseable = 0usize;
        for r in 0..runs {
            let rec = Recorder {
                inner: &session,
                answers: RefCell::new(HashMap::new()),
                seconds: RefCell::new(0.0),
                calls: RefCell::new(0),
                failures: RefCell::new(Vec::new()),
            };
            let (p_alone, stats, chunks, failed) = l3_preds(&fixtures, &rec, &default_cfg, false);
            let s = *rec.seconds.borrow();
            eprintln!("[{}] run {}: {:.1}s ({:.2} s/page), {} chunks, {} failed", spec.id, r + 1, s, s / pages as f64, chunks, failed);
            secs.push(s / pages as f64);
            chunk_counts = (chunks, chunk_counts.1 + failed);
            stats_runs.push(stats);
            let errs = rec.failures.borrow().clone();
            let ans = rec.answers.into_inner();
            let bad: Vec<&String> = ans.values().filter(|a| llm_scanner::parse_strict(a).is_err()).collect();
            eprintln!("[{}] engine errors: {:?}", spec.id, errs.iter().take(5).collect::<Vec<_>>());
            eprintln!("[{}] unparseable answers: {}", spec.id, bad.len());
            for b in bad.iter().take(3) {
                eprintln!("   {:?}", b.chars().take(300).collect::<String>());
            }
            engine_errors.extend(errs);
            unparseable += bad.len();
            let (p_comb, _, _, _) = l3_preds(&fixtures, &Replay(&ans), &default_cfg, true);
            alone.push(score(&fixtures, &p_alone));
            combined.push(score(&fixtures, &p_comb));
            answers.push(ans);
        }
        let peak_mb = mem.finish() as f64 / 1024.0;

        // Extra prompts: one run each.
        let mut prompt_rows = String::new();
        for (k, pv) in prompts.iter().enumerate() {
            let (a, c) = if k == 0 {
                (alone[0].clone(), combined[0].clone())
            } else {
                session.prompt = *pv;
                let rec = Recorder {
                    inner: &session,
                    answers: RefCell::new(HashMap::new()),
                    seconds: RefCell::new(0.0),
                    calls: RefCell::new(0),
                    failures: RefCell::new(Vec::new()),
                };
                let (pa, _, _, _) = l3_preds(&fixtures, &rec, &default_cfg, false);
                let ans = rec.answers.into_inner();
                let (pc, _, _, _) = l3_preds(&fixtures, &Replay(&ans), &default_cfg, true);
                if *pv == PromptVersion::P1b {
                    // Phase 1B end to end: old prompt and old post-processing.
                    let mut p1b = PostConfig::phase1b();
                    p1b.chunk_words = default_cfg.chunk_words;
                    p1b.chunk_overlap = default_cfg.chunk_overlap;
                    let (oa, _, _, _) = l3_preds(&fixtures, &Replay(&ans), &p1b, false);
                    let (oc, _, _, _) = l3_preds(&fixtures, &Replay(&ans), &p1b, true);
                    prompt_rows.push_str(&row("Phase 1B (p1b prompt + p1b post) Layer 3 alone", &score(&fixtures, &oa).overall));
                    prompt_rows.push_str(&row("Phase 1B (p1b prompt + p1b post) Layers 1+2+3", &score(&fixtures, &oc).overall));
                }
                (score(&fixtures, &pa), score(&fixtures, &pc))
            };
            prompt_rows.push_str(&row(&format!("`{}` Layer 3 alone (run 1)", pv.id()), &a.overall));
            prompt_rows.push_str(&row(&format!("`{}` Layers 1+2+3 (run 1)", pv.id()), &c.overall));
        }
        svc.shutdown();

        // Summary over runs.
        let pr = |rs: &[Report], f: fn(&Report) -> f64| rs.iter().map(f).collect::<Vec<f64>>();
        let a_p = pr(&alone, |r| r.overall.overlap.precision());
        let a_r = pr(&alone, |r| r.overall.overlap.recall());
        let a_f = pr(&alone, |r| r.overall.overlap.f1());
        let c_p = pr(&combined, |r| r.overall.overlap.precision());
        let c_r = pr(&combined, |r| r.overall.overlap.recall());
        let c_f = pr(&combined, |r| r.overall.overlap.f1());
        let c_cov = pr(&combined, |r| r.overall.covered_recall());
        let lang_f1 = |lang: &str| pm(&combined.iter().map(|r| r.by_lang.get(lang).map(|b| b.overlap.f1()).unwrap_or(0.0)).collect::<Vec<_>>());
        let lang_f1_alone = |lang: &str| pm(&alone.iter().map(|r| r.by_lang.get(lang).map(|b| b.overlap.f1()).unwrap_or(0.0)).collect::<Vec<_>>());

        // Run-to-run variance.
        let sets: Vec<_> = alone.iter().enumerate().map(|(i, _)| i).collect::<Vec<_>>();
        let _ = sets;
        let span_sets: Vec<BTreeSet<_>> = answers
            .iter()
            .map(|ans| span_set(&fixtures, &l3_preds(&fixtures, &Replay(ans), &default_cfg, false).0))
            .collect();
        let mut jac = Vec::new();
        let mut same_answers = Vec::new();
        for i in 0..answers.len() {
            for j in i + 1..answers.len() {
                jac.push(jaccard(&span_sets[i], &span_sets[j]));
                let keys: BTreeSet<&String> = answers[i].keys().chain(answers[j].keys()).collect();
                let same = keys.iter().filter(|k| answers[i].get(**k) == answers[j].get(**k)).count();
                same_answers.push(ratio(same, keys.len()));
            }
        }

        md.push_str("## Overall (overlap matching; mean ± sd over runs)\n\n| | P | R | F1 | covered R | EN F1 | ES F1 | PT F1 |\n|---|---|---|---|---|---|---|---|\n");
        md.push_str(&format!(
            "| Layer 3 alone | {} | {} | {} | {} | {} | {} | {} |\n",
            pm(&a_p), pm(&a_r), pm(&a_f),
            pm(&pr(&alone, |r| r.overall.covered_recall())),
            lang_f1_alone("en"), lang_f1_alone("es"), lang_f1_alone("pt")
        ));
        md.push_str(&format!(
            "| Layers 1+2 | {} | {} | {} | {} | {} | {} | {} |\n",
            pct(l12.overall.overlap.precision()), pct(l12.overall.overlap.recall()), pct(l12.overall.overlap.f1()),
            pct(l12.overall.covered_recall()),
            pct(l12.by_lang.get("en").map(|b| b.overlap.f1()).unwrap_or(0.0)), pct(l12.by_lang.get("es").map(|b| b.overlap.f1()).unwrap_or(0.0)), pct(l12.by_lang.get("pt").map(|b| b.overlap.f1()).unwrap_or(0.0))
        ));
        md.push_str(&format!(
            "| Layers 1+2+3 | {} | {} | {} | {} | {} | {} | {} |\n\n",
            pm(&c_p), pm(&c_r), pm(&c_f), pm(&c_cov), lang_f1("en"), lang_f1("es"), lang_f1("pt")
        ));
        md.push_str(&format!(
            "Speed: {} s/page (Layer 3 only; {} chunks/run, {} failed chunks over all runs: {} engine errors, {} unparseable answers). Peak RSS of the sidecar: {:.0} MB.\n\n",
            secs_pm(&secs), chunk_counts.0, chunk_counts.1, engine_errors.len(), unparseable, peak_mb
        ));
        md.push_str(&format!(
            "Run-to-run: span-set Jaccard {} (pairs of runs), identical raw answers {} of chunks.\n\n",
            if jac.is_empty() { "–".into() } else { pm(&jac) },
            if same_answers.is_empty() { "–".into() } else { pm(&same_answers) }
        ));
        md.push_str("### Exact / overlap / covered, run 1\n\n");
        md.push_str(TABLE_HEAD);
        md.push_str(&row("Layer 3 alone", &alone[0].overall));
        md.push_str(&row("Layers 1+2+3", &combined[0].overall));
        for (s, b) in &combined[0].by_split {
            md.push_str(&row(&format!("Layers 1+2+3 – {}", s), b));
        }

        md.push_str("\n### Prompt versions (run 1 each)\n\n");
        md.push_str(TABLE_HEAD);
        md.push_str(&prompt_rows);

        // Ablation on run 1's answers.
        md.push_str("\n### Post-processing ablation (replayed on run 1's answers)\n\n| config | L3 P | L3 R | L3 F1 | L1+2+3 P | L1+2+3 R | L1+2+3 F1 |\n|---|---|---|---|---|---|---|\n");
        let mut p1b = PostConfig::phase1b();
        p1b.chunk_words = default_cfg.chunk_words;
        p1b.chunk_overlap = default_cfg.chunk_overlap;
        let mut ablations: Vec<(&str, PostConfig)> = vec![("default", default_cfg.clone()), ("Phase 1B post-processing", p1b)];
        let mut c = default_cfg.clone();
        c.stoplist = false;
        ablations.push(("− stop-list", c));
        let mut c = default_cfg.clone();
        c.last_first = false;
        ablations.push(("− Last, First variants", c));
        let mut c = default_cfg.clone();
        c.honorific_surname = false;
        ablations.push(("− honorific + surname", c));
        let mut c = default_cfg.clone();
        c.page_wide = false;
        ablations.push(("− page-wide mapping", c));
        let mut c = default_cfg.clone();
        c.matching.fold_accents = false;
        ablations.push(("− accent folding", c));
        let mut c = default_cfg.clone();
        c.matching.hyphen_wrap = false;
        ablations.push(("− hyphen line-wrap", c));
        let mut abl_json = serde_json::Map::new();
        for (name, cfg) in &ablations {
            let (pa, st, _, _) = l3_preds(&fixtures, &Replay(&answers[0]), cfg, false);
            let (pc, _, _, _) = l3_preds(&fixtures, &Replay(&answers[0]), cfg, true);
            let (ra, rc) = (score(&fixtures, &pa), score(&fixtures, &pc));
            let (ap, ar, af) = f1s(&ra);
            let (cp, cr, cf) = f1s(&rc);
            md.push_str(&format!("| {} | {} | {} | {} | {} | {} | {} |\n", name, pct(ap), pct(ar), pct(af), pct(cp), pct(cr), pct(cf)));
            abl_json.insert(name.to_string(), json!({"l3": [ap, ar, af], "l123": [cp, cr, cf], "stats": st}));
        }

        // Drop statistics.
        md.push_str("\n### What happened to the model's items (run 1)\n\n```\n");
        md.push_str(&serde_json::to_string_pretty(&stats_runs[0]).unwrap());
        md.push_str("\n```\n");

        // Calibration: Layer 3 alone precision by category, and by agreement.
        md.push_str("\n### Layer 3 alone by category (all runs pooled; for confidence calibration)\n\n| category | preds | P (overlap) | R (overlap) | type acc. |\n|---|---:|---:|---:|---:|\n");
        let mut pooled: BTreeMap<String, Breakdown> = BTreeMap::new();
        for r in &alone {
            for (k, b) in &r.by_category {
                pooled.entry(k.clone()).or_default().add(b);
            }
        }
        for (k, b) in &pooled {
            md.push_str(&format!(
                "| {} | {} | {} | {} | {} |\n",
                k,
                b.overlap.preds,
                if b.overlap.preds == 0 { "–".into() } else { pct(b.overlap.precision()) },
                if b.overlap.labels == 0 { "–".into() } else { pct(b.overlap.recall()) },
                pct(b.type_accuracy())
            ));
        }
        // agreement: re-score run 1 with flat confidence vs the two-chunk bonus
        let (p_agree, _, _, _) = l3_preds(&fixtures, &Replay(&answers[0]), &default_cfg, false);
        let (mut agree_tp, mut agree_n, mut single_tp, mut single_n) = (0, 0, 0, 0);
        for (fi, fx) in fixtures.iter().enumerate() {
            for (pi, page) in fx.pages.iter().enumerate() {
                for p in &p_agree[fi][pi] {
                    let base = llm_scanner::category_confidence(&llm_scanner::parse_category(&p.category).unwrap());
                    let hit = page.spans.iter().any(|s| s.start < p.end && p.start < s.end);
                    let opt_only = hit && page.spans.iter().filter(|s| s.start < p.end && p.start < s.end).all(|s| s.optional);
                    if opt_only {
                        continue;
                    }
                    if p.confidence > base + 0.001 {
                        agree_n += 1;
                        agree_tp += hit as usize;
                    } else {
                        single_n += 1;
                        single_tp += hit as usize;
                    }
                }
            }
        }
        md.push_str(&format!(
            "\nAgreement (run 1): detections confirmed by two overlapping chunks: {}/{} correct ({}); single-chunk: {}/{} ({}).\n",
            agree_tp, agree_n, pct(ratio(agree_tp, agree_n)), single_tp, single_n, pct(ratio(single_tp, single_n))
        ));

        // What Layer 3 adds on top of Layers 1-2 (the detections users see
        // from it), by predicted category, pooled over runs: the basis for
        // the per-category confidence.
        md.push_str("\n### Layer 3 additions to Layers 1+2 by category (all runs pooled; confidence calibration)\n\n| category | added | correct | P |\n|---|---:|---:|---:|\n");
        let mut added: BTreeMap<String, (usize, usize)> = BTreeMap::new();
        for ans in &answers {
            let (pc, _, _, _) = l3_preds(&fixtures, &Replay(ans), &default_cfg, true);
            for (fi, fx) in fixtures.iter().enumerate() {
                for (pi, page) in fx.pages.iter().enumerate() {
                    for p in pc[fi][pi].iter().filter(|p| p.layer == "llm") {
                        let hits: Vec<&Span> = page.spans.iter().filter(|s| s.start < p.end && p.start < s.end).collect();
                        if !hits.is_empty() && hits.iter().all(|s| s.optional) {
                            continue;
                        }
                        let e = added.entry(p.category.clone()).or_default();
                        e.0 += 1;
                        e.1 += (!hits.is_empty()) as usize;
                    }
                }
            }
        }
        for (k, (n, ok)) in &added {
            md.push_str(&format!("| {} | {} | {} | {} |\n", k, n, ok, pct(ratio(*ok, *n))));
        }
        let (n, ok) = added.values().fold((0, 0), |a, v| (a.0 + v.0, a.1 + v.1));
        md.push_str(&format!("| **all** | {} | {} | {} |\n", n, ok, pct(ratio(ok, n))));

        md.push_str("\n### Layers 1+2+3 by category, run 1\n\n");
        md.push_str(&category_table(&[("L1+2", &l12), ("L3", &alone[0]), ("L1+2+3", &combined[0])]));
        md.push_str("\n### Layers 1+2+3 errors, run 1\n\n");
        md.push_str(&lists(&combined[0], usize::MAX));
        md.push_str("\n### Layer 3 alone false positives, run 1\n\n");
        for f in &alone[0].false_positives {
            md.push_str(&format!("- `{}` p{} {}: {:?}\n", f.fixture, f.page, f.category, f.text));
        }

        let js = json!({
            "model": spec.id, "fit": fit, "load_seconds": load_s, "runs": runs, "pages": pages,
            "prompt": prompts[0].id(),
            "seconds_per_page": secs, "peak_rss_mb": peak_mb,
            "layer3_alone": alone.iter().map(summary_json).collect::<Vec<_>>(),
            "layers123": combined.iter().map(summary_json).collect::<Vec<_>>(),
            "jaccard": jac, "identical_answers": same_answers,
            "ablation": abl_json,
            "stats": stats_runs,
        });
        write(&out, &format!("layer3-{}", spec.id), &md, &js);

        summary_rows.push_str(&format!(
            "| {} | {} | {:.1} | {} | {:.0} | {} | {} | {} | {} | {} | {} | {} | {} |\n",
            spec.id, fit, load_s, secs_pm(&secs), peak_mb,
            pm(&a_p), pm(&a_r), pm(&a_f), pm(&c_f), lang_f1("en"), lang_f1("es"), lang_f1("pt"),
            if jac.is_empty() { "–".into() } else { pm(&jac) }
        ));
        summary_map.insert(spec.id.clone(), json!({
            "fit": fit, "load_seconds": load_s, "seconds_per_page": mean_std(&secs).0, "peak_rss_mb": peak_mb,
            "l3": {"p": mean_std(&a_p), "r": mean_std(&a_r), "f1": mean_std(&a_f)},
            "l123": {"p": mean_std(&c_p), "r": mean_std(&c_r), "f1": mean_std(&c_f), "covered": mean_std(&c_cov)},
            "jaccard": jac,
        }));
    }

    let mut md = String::from("# Eval: Layer 3 models\n\n");
    md.push_str(&format!(
        "Layers 1+2 alone: overlap P {} / R {} / F1 {}. Values are mean ± sd over runs (overlap matching). s/page counts only Layer 3 model time.\n\n",
        pct(l12.overall.overlap.precision()), pct(l12.overall.overlap.recall()), pct(l12.overall.overlap.f1())
    ));
    md.push_str("| model | assessFit | load s | s/page | peak RSS MB | L3 P | L3 R | L3 F1 | L1+2+3 F1 | EN F1 | ES F1 | PT F1 | run Jaccard |\n|---|---|---:|---|---:|---|---|---|---|---|---|---|---|\n");
    md.push_str(&summary_rows);
    let name = std::env::var("QVAULT_EVAL_SUMMARY").unwrap_or_else(|_| "layer3-models".into());
    write(&out, &name, &md, &Value::Object(summary_map));
    let _ = std::fs::remove_dir_all(&root);
}
