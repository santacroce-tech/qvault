//! Learning from review decisions (§13), measured on the detection fixtures
//! with a simulated review history (see eval/README.md):
//!
//! * `decision_memory_layers12` (CI, no model): each fixture is scanned with
//!   the memory of a reviewer who reviewed *the other* fixtures (split by
//!   document, leave one out; holdout fixtures included). Their Layers 1–2
//!   detections that overlap a required label were approved, those that
//!   overlap no label were rejected. Reports how many false positives of the
//!   scanned document are marked "rejected before", and how many would be
//!   suppressed if the user had put strings on the "Not personal data" list
//!   (every rejected string, or only the suggested candidates). Asserts that
//!   the list never removes a labelled item (a label covered before
//!   suppression is still covered after it) and that the memory never hides
//!   anything.
//! * `fewshot_layer3` (opt-in, `QVAULT_EVAL_FEWSHOT=1`): Layer 3 with
//!   examples from that simulated history in the prompt, against the same
//!   model without them; examples never come from the document being scored.

use std::cell::RefCell;
use std::collections::{BTreeMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use serde::Serialize;
use serde_json::json;

use app_lib::learning::{self, Decision, Example, ExampleCandidate, ExampleMix, Memory};
use app_lib::llm::llm_scanner::{self, Layer3Engine, Layer3Error, PostConfig};
use app_lib::pipeline::types::PiiEntity;
use app_lib::qvac::QvacService;

use super::layer3::{install, l3_preds, sidecar, ModelSpec};
use super::*;

fn to_entity(p: &Pred, page: u32) -> PiiEntity {
    let category = llm_scanner::parse_category(&p.category).expect("known category");
    PiiEntity {
        id: format!("{}:{}:{}", page, p.start, p.end),
        text: p.text.clone(),
        start: p.start,
        end: p.end,
        page,
        replacement: category.replacement_label().into(),
        category,
        layer: p.layer.clone(),
        confidence: p.confidence,
    }
}

/// What a reviewer decides about a prediction: approved (overlaps a
/// required label), rejected (overlaps no label at all), or nothing (it
/// only touches optional labels: the reviewer could go either way).
fn verdict(page: &FixturePage, p: &Pred) -> Option<bool> {
    let hits: Vec<&Span> = page.spans.iter().filter(|s| s.start < p.end && p.start < s.end).collect();
    if hits.iter().any(|s| !s.optional) {
        Some(true)
    } else if hits.is_empty() {
        Some(false)
    } else {
        None
    }
}

/// The simulated review of every prediction of the fixtures in `docs`.
fn history(fixtures: &[Fixture], preds: &Predictions, docs: &dyn Fn(usize) -> bool) -> Vec<Decision> {
    let mut out = Vec::new();
    for (fi, fx) in fixtures.iter().enumerate().filter(|(i, _)| docs(*i)) {
        for (pi, page) in fx.pages.iter().enumerate() {
            for p in &preds[fi][pi] {
                if let Some(approved) = verdict(page, p) {
                    out.push(Decision { document: fx.id.clone(), category: p.category.clone(), text: p.text.clone(), approved });
                }
            }
        }
    }
    out
}

/// A list holding the strings rejected at least `min` times and never
/// approved (`min` = 1: the user added every rejected string; 3: the user
/// added only what Settings suggests).
fn memory_with_list(decisions: &[Decision], min: usize) -> Memory {
    let mut m = Memory::from_decisions(decisions);
    for (cat, key, _) in m.candidates(min) {
        m.add_list_item(&cat, &key, &format!("{}:{}", cat, key));
    }
    m
}

#[derive(Debug, Default, Clone, Serialize)]
pub struct MemoryResult {
    pub scenario: String,
    pub preds: usize,
    pub false_positives: usize,
    pub true_positives: usize,
    /// Marked "rejected before" (still shown, pending).
    pub fp_marked: usize,
    pub tp_marked: usize,
    /// Suppressed by the list.
    pub fp_suppressed: usize,
    /// Suppressed predictions that overlap a required label.
    pub tp_suppressed: usize,
    /// Required labels covered (overlap) before suppression and not after.
    pub labels_lost: usize,
    pub labels_lost_list: Vec<String>,
    pub list_size_mean: f64,
    pub before: (f64, f64, f64),
    pub after: (f64, f64, f64),
}

fn prf(r: &Report) -> (f64, f64, f64) {
    (r.overall.overlap.precision(), r.overall.overlap.recall(), r.overall.overlap.f1())
}

/// Scan every fixture in `test` with a memory built from the fixtures
/// `train(test_index)` selects.
fn memory_scenario(
    name: &str,
    fixtures: &[Fixture],
    preds: &Predictions,
    test: &dyn Fn(usize) -> bool,
    train: &dyn Fn(usize, usize) -> bool,
    list_min: Option<usize>,
) -> MemoryResult {
    let mut r = MemoryResult { scenario: name.into(), ..Default::default() };
    let mut kept_all: Predictions = preds.iter().map(|f| f.iter().map(|_| Vec::new()).collect()).collect();
    let mut before_preds: Predictions = kept_all.clone();
    let mut list_sizes = Vec::new();
    for (fi, fx) in fixtures.iter().enumerate() {
        if !test(fi) {
            continue;
        }
        let hist = history(fixtures, preds, &|o| train(fi, o));
        let mem = match list_min {
            Some(min) => memory_with_list(&hist, min),
            None => Memory::from_decisions(&hist),
        };
        list_sizes.push(mem.list_len() as f64);
        for (pi, page) in fx.pages.iter().enumerate() {
            let ps = &preds[fi][pi];
            before_preds[fi][pi] = ps.clone();
            let ents: Vec<PiiEntity> = ps.iter().map(|p| to_entity(p, pi as u32)).collect();
            let applied = mem.apply(ents);
            assert_eq!(applied.kept.len() + applied.suppressed.len(), ps.len(), "nothing disappears silently");
            let marked: HashSet<&str> = applied.marks.iter().map(|m| m.redaction_id.as_str()).collect();
            for p in ps {
                let id = to_entity(p, pi as u32).id;
                let v = verdict(page, p);
                r.preds += 1;
                match v {
                    Some(true) => r.true_positives += 1,
                    Some(false) => r.false_positives += 1,
                    None => {}
                }
                if marked.contains(id.as_str()) {
                    match v {
                        Some(true) => r.tp_marked += 1,
                        Some(false) => r.fp_marked += 1,
                        None => {}
                    }
                }
            }
            for (e, _) in &applied.suppressed {
                let p = ps.iter().find(|p| to_entity(p, pi as u32).id == e.id).unwrap();
                match verdict(page, p) {
                    Some(true) => r.tp_suppressed += 1,
                    Some(false) => r.fp_suppressed += 1,
                    None => {}
                }
            }
            let kept: Vec<Pred> = applied
                .kept
                .iter()
                .map(|e| ps.iter().find(|p| to_entity(p, pi as u32).id == e.id).unwrap().clone())
                .collect();
            for s in page.spans.iter().filter(|s| !s.optional) {
                let hit = |v: &[Pred]| v.iter().any(|p| p.start < s.end && s.start < p.end);
                if hit(ps) && !hit(&kept) {
                    r.labels_lost += 1;
                    r.labels_lost_list.push(format!("{} p{} {}: {:?}", fx.id, pi + 1, s.category, s.text));
                }
            }
            kept_all[fi][pi] = kept;
        }
    }
    let sub = |p: &Predictions| -> (Vec<Fixture>, Predictions) {
        let idx: Vec<usize> = (0..fixtures.len()).filter(|i| test(*i)).collect();
        (idx.iter().map(|&i| fixtures[i].clone()).collect(), idx.iter().map(|&i| p[i].clone()).collect())
    };
    let (fx_b, p_b) = sub(&before_preds);
    let (_, p_a) = sub(&kept_all);
    r.before = prf(&score(&fx_b, &p_b));
    r.after = prf(&score(&fx_b, &p_a));
    r.list_size_mean = if list_sizes.is_empty() { 0.0 } else { list_sizes.iter().sum::<f64>() / list_sizes.len() as f64 };
    r
}

/// All scenarios on one system's predictions.
pub fn memory_scenarios(fixtures: &[Fixture], preds: &Predictions) -> Vec<MemoryResult> {
    let all = |_: usize| true;
    let loo = |fi: usize, o: usize| o != fi;
    let everything = |_: usize, _: usize| true;
    let holdout = |i: usize| fixtures[i].split == "holdout";
    let from_dev = |_: usize, o: usize| fixtures[o].split != "holdout";
    vec![
        memory_scenario("leave one document out: memory only", fixtures, preds, &all, &loo, None),
        memory_scenario("leave one document out: list = suggested candidates (rejected ≥ 3×)", fixtures, preds, &all, &loo, Some(service_min())),
        memory_scenario("leave one document out: list = every rejected string", fixtures, preds, &all, &loo, Some(1)),
        memory_scenario("stress: list = every string rejected in any document, this one included", fixtures, preds, &all, &everything, Some(1)),
        memory_scenario("dev history → holdout documents: memory only", fixtures, preds, &holdout, &from_dev, None),
        memory_scenario("dev history → holdout documents: list = every rejected string", fixtures, preds, &holdout, &from_dev, Some(1)),
    ]
}

fn service_min() -> usize {
    learning::service::CANDIDATE_MIN_REJECTIONS
}

pub fn memory_md(title: &str, rows: &[MemoryResult]) -> String {
    let mut md = format!("### {}\n\n| scenario | preds | FP | FP marked | FP suppressed | TP marked | TP suppressed | labels lost | list size (mean) | P / R / F1 before | P / R / F1 after suppression |\n|---|---:|---:|---:|---:|---:|---:|---:|---:|---|---|\n", title);
    for r in rows {
        md.push_str(&format!(
            "| {} | {} | {} | {} ({}) | {} ({}) | {} | {} | {} | {:.1} | {} / {} / {} | {} / {} / {} |\n",
            r.scenario, r.preds, r.false_positives,
            r.fp_marked, pct(ratio(r.fp_marked, r.false_positives)),
            r.fp_suppressed, pct(ratio(r.fp_suppressed, r.false_positives)),
            r.tp_marked, r.tp_suppressed, r.labels_lost, r.list_size_mean,
            pct(r.before.0), pct(r.before.1), pct(r.before.2), pct(r.after.0), pct(r.after.1), pct(r.after.2)
        ));
    }
    for r in rows.iter().filter(|r| !r.labels_lost_list.is_empty()) {
        md.push_str(&format!("\nLabels lost in \"{}\":\n\n", r.scenario));
        for l in &r.labels_lost_list {
            md.push_str(&format!("- {}\n", l));
        }
    }
    md
}

/// What was marked / suppressed, for reading the report.
fn examples_of_marks(fixtures: &[Fixture], preds: &Predictions) -> String {
    let mut by_text: BTreeMap<(String, String, bool), usize> = BTreeMap::new();
    for (fi, fx) in fixtures.iter().enumerate() {
        let hist = history(fixtures, preds, &|o| o != fi);
        let mem = Memory::from_decisions(&hist);
        for (pi, page) in fx.pages.iter().enumerate() {
            for p in &preds[fi][pi] {
                if mem.stats(&p.category, &p.text).map(|s| s.rejected_before()).unwrap_or(false) {
                    let tp = verdict(page, p) == Some(true);
                    *by_text.entry((p.category.clone(), learning::key(&p.text), tp)).or_default() += 1;
                }
            }
        }
    }
    let mut rows: Vec<_> = by_text.into_iter().collect();
    rows.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    let mut md = String::from("| category | text (normalised) | correct detection? | times marked |\n|---|---|---|---:|\n");
    for ((c, t, tp), n) in rows {
        md.push_str(&format!("| {} | {} | {} | {} |\n", c, t, if tp { "**yes**" } else { "no" }, n));
    }
    md
}

#[test]
fn decision_memory_layers12() {
    let fixtures = load_fixtures();
    let preds = run_system(&fixtures, layers12);
    let rows = memory_scenarios(&fixtures, &preds);
    let mut md = String::from("# Eval: learning from review decisions, Layers 1–2\n\n");
    md.push_str("Simulated reviewer: detections overlapping a required label approved, detections overlapping no label rejected (optional-only: no decision). Each document is scanned with the memory of the *other* documents. Marked = shown, pending, lower confidence. Suppressed = set aside by the \"Not personal data\" list (counted and restorable in the app). *Labels lost* = required labels some detection covered before suppression and none after.\n\n");
    md.push_str(&memory_md("Layers 1–2", &rows));
    md.push_str("\n### What the memory marks (leave one document out)\n\n");
    md.push_str(&examples_of_marks(&fixtures, &preds));
    write(&out_dir(), "learning-layers12", &md, &json!({ "layers12": rows }));
    for r in &rows {
        eprintln!(
            "learning [{}]: FP {} marked {} suppressed {} | TP marked {} suppressed {} | labels lost {}",
            r.scenario, r.false_positives, r.fp_marked, r.fp_suppressed, r.tp_marked, r.tp_suppressed, r.labels_lost
        );
    }
    // Safety: the list never removes a labelled item on the eval set (in any
    // scenario, including "every rejected string"), and the memory alone
    // never suppresses anything.
    for r in &rows {
        assert_eq!(r.labels_lost, 0, "{}: the list removed labelled items: {:?}", r.scenario, r.labels_lost_list);
        if r.scenario.ends_with("memory only") {
            assert_eq!(r.fp_suppressed + r.tp_suppressed, 0);
        }
    }
    // The memory does something on this set (independent documents share
    // few false positives; see the report for the numbers).
    assert!(rows[0].fp_marked > 0, "the memory marked no false positive");
    // Strictest form of the safety rule: nothing labelled is suppressed at all.
    for r in &rows {
        assert_eq!(r.tp_suppressed, 0, "{}: the list suppressed a detection of labelled data", r.scenario);
    }
}

// ── opt-in: few-shot examples for Layer 3 ────────────────────────────────

/// Times the engine and counts calls (model time only).
struct Timed<'a> {
    inner: &'a dyn Layer3Engine,
    seconds: RefCell<f64>,
    failures: RefCell<usize>,
}

