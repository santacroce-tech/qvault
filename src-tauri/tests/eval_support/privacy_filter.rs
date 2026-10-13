//! Opt-in: OpenAI Privacy Filter as Layer 3 through the app's own pipeline
//! (docs/qvac-integration.md §15, eval/README.md).
//!
//!   QVAULT_EVAL_PF=1 QVAULT_PF_MODEL_DIR=/dir/with/the/4/files \
//!   [QVAC_MODELS_DIR=/models QVAULT_EVAL_MODELS=qwen3-1.7b-q8_0] QVAULT_EVAL_OUT=../eval/reports \
//!   cargo test --release --test eval_harness privacy_filter -- --nocapture --test-threads 1
//!
//! Every page is classified once through the real sidecar (`QvacService`,
//! `@qvac/onnx`), timed, and its logits kept; every operating point and
//! post-processing variant is then replayed on those logits with the app's
//! decoder and scanner, so variants differ only in what they change.
//! Variants are chosen on the **dev** split only (rule below); the holdout
//! split is reported, never used to choose.
//!
//! With `QVAULT_EVAL_MODELS` (a catalog LLM, as in `layer3_models`) the
//! language model runs too (one run, answers recorded), giving Layers 1+2 +
//! LLM and Layers 1+2 + Privacy Filter + LLM merged the way the app's
//! thorough mode merges them (`merge_detectors`).
//!
//! "False suggestions" = detections a Layer 3 detector adds to Layers 1–2
//! that overlap no label: what the reviewer has to reject.

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use serde_json::{json, Value};

use app_lib::llm::llm_scanner::{self, Layer3Engine, Layer3Error};
use app_lib::pipeline::types::PiiEntity;
use app_lib::privacy_filter::decoder::Biases;
use app_lib::privacy_filter::scanner::{self, PageScores, PfConfig};
use app_lib::qvac::{catalog, QvacService};

use super::layer3::{install, sidecar, MemSampler, ModelSpec};
use super::*;

const AI_LAYERS: [&str; 2] = ["privacy-filter", "llm"];

fn enabled() -> bool {
    std::env::var("QVAULT_EVAL_PF").map(|v| v == "1").unwrap_or(false)
}

/// Throw-away app data whose models dir holds the Privacy Filter files.
fn pf_service(root: &std::path::Path) -> Arc<QvacService> {
    let src = PathBuf::from(std::env::var("QVAULT_PF_MODEL_DIR").expect("QVAULT_PF_MODEL_DIR"));
    let models = root.join("models-pf");
    let m = catalog::privacy_filter();
    let target = models.join(m.dir.unwrap());
    std::fs::create_dir_all(&target).unwrap();
    for f in m.files() {
        let from = [src.join(f.name), src.join("onnx").join(f.name)]
            .into_iter()
            .find(|p| p.is_file())
            .unwrap_or_else(|| panic!("{} missing in QVAULT_PF_MODEL_DIR", f.name));
        let to = target.join(f.name);
        let _ = std::fs::remove_file(&to);
        std::os::unix::fs::symlink(&from, &to).unwrap();
    }
    let app = root.join("app-pf");
    std::fs::create_dir_all(&app).unwrap();
    Arc::new(QvacService::new(&app, Some(&models), Some(sidecar())))
}

/// Records the language model's answers so it runs once.
struct Rec<'a> {
    inner: &'a dyn Layer3Engine,
    answers: RefCell<HashMap<String, String>>,
    seconds: RefCell<f64>,
}

impl Layer3Engine for Rec<'_> {
    fn complete(&self, chunk: &str) -> Result<String, Layer3Error> {
        if let Some(a) = self.answers.borrow().get(chunk) {
            return Ok(a.clone());
        }
        let t = Instant::now();
        let r = self.inner.complete(chunk);
        *self.seconds.borrow_mut() += t.elapsed().as_secs_f64();
        if let Ok(a) = &r {
            self.answers.borrow_mut().insert(chunk.to_string(), a.clone());
        }
        r
    }
}

/// Predictions of one Layer 3 setup, per (fixture, page): Layer 1–2 entities
/// plus the given Layer 3 entities.
fn combine(l12: &[Vec<Vec<PiiEntity>>], l3: &[Vec<Vec<PiiEntity>>]) -> Predictions {
    l12.iter()
        .zip(l3)
        .map(|(a, b)| a.iter().zip(b).map(|(x, y)| x.iter().chain(y.iter()).map(Pred::from).collect()).collect())
        .collect()
}

fn alone(l3: &[Vec<Vec<PiiEntity>>]) -> Predictions {
    l3.iter().map(|f| f.iter().map(|p| p.iter().map(Pred::from).collect()).collect()).collect()
}

fn false_suggestions(r: &Report) -> usize {
    r.false_positives.iter().filter(|f| AI_LAYERS.contains(&f.layer.as_str())).count()
}

/// Same, restricted to one split.
fn false_suggestions_in(r: &Report, fixtures: &[Fixture], split: &str) -> usize {
    r.false_positives
        .iter()
        .filter(|f| AI_LAYERS.contains(&f.layer.as_str()))
        .filter(|f| fixtures.iter().any(|fx| fx.id == f.fixture && fx.split == split))
        .count()
}

fn split_recall(r: &Report, split: &str) -> f64 {
    r.by_split.get(split).map(|b| b.overlap.recall()).unwrap_or(0.0)
}

fn split_f1(r: &Report, split: &str) -> f64 {
    r.by_split.get(split).map(|b| b.overlap.f1()).unwrap_or(0.0)
}

fn lang_f1(r: &Report, lang: &str) -> f64 {
    r.by_lang.get(lang).map(|b| b.overlap.f1()).unwrap_or(0.0)
}

fn pf_entities(fixtures: &[Fixture], scores: &[Vec<PageScores>], cal: &Biases, l12: &[Vec<Vec<PiiEntity>>], cfg: &PfConfig, with_l12: bool) -> Vec<Vec<Vec<PiiEntity>>> {
    fixtures
        .iter()
        .enumerate()
        .map(|(fi, fx)| {
            fx.pages
                .iter()
                .enumerate()
                .map(|(pi, p)| {
                    let existing: Vec<(usize, usize)> =
                        if with_l12 { l12[fi][pi].iter().map(|e| (e.start, e.end)).collect() } else { vec![] };
                    scanner::detections(&p.text, pi as u32, &scores[fi][pi], cal, &existing, cfg).entities
                })
                .collect()
        })
        .collect()
}

/// One row of the summary table.
fn summary_row(name: &str, r: &Report, fixtures: &[Fixture], secs: Option<f64>, mem_mb: Option<f64>) -> (String, Value) {
    let o = &r.overall.overlap;
    let fs = false_suggestions(r);
    let md = format!(
        "| {} | {} / {} / {} | {} | {} / {} / {} | {} / {} | {} | {} | {} |\n",
        name,
        pct(o.precision()),
        pct(o.recall()),
        pct(o.f1()),
        pct(r.overall.covered_recall()),
        pct(lang_f1(r, "en")),
        pct(lang_f1(r, "es")),
        pct(lang_f1(r, "pt")),
        pct(split_recall(r, "holdout")),
        pct(split_f1(r, "holdout")),
        fs,
        secs.map(|s| format!("{:.2}", s)).unwrap_or_else(|| "–".into()),
        mem_mb.map(|m| format!("{:.2} GB", m / 1024.0)).unwrap_or_else(|| "–".into()),
    );
    let js = json!({
        "p": o.precision(), "r": o.recall(), "f1": o.f1(), "covered": r.overall.covered_recall(),
        "en_f1": lang_f1(r, "en"), "es_f1": lang_f1(r, "es"), "pt_f1": lang_f1(r, "pt"),
        "holdout_r": split_recall(r, "holdout"), "holdout_f1": split_f1(r, "holdout"),
        "dev_r": split_recall(r, "dev"), "dev_f1": split_f1(r, "dev"),
        "false_suggestions": fs,
        "false_suggestions_dev": false_suggestions_in(r, fixtures, "dev"),
        "false_suggestions_holdout": false_suggestions_in(r, fixtures, "holdout"),
        "type_accuracy": r.overall.type_accuracy(),
        "s_per_page": secs, "peak_rss_mb": mem_mb,
    });
    (md, js)
}

const SUMMARY_HEAD: &str = "| system | L1+2+3 P / R / F1 | covered R | F1 EN / ES / PT | holdout R / F1 | false suggestions | s/page (L3) | memory |\n|---|---|---:|---|---|---:|---:|---:|\n";