impl Layer3Engine for Timed<'_> {
    fn complete(&self, chunk: &str) -> Result<String, Layer3Error> {
        let t = Instant::now();
        let r = self.inner.complete(chunk);
        *self.seconds.borrow_mut() += t.elapsed().as_secs_f64();
        if r.is_err() {
            *self.failures.borrow_mut() += 1;
        }
        r
    }
}

/// Few-shot candidates from a simulated history (unanimous decisions per
/// category + normalised text, as the app aggregates them).
fn candidates_from(decisions: &[Decision]) -> Vec<ExampleCandidate> {
    let mut agg: BTreeMap<(String, String), (String, usize, usize)> = BTreeMap::new();
    for d in decisions {
        let e = agg.entry((d.category.clone(), learning::key(&d.text))).or_insert_with(|| (d.text.trim().to_string(), 0, 0));
        if d.approved {
            e.2 += 1;
        } else {
            e.1 += 1;
        }
    }
    agg.into_iter()
        .filter_map(|((c, _), (text, rejected, approved))| {
            Some(ExampleCandidate { text, category: llm_scanner::parse_category(&c)?, rejected, approved })
        })
        .collect()
}

#[derive(Debug, Clone, Serialize)]
struct FewShotRow {
    variant: String,
    p: f64,
    r: f64,
    f1: f64,
    covered: f64,
    holdout_r: f64,
    holdout_f1: f64,
    l3_p: f64,
    l3_r: f64,
    seconds_per_page: f64,
    failed_chunks: usize,
    examples_mean: f64,
    prompt_chars_added_mean: f64,
}