fn variants() -> Vec<(String, PfConfig)> {
    let ops: Vec<(&str, Biases)> = vec![
        ("op0", Biases::default()),
        ("stay1", Biases { transition_bias_background_stay: -1.0, ..Biases::default() }),
        ("stay2", Biases { transition_bias_background_stay: -2.0, ..Biases::default() }),
        ("stay3", Biases { transition_bias_background_stay: -3.0, ..Biases::default() }),
        ("start-1", Biases { transition_bias_background_to_start: -1.0, ..Biases::default() }),
        ("start-2", Biases { transition_bias_background_to_start: -2.0, ..Biases::default() }),
        ("start-3", Biases { transition_bias_background_to_start: -3.0, ..Biases::default() }),
        ("stay+1", Biases { transition_bias_background_stay: 1.0, ..Biases::default() }),
        ("stay+2", Biases { transition_bias_background_stay: 2.0, ..Biases::default() }),
    ];
    let filters: Vec<(&str, fn(&mut PfConfig))> = vec![
        ("app", |_c| {}),
        ("-legal-context", |c| c.legal_context = false),
        ("-legal-words", |c| c.legal_words = false),
        ("-year-only", |c| c.year_only_accounts = false),
        ("-all-legal", |c| {
            c.legal_context = false;
            c.legal_words = false;
            c.year_only_accounts = false;
        }),
        ("+address-digit", |c| c.address_needs_digit = true),
        ("names>=4", |c| c.min_single_name_letters = 4),
        ("names>=5", |c| c.min_single_name_letters = 5),
        ("-untyped", |c| c.drop_untyped_accounts = true),
        ("names>=4 -untyped", |c| {
            c.min_single_name_letters = 4;
            c.drop_untyped_accounts = true;
        }),
        ("+page-wide", |c| c.page_wide_names = true),
    ];
    let mut out = Vec::new();
    for (on, b) in &ops {
        for (fname, f) in &filters {
            let mut c = PfConfig { operating_point: *b, ..PfConfig::default() };
            f(&mut c);
            out.push((format!("{} {}", on, fname), c));
        }
    }
    out
}