/// Control: the same number of "rejected" examples, but generic legal
/// terms that come from no review. If this helps as much as the user's own
/// decisions, the gain is from changing the prompt, not from learning.
fn placebo(n: usize) -> Vec<Example> {
    use app_lib::pipeline::types::PiiCategory::*;
    let all = [("Schedule B", CompanyName), ("Governing Law", PersonName), ("Force Majeure", PersonName), ("Exhibit C", Address), ("Recitals", CompanyName), ("Annex II", Address)];
    all.iter().take(n).map(|(t, c)| Example { text: t.to_string(), category: c.clone(), personal: false }).collect()
}

fn parse_variant(v: &str) -> (ExampleMix, usize) {
    let (mix, n) = v.split_at(v.find(|c: char| c.is_ascii_digit()).unwrap_or(v.len()));
    let mix = match mix {
        "placebo" => ExampleMix::RejectedOnly,
        "mixed" => ExampleMix::Mixed,
        "rejected" => ExampleMix::RejectedOnly,
        other => panic!("unknown few-shot variant {} (mixed<N> | rejected<N> | placebo<N>)", other),
    };
    (mix, n.parse().unwrap_or(6))
}

#[test]
fn fewshot_layer3() {
    if std::env::var("QVAULT_EVAL_FEWSHOT").map(|v| v != "1").unwrap_or(true) {
        eprintln!("skipping: set QVAULT_EVAL_FEWSHOT=1 (and QVAC_MODELS_DIR, QVAULT_EVAL_MODELS) to run");
        return;
    }
    let fixtures = load_fixtures();
    let pages: usize = fixtures.iter().map(|f| f.pages.len()).sum();
    let models_dir = PathBuf::from(std::env::var("QVAC_MODELS_DIR").expect("QVAC_MODELS_DIR"));
    let spec_str = std::env::var("QVAULT_EVAL_MODELS").unwrap_or_else(|_| app_lib::qvac::catalog::default_llm().id.to_string());
    let variants: Vec<String> = std::env::var("QVAULT_EVAL_FEWSHOT_VARIANTS")
        .unwrap_or_else(|_| "mixed6,rejected6".into())
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    let cfg = PostConfig::default();
    let out = out_dir();
    for s in spec_str.split(',').map(str::trim).filter(|s| !s.is_empty()) {
        let spec = match s.split_once('=') {
            Some((id, path)) => ModelSpec { id: id.into(), path: PathBuf::from(path), custom: true },
            None => ModelSpec { id: s.into(), path: models_dir.join(app_lib::qvac::catalog::find(s).expect("catalog id").file_name), custom: false },
        };
        let root = std::env::temp_dir().join(format!("qvault-eval-fewshot-{}", std::process::id()));
        std::fs::create_dir_all(root.join("app")).unwrap();
        let (dir, id) = install(&spec, &root);
        let svc = Arc::new(QvacService::new(&root.join("app"), Some(&dir), Some(sidecar())));
        svc.set_layer3(true, Some(id)).unwrap();
        let mut session = svc.prepare_layer3().expect("layer 3");

        // Baseline: the app's prompt, no examples.
        let timed = Timed { inner: &session, seconds: RefCell::new(0.0), failures: RefCell::new(0) };
        let t0 = Instant::now();
        let (base, _, _, base_failed) = l3_preds(&fixtures, &timed, &cfg, true);
        let base_s = *timed.seconds.borrow() / pages as f64;
        eprintln!("[{}] baseline {:.1}s ({:.2} s/page), failed chunks {}", spec.id, t0.elapsed().as_secs_f64(), base_s, base_failed);
        let mk_row = |name: &str, preds: &Predictions, sec: f64, failed: usize, ex_mean: f64, chars: f64| -> FewShotRow {
            let rep = score(&fixtures, preds);
            let l3_only: Predictions = preds.iter().map(|f| f.iter().map(|p| p.iter().filter(|x| x.layer == "llm").cloned().collect()).collect()).collect();
            let l3 = score(&fixtures, &l3_only);
            let ho = rep.by_split.get("holdout").cloned().unwrap_or_default();
            FewShotRow {
                variant: name.into(),
                p: rep.overall.overlap.precision(),
                r: rep.overall.overlap.recall(),
                f1: rep.overall.overlap.f1(),
                covered: rep.overall.covered_recall(),
                holdout_r: ho.overlap.recall(),
                holdout_f1: ho.overlap.f1(),
                l3_p: l3.overall.overlap.precision(),
                l3_r: l3.overall.overlap.recall(),
                seconds_per_page: sec,
                failed_chunks: failed,
                examples_mean: ex_mean,
                prompt_chars_added_mean: chars,
            }
        };
        let mut rows = vec![mk_row("no examples (app default)", &base, base_s, base_failed, 0.0, 0.0)];
        let mut examples_md = String::new();

        for v in &variants {
            let (mix, n) = parse_variant(v);
            let mut preds: Predictions = Vec::new();
            let (mut secs, mut failed, mut ex_total, mut chars_total) = (0.0, 0usize, 0usize, 0usize);
            let t = Instant::now();
            for (fi, fx) in fixtures.iter().enumerate() {
                // History: the baseline review of every *other* document.
                let hist = history(&fixtures, &base, &|o| o != fi);
                let ex: Vec<Example> = if v.starts_with("placebo") {
                    placebo(n)
                } else {
                    learning::select_examples(&candidates_from(&hist), n, mix)
                };
                ex_total += ex.len();
                chars_total += llm_scanner::examples_paragraph(&ex).chars().count();
                if fi < 3 {
                    examples_md.push_str(&format!("- `{}` / `{}`: {}\n", v, fx.id, ex.iter().map(|e| format!("{}{:?} ({})", if e.personal { "+" } else { "−" }, e.text, e.category.to_string())).collect::<Vec<_>>().join(", ")));
                }
                session.examples = ex;
                let timed = Timed { inner: &session, seconds: RefCell::new(0.0), failures: RefCell::new(0) };
                let (p, _, _, f) = l3_preds(std::slice::from_ref(fx), &timed, &cfg, true);
                secs += *timed.seconds.borrow();
                failed += f;
                preds.push(p.into_iter().next().unwrap());
            }
            session.examples.clear();
            let row = mk_row(v, &preds, secs / pages as f64, failed, ex_total as f64 / fixtures.len() as f64, chars_total as f64 / fixtures.len() as f64);
            eprintln!(
                "[{}] {}: {:.0}s, P {} R {} F1 {} (baseline F1 {}), {:.2} s/page",
                spec.id, v, t.elapsed().as_secs_f64(), pct(row.p), pct(row.r), pct(row.f1), pct(rows[0].f1), row.seconds_per_page
            );
            rows.push(row);
        }
        svc.shutdown();

        // The memory and the list on top of Layers 1–3 (baseline answers).
        let mem_rows = memory_scenarios(&fixtures, &base);

        let mut md = format!("# Eval: few-shot examples from review decisions, Layer 3 `{}`\n\n", spec.id);
        md.push_str(&format!("{} documents, {} pages. Examples for each document come only from the simulated review of the *other* documents (baseline Layers 1+2+3 detections: approved if they overlap a required label, rejected if they overlap none). Prompt `{}`, temp 0, seed 42. s/page = Layer 3 model time.\n\n", fixtures.len(), pages, llm_scanner::DEFAULT_PROMPT.id()));
        md.push_str("| variant | L1+2+3 P | R | F1 | covered R | holdout R | holdout F1 | L3 additions P / R | s/page | failed chunks | examples (mean) | prompt chars added (mean) |\n|---|---:|---:|---:|---:|---:|---:|---|---:|---:|---:|---:|\n");
        for r in &rows {
            md.push_str(&format!(
                "| {} | {} | {} | {} | {} | {} | {} | {} / {} | {:.2} | {} | {:.1} | {:.0} |\n",
                r.variant, pct(r.p), pct(r.r), pct(r.f1), pct(r.covered), pct(r.holdout_r), pct(r.holdout_f1),
                pct(r.l3_p), pct(r.l3_r), r.seconds_per_page, r.failed_chunks, r.examples_mean, r.prompt_chars_added_mean
            ));
        }
        md.push_str("\nExamples given (first three documents; + approved, − rejected):\n\n");
        md.push_str(&examples_md);
        md.push('\n');
        md.push_str(&memory_md("Decision memory and list on top of Layers 1+2+3 (baseline answers)", &mem_rows));
        write(&out, &format!("learning-fewshot-{}", spec.id), &md, &json!({ "rows": rows, "memory_layers123": mem_rows }));
        let _ = std::fs::remove_dir_all(&root);
    }
}