#[test]
fn privacy_filter_layer3() {
    if !enabled() {
        eprintln!("skipping: set QVAULT_EVAL_PF=1 and QVAULT_PF_MODEL_DIR (see eval/README.md)");
        return;
    }
    let fixtures = load_fixtures();
    let pages: usize = fixtures.iter().map(|f| f.pages.len()).sum();
    let root = std::env::temp_dir().join(format!("qvault-eval-pf-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let out = out_dir();

    let l12_ents: Vec<Vec<Vec<PiiEntity>>> =
        fixtures.iter().map(|fx| fx.pages.iter().enumerate().map(|(i, p)| layers12(&p.text, i as u32)).collect()).collect();
    let empty: Vec<Vec<Vec<PiiEntity>>> = fixtures.iter().map(|fx| fx.pages.iter().map(|_| vec![]).collect()).collect();
    let l12 = score(&fixtures, &combine(&l12_ents, &empty));

    // ── Privacy Filter: classify every page once, in the app's engine ──
    let svc = pf_service(&root);
    let mem = MemSampler::start();
    let t = Instant::now();
    let session = svc.prepare_privacy_filter().expect("Privacy Filter loads");
    let load_s = t.elapsed().as_secs_f64();
    let cal = *session.calibration();
    let mut model_secs = 0.0;
    let mut decode_secs = 0.0;
    let mut tokens = 0usize;
    let scores: Vec<Vec<PageScores>> = fixtures
        .iter()
        .map(|fx| {
            fx.pages
                .iter()
                .map(|p| {
                    let t = Instant::now();
                    let s = scanner::score_page(&session, session.tokenizer(), &p.text).expect("classify");
                    model_secs += t.elapsed().as_secs_f64();
                    let t = Instant::now();
                    let _ = scanner::detections(&p.text, 0, &s, &cal, &[], &PfConfig::default());
                    decode_secs += t.elapsed().as_secs_f64();
                    tokens += s.tokens.len();
                    s
                })
                .collect()
        })
        .collect();
    let pf_mem_mb = mem.finish() as f64 / 1024.0;
    svc.shutdown();
    let pf_secs = (model_secs + decode_secs) / pages as f64;
    eprintln!(
        "Privacy Filter: load {:.1}s, {:.3} s/page (model {:.3}, tokenize+decode+post {:.3}), {} tokens, peak RSS {:.0} MB",
        load_s,
        pf_secs,
        model_secs / pages as f64,
        decode_secs / pages as f64,
        tokens,
        pf_mem_mb
    );

    // ── variants, replayed on the recorded logits ──
    let mut sweep_md = String::from("| variant | dev L1+2+3 R | dev false sugg. | dev F1 | holdout R | holdout false sugg. | all P / R / F1 | all false sugg. | PF alone P / R |\n|---|---:|---:|---:|---:|---:|---|---:|---|\n");
    let mut sweep_js = serde_json::Map::new();
    let mut results: Vec<(String, PfConfig, Report, Report)> = Vec::new();
    let raw_cfg = PfConfig::raw();
    let mut all = vec![
        ("opf raw (labels mapped, nothing else)".to_string(), raw_cfg.clone()),
        ("app (PfConfig::default)".to_string(), PfConfig::default()),
    ];
    all.extend(variants());
    for (name, cfg) in &all {
        let comb = score(&fixtures, &combine(&l12_ents, &pf_entities(&fixtures, &scores, &cal, &l12_ents, cfg, true)));
        let solo = score(&fixtures, &alone(&pf_entities(&fixtures, &scores, &cal, &l12_ents, cfg, false)));
        let fs_dev = false_suggestions_in(&comb, &fixtures, "dev");
        let fs_ho = false_suggestions_in(&comb, &fixtures, "holdout");
        sweep_md.push_str(&format!(
            "| {} | {} | {} | {} | {} | {} | {} / {} / {} | {} | {} / {} |\n",
            name,
            pct(split_recall(&comb, "dev")),
            fs_dev,
            pct(split_f1(&comb, "dev")),
            pct(split_recall(&comb, "holdout")),
            fs_ho,
            pct(comb.overall.overlap.precision()),
            pct(comb.overall.overlap.recall()),
            pct(comb.overall.overlap.f1()),
            false_suggestions(&comb),
            pct(solo.overall.overlap.precision()),
            pct(solo.overall.overlap.recall()),
        ));
        sweep_js.insert(name.clone(), json!({
            "dev_r": split_recall(&comb, "dev"), "dev_fs": fs_dev, "dev_f1": split_f1(&comb, "dev"),
            "holdout_r": split_recall(&comb, "holdout"), "holdout_fs": fs_ho,
            "all": [comb.overall.overlap.precision(), comb.overall.overlap.recall(), comb.overall.overlap.f1()],
            "false_suggestions": false_suggestions(&comb),
            "alone": [solo.overall.overlap.precision(), solo.overall.overlap.recall(), solo.overall.overlap.f1()],
        }));
        results.push((name.clone(), cfg.clone(), comb, solo));
    }
    // Selection on dev only: among variants whose dev recall is within 0.5
    // points of the best dev recall, the fewest dev false suggestions; ties:
    // higher dev F1.
    // Eligible: the variants of the sweep (not the two reference rows), except
    // those that drop short single-word names: they never cost a label here,
    // but they would hide real short names ("Ana", "Li") in real documents, a
    // risk this set cannot measure.
    let eligible = |r: &&(String, PfConfig, Report, Report)| r.1.min_single_name_letters == 0;
    let best_dev_r = results.iter().skip(2).filter(eligible).map(|r| split_recall(&r.2, "dev")).fold(0.0, f64::max);
    let chosen = results
        .iter()
        .skip(2)
        .filter(eligible)
        .filter(|r| split_recall(&r.2, "dev") >= best_dev_r - 0.005)
        .min_by(|a, b| {
            false_suggestions_in(&a.2, &fixtures, "dev")
                .cmp(&false_suggestions_in(&b.2, &fixtures, "dev"))
                .then(split_f1(&b.2, "dev").partial_cmp(&split_f1(&a.2, "dev")).unwrap())
        })
        .unwrap();
    let app = &results[1];
    eprintln!("dev-chosen variant: {} (best dev recall {:.1})", chosen.0, 100.0 * best_dev_r);

    // ── optional: the language model, and the thorough mode ──
    let mut llm_rows: Vec<(String, Report, f64, f64)> = Vec::new();
    let mut merge_info = String::new();
    if let (Ok(list), Ok(dir)) = (std::env::var("QVAULT_EVAL_MODELS"), std::env::var("QVAC_MODELS_DIR")) {
        for id in list.split(',').map(str::trim).filter(|s| !s.is_empty()) {
            let m = catalog::find(id).unwrap_or_else(|| panic!("{} is not a catalog LLM", id));
            let spec = ModelSpec { id: id.to_string(), path: PathBuf::from(&dir).join(m.file_name), custom: false };
            let (mdir, mid) = install(&spec, &root);
            let app_data = root.join(format!("app-{}", id));
            std::fs::create_dir_all(&app_data).unwrap();
            let lsvc = Arc::new(QvacService::new(&app_data, Some(&mdir), Some(sidecar())));
            lsvc.set_layer3(true, Some(mid.clone())).unwrap();
            let mem = MemSampler::start();
            let sess = lsvc.prepare_layer3().expect("LLM loads");
            let rec = Rec { inner: &sess, answers: RefCell::new(HashMap::new()), seconds: RefCell::new(0.0) };
            let llm_ents: Vec<Vec<Vec<PiiEntity>>> = fixtures
                .iter()
                .enumerate()
                .map(|(fi, fx)| {
                    fx.pages
                        .iter()
                        .enumerate()
                        .map(|(pi, p)| {
                            let ex: Vec<(usize, usize)> = l12_ents[fi][pi].iter().map(|e| (e.start, e.end)).collect();
                            llm_scanner::scan(&rec, &p.text, pi as u32, &ex, None).entities
                        })
                        .collect()
                })
                .collect();
            let llm_mem = mem.finish() as f64 / 1024.0;
            lsvc.shutdown();
            let llm_secs = *rec.seconds.borrow() / pages as f64;
            eprintln!("[{}] {:.2} s/page, peak RSS {:.0} MB", id, llm_secs, llm_mem);
            let with_llm = score(&fixtures, &combine(&l12_ents, &llm_ents));
            llm_rows.push((format!("Layers 1+2 + {}", id), with_llm, llm_secs, llm_mem));
            for (label, cfg) in [("app", &app.1), (chosen.0.as_str(), &chosen.1)] {
                let pf_e = pf_entities(&fixtures, &scores, &cal, &l12_ents, cfg, true);
                let mut stats = scanner::MergeStats::default();
                let merged: Vec<Vec<Vec<PiiEntity>>> = pf_e
                    .into_iter()
                    .zip(llm_ents.clone())
                    .map(|(a, b)| {
                        a.into_iter()
                            .zip(b)
                            .map(|(x, y)| {
                                let (m, st) = scanner::merge_detectors(x, y);
                                stats.agreements += st.agreements;
                                stats.first_replaced += st.first_replaced;
                                stats.second_dropped += st.second_dropped;
                                m
                            })
                            .collect()
                    })
                    .collect();
                let r = score(&fixtures, &combine(&l12_ents, &merged));
                merge_info.push_str(&format!("- PF `{}` + {}: {:?}\n", label, id, stats));
                llm_rows.push((format!("Layers 1+2 + PF `{}` + {} (thorough)", label, id), r, pf_secs + llm_secs, pf_mem_mb.max(llm_mem)));
            }
        }
    }

    // ── report ──
    let mut md = String::from("# Eval: OpenAI Privacy Filter as Layer 3 (in-app pipeline)\n\n");
    md.push_str(&format!(
        "Generated by `QVAULT_EVAL_PF=1 cargo test --release --test eval_harness privacy_filter`. Model `{}` (int8 ONNX) through the QVAC sidecar (`@qvac/onnx` 0.15.1, CPU) and Qvault's tokenizer, decoder and scanner. {} fixtures, {} pages, {} tokens. Load {:.1} s; {:.3} s/page ({:.3} model + {:.3} tokenize/decode/post-process); peak RSS of the sidecar {:.2} GB. Overlap matching; \"false suggestions\" = Layer 3 additions that overlap no label.\n\n",
        catalog::privacy_filter().id, fixtures.len(), pages, tokens, load_s, pf_secs, model_secs / pages as f64, decode_secs / pages as f64, pf_mem_mb / 1024.0
    ));
    md.push_str(&format!("Selection rule (dev split only): among the sweep's variants within 0.5 points of the best dev recall ({}), the fewest dev false suggestions, then the higher dev F1; variants that drop short single-word names are listed but not eligible (they would hide real short names, which this set cannot measure). **Chosen: `{}`.** The app's default (`PfConfig::default`) should equal it.\n\n", pct(best_dev_r), chosen.0));
    md.push_str("## Summary\n\n");
    md.push_str(SUMMARY_HEAD);
    let mut sum_js = serde_json::Map::new();
    let (r0, j0) = summary_row("Layers 1+2 only", &l12, &fixtures, None, None);
    md.push_str(&r0);
    sum_js.insert("layers12".into(), j0);
    for (name, rep, secs, mem) in llm_rows.iter().filter(|r| !r.0.contains("PF")) {
        let (r, j) = summary_row(name, rep, &fixtures, Some(*secs), Some(*mem));
        md.push_str(&r);
        sum_js.insert(name.clone(), j);
    }
    for (label, res) in [("opf raw", &results[0]), ("app default", app), ("dev-chosen", chosen)] {
        let name = format!("Layers 1+2 + Privacy Filter, {} `{}`", label, res.0);
        let (r, j) = summary_row(&name, &res.2, &fixtures, Some(pf_secs), Some(pf_mem_mb));
        md.push_str(&r);
        sum_js.insert(name, j);
    }
    for (name, rep, secs, mem) in llm_rows.iter().filter(|r| r.0.contains("PF")) {
        let (r, j) = summary_row(name, rep, &fixtures, Some(*secs), Some(*mem));
        md.push_str(&r);
        sum_js.insert(name.clone(), j);
    }
    if !merge_info.is_empty() {
        md.push_str("\nMerge (thorough mode, all pages):\n\n");
        md.push_str(&merge_info);
    }
    md.push_str("\n## Variants (replayed on the recorded logits)\n\n");
    md.push_str(&sweep_md);

    // Calibration: precision of the chosen variant's additions by category
    // (dev split only, which is what the confidence values may use).
    md.push_str("\n## Privacy Filter additions to Layers 1+2 by category, dev split (confidence calibration)\n\n| category | added | correct | P |\n|---|---:|---:|---:|\n");
    let pf_chosen = pf_entities(&fixtures, &scores, &cal, &l12_ents, &chosen.1, true);
    let mut by_cat: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    let mut by_conf: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    for (fi, fx) in fixtures.iter().enumerate().filter(|(_, f)| f.split == "dev") {
        for (pi, page) in fx.pages.iter().enumerate() {
            for e in &pf_chosen[fi][pi] {
                let hits: Vec<&Span> = page.spans.iter().filter(|s| s.start < e.end && e.start < s.end).collect();
                if !hits.is_empty() && hits.iter().all(|s| s.optional) {
                    continue;
                }
                let ok = !hits.is_empty() as usize;
                let c = by_cat.entry(e.category.to_string()).or_default();
                c.0 += 1;
                c.1 += ok;
                let c = by_conf.entry(format!("{:.2}", e.confidence)).or_default();
                c.0 += 1;
                c.1 += ok;
            }
        }
    }
    for (k, (n, ok)) in &by_cat {
        md.push_str(&format!("| {} | {} | {} | {} |\n", k, n, ok, pct(ratio(*ok, *n))));
    }
    md.push_str("\n| confidence | added | correct | P |\n|---:|---:|---:|---:|\n");
    for (k, (n, ok)) in &by_conf {
        md.push_str(&format!("| {} | {} | {} | {} |\n", k, n, ok, pct(ratio(*ok, *n))));
    }
    for (label, res) in [("app default", app), ("dev-chosen", chosen)] {
        md.push_str(&format!("\n## False suggestions, {} (dev split first)\n\n", label));
        let mut fps: Vec<&FalsePos> =
            res.2.false_positives.iter().filter(|f| f.layer == app_lib::privacy_filter::LAYER).collect();
        let split_of = |id: &str| fixtures.iter().find(|fx| fx.id == id).map(|fx| fx.split.clone()).unwrap_or_default();
        fps.sort_by_key(|f| (split_of(&f.fixture) != "dev", f.fixture.clone()));
        for f in fps {
            md.push_str(&format!("- [{}] `{}` p{} {}: {:?}\n", split_of(&f.fixture), f.fixture, f.page, f.category, f.text));
        }
    }
    md.push_str("\n## Layers 1+2 + Privacy Filter (dev-chosen) by category\n\n");
    md.push_str(&category_table(&[("L1+2", &l12), ("PF alone", &chosen.3), ("L1+2+PF", &chosen.2)]));
    md.push_str("\n## Layers 1+2 + Privacy Filter (dev-chosen): errors\n\n");
    md.push_str(&lists(&chosen.2, usize::MAX));
    let js = json!({
        "model": catalog::privacy_filter().id, "pages": pages, "tokens": tokens,
        "load_seconds": load_s, "seconds_per_page": pf_secs, "model_seconds_per_page": model_secs / pages as f64,
        "peak_rss_mb": pf_mem_mb, "chosen": chosen.0, "chosen_config": chosen.1,
        "summary": sum_js, "variants": sweep_js,
        "chosen_report": summary_json(&chosen.2),
    });
    write(&out, "privacy-filter-layer3", &md, &js);
    let _ = std::fs::remove_dir_all(&root);
}
