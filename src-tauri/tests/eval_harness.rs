//! PII detection evaluation harness (see eval/README.md).
//!
//! * `layers_1_2_against_baseline` (always runs, CI): validates every fixture
//!   in `eval/fixtures/*.json`, scores Layer 1, Layer 2 and Layers 1+2, writes
//!   a Markdown + JSON report, and fails if recall (or precision) drops below
//!   `eval/baseline.json` minus its tolerance. Deterministic, no models.
//! * `layer3_models` (opt-in, `QVAULT_EVAL_L3=1`): runs Layer 3 through the
//!   real QVAC sidecar for one or more models, N runs each, and reports
//!   per-model / per-language / per-category numbers, run-to-run variance,
//!   seconds per page, peak RAM and the assessFit verdict.
//!
//! Matching (all three are reported; **overlap** is the headline):
//! * exact   – prediction and label have the same boundaries (ignoring
//!             surrounding whitespace and trailing `.,;:`);
//! * overlap – they share at least one byte;
//! * covered – (recall only) every letter/digit of the label lies inside
//!             some prediction, of any category: the redaction view.
//! Matching ignores categories (a name redacted as a company is still
//! redacted); category agreement is reported separately as "type accuracy".
//! Labels marked `optional` never count as misses, and predictions that only
//! hit optional labels are neither true nor false positives.

#![allow(dead_code)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use app_lib::pipeline::heuristic_scanner::HeuristicScanner;
use app_lib::pipeline::regex_scanner::RegexScanner;
use app_lib::pipeline::types::PiiEntity;

pub const JURISDICTIONS: [&str; 5] = ["global", "us", "eu", "br", "de"];

// ── fixtures ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Span {
    pub start: usize,
    pub end: usize,
    pub category: String,
    pub text: String,
    #[serde(default)]
    pub optional: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FixturePage {
    pub text: String,
    pub spans: Vec<Span>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Fixture {
    pub id: String,
    pub lang: String,
    #[serde(default = "dev_split")]
    pub split: String,
    pub kind: String,
    pub description: String,
    pub source: String,
    pub pages: Vec<FixturePage>,
}

fn dev_split() -> String {
    "dev".into()
}

pub fn eval_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join("eval")
}

pub fn load_fixtures() -> Vec<Fixture> {
    let dir = eval_dir().join("fixtures");
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("{}: {}", dir.display(), e))
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().map_or(false, |e| e == "json"))
        .collect();
    files.sort();
    files
        .iter()
        .map(|p| {
            let fx: Fixture = serde_json::from_slice(&std::fs::read(p).unwrap())
                .unwrap_or_else(|e| panic!("{}: {}", p.display(), e));
            fx
        })
        .collect()
}

pub const CATEGORIES: [&str; 23] = [
    "PersonName", "Email", "PhoneNumber", "Address", "DateOfBirth", "Date", "MonetaryAmount",
    "SocialSecurityNumber", "NationalId", "PassportNumber", "DriversLicense", "BankAccount",
    "CreditCard", "IBAN", "TaxId", "CaseNumber", "CourtFileNumber", "ClientId", "IpAddress",
    "Url", "LicensePlate", "MedicalRecordNumber", "CompanyName",
];

/// Spans must reproduce their text exactly (UTF-8 byte offsets), sit on
/// char boundaries, use a known category, and be sorted without overlaps.
pub fn validate(fx: &Fixture) -> Vec<String> {
    let mut errs = Vec::new();
    if !["en", "es", "pt"].contains(&fx.lang.as_str()) {
        errs.push(format!("{}: unknown lang {}", fx.id, fx.lang));
    }
    for (n, p) in fx.pages.iter().enumerate() {
        let mut prev_end = 0usize;
        for s in &p.spans {
            let w = format!("{} p{} {}..{}", fx.id, n + 1, s.start, s.end);
            if s.start >= s.end || s.end > p.text.len() {
                errs.push(format!("{}: out of range", w));
                continue;
            }
            if !p.text.is_char_boundary(s.start) || !p.text.is_char_boundary(s.end) {
                errs.push(format!("{}: not on a char boundary", w));
                continue;
            }
            if p.text[s.start..s.end] != s.text {
                errs.push(format!("{}: text is {:?}, label says {:?}", w, &p.text[s.start..s.end], s.text));
            }
            if !CATEGORIES.contains(&s.category.as_str()) {
                errs.push(format!("{}: unknown category {}", w, s.category));
            }
            if s.start < prev_end {
                errs.push(format!("{}: overlaps or is out of order", w));
            }
            prev_end = s.end;
        }
    }
    errs
}

// ── predictions and scoring ──────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct Pred {
    pub start: usize,
    pub end: usize,
    pub category: String,
    pub text: String,
    pub layer: String,
    pub confidence: f64,
}

impl From<&PiiEntity> for Pred {
    fn from(e: &PiiEntity) -> Self {
        Pred {
            start: e.start,
            end: e.end,
            category: e.category.to_string(),
            text: e.text.clone(),
            layer: e.layer.clone(),
            confidence: e.confidence,
        }
    }
}

/// Predictions for every (fixture, page), in fixture order.
pub type Predictions = Vec<Vec<Vec<Pred>>>;

const TRIM: &[char] = &[' ', '\t', '\n', '\r', '.', ',', ';', ':'];

fn trimmed(text: &str, start: usize, end: usize) -> (usize, usize) {
    let s = &text[start..end];
    let lead = s.len() - s.trim_start_matches(TRIM).len();
    let tail = s.trim_end_matches(TRIM).len();
    if tail <= lead {
        return (start, end);
    }
    (start + lead, start + tail)
}

fn overlaps(a: (usize, usize), b: (usize, usize)) -> bool {
    a.0 < b.1 && b.0 < a.1
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq)]
pub struct Counts {
    pub tp: usize,
    pub fp: usize,
    #[serde(rename = "fn")]
    pub fn_: usize,
}

impl Counts {
    pub fn precision(&self) -> f64 {
        ratio(self.tp, self.tp + self.fp)
    }
    pub fn add(&mut self, o: &Counts) {
        self.tp += o.tp;
        self.fp += o.fp;
        self.fn_ += o.fn_;
    }
}

/// Recall uses label counts (tp_labels / labels); precision uses prediction
/// counts. They differ when one prediction covers two labels or vice versa.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Score {
    pub labels: usize,
    pub labels_found: usize,
    pub preds: usize,
    pub preds_correct: usize,
}

impl Score {
    pub fn recall(&self) -> f64 {
        ratio(self.labels_found, self.labels)
    }
    pub fn precision(&self) -> f64 {
        ratio(self.preds_correct, self.preds)
    }
    pub fn f1(&self) -> f64 {
        let (p, r) = (self.precision(), self.recall());
        if p + r == 0.0 { 0.0 } else { 2.0 * p * r / (p + r) }
    }
    pub fn add(&mut self, o: &Score) {
        self.labels += o.labels;
        self.labels_found += o.labels_found;
        self.preds += o.preds;
        self.preds_correct += o.preds_correct;
    }
}

pub fn ratio(a: usize, b: usize) -> f64 {
    if b == 0 { 1.0 } else { a as f64 / b as f64 }
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Breakdown {
    pub exact: Score,
    pub overlap: Score,
    /// labels / labels fully covered (letters and digits) by predictions
    pub covered: (usize, usize),
    /// labels found (overlap) / of those, found with the right category
    pub typed: (usize, usize),
}

impl Breakdown {
    fn add(&mut self, o: &Breakdown) {
        self.exact.add(&o.exact);
        self.overlap.add(&o.overlap);
        self.covered.0 += o.covered.0;
        self.covered.1 += o.covered.1;
        self.typed.0 += o.typed.0;
        self.typed.1 += o.typed.1;
    }
    pub fn covered_recall(&self) -> f64 {
        ratio(self.covered.1, self.covered.0)
    }
    pub fn type_accuracy(&self) -> f64 {
        ratio(self.typed.1, self.typed.0)
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Miss {
    pub fixture: String,
    pub page: usize,
    pub category: String,
    pub text: String,
    /// "missed" (nothing overlaps) or "partial" (overlap, not fully covered)
    pub kind: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct FalsePos {
    pub fixture: String,
    pub page: usize,
    pub category: String,
    pub text: String,
    pub layer: String,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Report {
    pub overall: Breakdown,
    pub by_category: BTreeMap<String, Breakdown>,
    pub by_lang: BTreeMap<String, Breakdown>,
    pub by_split: BTreeMap<String, Breakdown>,
    pub false_negatives: Vec<Miss>,
    pub false_positives: Vec<FalsePos>,
}

fn alnum_covered(text: &str, span: (usize, usize), preds: &[(usize, usize)]) -> bool {
    text[span.0..span.1].char_indices().all(|(i, c)| {
        let at = span.0 + i;
        !c.is_alphanumeric() || preds.iter().any(|p| p.0 <= at && at < p.1)
    })
}

pub fn score(fixtures: &[Fixture], preds: &Predictions) -> Report {
    let mut rep = Report::default();
    for (fi, fx) in fixtures.iter().enumerate() {
        for (pi, page) in fx.pages.iter().enumerate() {
            let text = &page.text;
            let ps = &preds[fi][pi];
            let p_ranges: Vec<(usize, usize)> = ps.iter().map(|p| (p.start, p.end)).collect();
            let p_trim: Vec<(usize, usize)> = ps.iter().map(|p| trimmed(text, p.start, p.end)).collect();
            // labels → recall
            for s in &page.spans {
                if s.optional {
                    continue;
                }
                let g = (s.start, s.end);
                let gt = trimmed(text, s.start, s.end);
                let hit_overlap: Vec<usize> = (0..ps.len()).filter(|&k| overlaps(p_ranges[k], g)).collect();
                let hit_exact = (0..ps.len()).any(|k| p_trim[k] == gt);
                let covered = alnum_covered(text, g, &p_ranges);
                let typed = hit_overlap.iter().any(|&k| ps[k].category == s.category);
                let mut b = Breakdown::default();
                b.exact.labels = 1;
                b.overlap.labels = 1;
                b.covered.0 = 1;
                if hit_exact {
                    b.exact.labels_found = 1;
                }
                if !hit_overlap.is_empty() {
                    b.overlap.labels_found = 1;
                    b.typed = (1, typed as usize);
                }
                if covered {
                    b.covered.1 = 1;
                }
                rep.overall.add(&b);
                rep.by_category.entry(s.category.clone()).or_default().add(&b);
                rep.by_lang.entry(fx.lang.clone()).or_default().add(&b);
                rep.by_split.entry(fx.split.clone()).or_default().add(&b);
                if hit_overlap.is_empty() || !covered {
                    rep.false_negatives.push(Miss {
                        fixture: fx.id.clone(),
                        page: pi + 1,
                        category: s.category.clone(),
                        text: s.text.clone(),
                        kind: if hit_overlap.is_empty() { "missed" } else { "partial" }.into(),
                    });
                }
            }
            // predictions → precision
            for (k, p) in ps.iter().enumerate() {
                let req_o = page.spans.iter().any(|s| !s.optional && overlaps((s.start, s.end), p_ranges[k]));
                let opt_o = page.spans.iter().any(|s| s.optional && overlaps((s.start, s.end), p_ranges[k]));
                let req_e = page.spans.iter().any(|s| !s.optional && trimmed(text, s.start, s.end) == p_trim[k]);
                let opt_e = page.spans.iter().any(|s| s.optional && trimmed(text, s.start, s.end) == p_trim[k]);
                let mut b = Breakdown::default();
                if req_o || !opt_o {
                    b.overlap.preds = 1;
                    b.overlap.preds_correct = req_o as usize;
                }
                if req_e || !(opt_e || (opt_o && !req_o)) {
                    b.exact.preds = 1;
                    b.exact.preds_correct = req_e as usize;
                }
                rep.overall.add(&b);
                rep.by_category.entry(p.category.clone()).or_default().add(&b);
                rep.by_lang.entry(fx.lang.clone()).or_default().add(&b);
                rep.by_split.entry(fx.split.clone()).or_default().add(&b);
                if !req_o && !opt_o {
                    rep.false_positives.push(FalsePos {
                        fixture: fx.id.clone(),
                        page: pi + 1,
                        category: p.category.clone(),
                        text: p.text.clone(),
                        layer: p.layer.clone(),
                    });
                }
            }
        }
    }
    rep
}

// ── systems ──────────────────────────────────────────────────────────────

pub fn run_system<F: Fn(&str, u32) -> Vec<PiiEntity>>(fixtures: &[Fixture], f: F) -> Predictions {
    fixtures
        .iter()
        .map(|fx| {
            fx.pages
                .iter()
                .enumerate()
                .map(|(i, p)| f(&p.text, i as u32).iter().map(Pred::from).collect())
                .collect()
        })
        .collect()
}

pub fn layer1(text: &str, page: u32) -> Vec<PiiEntity> {
    RegexScanner::new(&JURISDICTIONS).scan(text, page)
}

pub fn layer2(text: &str, page: u32) -> Vec<PiiEntity> {
    HeuristicScanner::new().scan(text, page, &[])
}

pub fn layers12(text: &str, page: u32) -> Vec<PiiEntity> {
    app_lib::pipeline::run_pipeline_no_llm(text, page, &JURISDICTIONS)
}

// ── report output ────────────────────────────────────────────────────────

pub fn pct(x: f64) -> String {
    format!("{:.1}", 100.0 * x)
}

pub fn summary_json(r: &Report) -> Value {
    let b = |b: &Breakdown| {
        json!({
            "labels": b.overlap.labels,
            "overlap": {"precision": b.overlap.precision(), "recall": b.overlap.recall(), "f1": b.overlap.f1(),
                        "preds": b.overlap.preds, "preds_correct": b.overlap.preds_correct, "labels_found": b.overlap.labels_found},
            "exact": {"precision": b.exact.precision(), "recall": b.exact.recall(), "f1": b.exact.f1()},
            "covered_recall": b.covered_recall(),
            "type_accuracy": b.type_accuracy(),
        })
    };
    json!({
        "overall": b(&r.overall),
        "by_lang": r.by_lang.iter().map(|(k, v)| (k.clone(), b(v))).collect::<serde_json::Map<_, _>>(),
        "by_split": r.by_split.iter().map(|(k, v)| (k.clone(), b(v))).collect::<serde_json::Map<_, _>>(),
        "by_category": r.by_category.iter().map(|(k, v)| (k.clone(), b(v))).collect::<serde_json::Map<_, _>>(),
        "false_negatives": r.false_negatives,
        "false_positives": r.false_positives,
    })
}

pub fn row(name: &str, b: &Breakdown) -> String {
    format!(
        "| {} | {} | {} | {} / {} / {} | {} / {} / {} | {} | {} |\n",
        name,
        b.overlap.labels,
        b.overlap.preds,
        pct(b.overlap.precision()), pct(b.overlap.recall()), pct(b.overlap.f1()),
        pct(b.exact.precision()), pct(b.exact.recall()), pct(b.exact.f1()),
        pct(b.covered_recall()),
        pct(b.type_accuracy()),
    )
}

pub const TABLE_HEAD: &str = "| | labels | preds | overlap P / R / F1 | exact P / R / F1 | covered R | type acc. |\n|---|---:|---:|---|---|---:|---:|\n";

pub fn category_table(systems: &[(&str, &Report)]) -> String {
    let mut s = String::from("| category | labels |");
    for (n, _) in systems {
        s.push_str(&format!(" {} P / R |", n));
    }
    s.push_str("\n|---|---:|");
    for _ in systems {
        s.push_str("---|");
    }
    s.push('\n');
    let mut cats: Vec<&str> = CATEGORIES.to_vec();
    cats.sort();
    for c in cats {
        let labels = systems
            .iter()
            .map(|(_, r)| r.by_category.get(c).map(|b| b.overlap.labels).unwrap_or(0))
            .max()
            .unwrap_or(0);
        let any_pred = systems.iter().any(|(_, r)| r.by_category.get(c).map_or(false, |b| b.overlap.preds > 0));
        if labels == 0 && !any_pred {
            continue;
        }
        s.push_str(&format!("| {} | {} |", c, labels));
        for (_, r) in systems {
            match r.by_category.get(c) {
                Some(b) => s.push_str(&format!(
                    " {} / {} |",
                    if b.overlap.preds == 0 { "–".into() } else { pct(b.overlap.precision()) },
                    if b.overlap.labels == 0 { "–".into() } else { pct(b.overlap.recall()) }
                )),
                None => s.push_str(" – / 0.0 |"),
            }
        }
        s.push('\n');
    }
    s
}

pub fn lists(r: &Report, limit: usize) -> String {
    let mut s = String::new();
    s.push_str(&format!("False negatives ({}):\n\n", r.false_negatives.len()));
    for m in r.false_negatives.iter().take(limit) {
        s.push_str(&format!("- `{}` p{} {} ({}): {:?}\n", m.fixture, m.page, m.category, m.kind, m.text));
    }
    s.push_str(&format!("\nFalse positives ({}):\n\n", r.false_positives.len()));
    for f in r.false_positives.iter().take(limit) {
        s.push_str(&format!("- `{}` p{} {} [{}]: {:?}\n", f.fixture, f.page, f.category, f.layer, f.text));
    }
    s
}

pub fn out_dir() -> PathBuf {
    let d = std::env::var_os("QVAULT_EVAL_OUT")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target").join("eval"));
    std::fs::create_dir_all(&d).unwrap();
    d
}

pub fn write(dir: &Path, name: &str, md: &str, js: &Value) {
    std::fs::write(dir.join(format!("{}.md", name)), md).unwrap();
    std::fs::write(dir.join(format!("{}.json", name)), serde_json::to_string_pretty(js).unwrap() + "\n").unwrap();
    eprintln!("eval report: {}", dir.join(format!("{}.md", name)).display());
}

fn fixture_stats(fixtures: &[Fixture]) -> String {
    let mut by_lang: BTreeMap<&str, (usize, usize, usize, usize)> = BTreeMap::new();
    for fx in fixtures {
        let e = by_lang.entry(fx.lang.as_str()).or_default();
        e.0 += 1;
        e.1 += fx.pages.len();
        e.2 += fx.pages.iter().map(|p| p.spans.iter().filter(|s| !s.optional).count()).sum::<usize>();
        e.3 += fx.pages.iter().map(|p| p.spans.iter().filter(|s| s.optional).count()).sum::<usize>();
    }
    let mut s = String::from("| lang | fixtures | pages | labels | optional |\n|---|---:|---:|---:|---:|\n");
    for (l, (f, p, n, o)) in by_lang {
        s.push_str(&format!("| {} | {} | {} | {} | {} |\n", l, f, p, n, o));
    }
    s
}

// ── CI test: Layers 1–2 vs. the committed baseline ───────────────────────

#[derive(Debug, Serialize, Deserialize)]
struct Baseline {
    system: String,
    /// Allowed absolute drop in any recall figure.
    recall_tolerance: f64,
    /// Allowed absolute drop in overall overlap precision.
    precision_tolerance: f64,
    overlap_recall: f64,
    covered_recall: f64,
    exact_recall: f64,
    overlap_precision: f64,
    by_category_overlap_recall: BTreeMap<String, f64>,
    by_lang_overlap_recall: BTreeMap<String, f64>,
}

fn baseline_from(r: &Report) -> Baseline {
    Baseline {
        system: "layers 1+2".into(),
        recall_tolerance: 0.005,
        precision_tolerance: 0.02,
        overlap_recall: r.overall.overlap.recall(),
        covered_recall: r.overall.covered_recall(),
        exact_recall: r.overall.exact.recall(),
        overlap_precision: r.overall.overlap.precision(),
        by_category_overlap_recall: r
            .by_category
            .iter()
            .filter(|(_, b)| b.overlap.labels > 0)
            .map(|(k, b)| (k.clone(), round4(b.overlap.recall())))
            .collect(),
        by_lang_overlap_recall: r.by_lang.iter().map(|(k, b)| (k.clone(), round4(b.overlap.recall()))).collect(),
    }
}

fn round4(x: f64) -> f64 {
    (x * 10000.0).round() / 10000.0
}

#[test]
fn layers_1_2_against_baseline() {
    let fixtures = load_fixtures();
    assert!(fixtures.len() >= 10, "expected the eval fixtures in eval/fixtures");
    let errs: Vec<String> = fixtures.iter().flat_map(validate).collect();
    assert!(errs.is_empty(), "invalid fixtures:\n{}", errs.join("\n"));
    let langs: std::collections::BTreeSet<&str> = fixtures.iter().map(|f| f.lang.as_str()).collect();
    assert_eq!(langs.into_iter().collect::<Vec<_>>(), vec!["en", "es", "pt"]);
    let labelled: std::collections::BTreeSet<&str> = fixtures
        .iter()
        .flat_map(|f| f.pages.iter().flat_map(|p| p.spans.iter().filter(|s| !s.optional).map(|s| s.category.as_str())))
        .collect();
    for c in CATEGORIES {
        assert!(labelled.contains(c), "no fixture labels category {}", c);
    }

    let l1 = score(&fixtures, &run_system(&fixtures, layer1));
    let l2 = score(&fixtures, &run_system(&fixtures, layer2));
    let l12 = score(&fixtures, &run_system(&fixtures, layers12));

    let mut md = String::from("# Eval: Layers 1–2 (deterministic)\n\n");
    md.push_str("Generated by `cargo test --test eval_harness layers_1_2_against_baseline`. Headline = overlap matching; see eval/README.md.\n\n");
    md.push_str(&fixture_stats(&fixtures));
    md.push_str("\n## Overall\n\n");
    md.push_str(TABLE_HEAD);
    md.push_str(&row("Layer 1 (regex)", &l1.overall));
    md.push_str(&row("Layer 2 (heuristics)", &l2.overall));
    md.push_str(&row("Layers 1+2", &l12.overall));
    md.push_str("\n## Layers 1+2 by language and split\n\n");
    md.push_str("`holdout` fixtures were written after the Layer 1 changes and never used to tune any rule.\n\n");
    md.push_str(TABLE_HEAD);
    for (l, b) in &l12.by_lang {
        md.push_str(&row(l, b));
    }
    for (s, b) in &l12.by_split {
        md.push_str(&row(s, b));
    }
    for (name, r) in [("Layer 1", &l1), ("Layer 2", &l2)] {
        for (s, b) in &r.by_split {
            md.push_str(&row(&format!("{} – {}", name, s), b));
        }
    }
    md.push_str("\n## By category (overlap)\n\n");
    md.push_str(&category_table(&[("L1", &l1), ("L2", &l2), ("L1+2", &l12)]));
    md.push_str("\n## Layers 1+2 errors\n\n");
    md.push_str(&lists(&l12, usize::MAX));
    let js = json!({
        "layer1": summary_json(&l1),
        "layer2": summary_json(&l2),
        "layers12": summary_json(&l12),
    });
    write(&out_dir(), "layers12", &md, &js);
    eprintln!(
        "layers 1+2: overlap P {} R {} F1 {} | exact R {} | covered R {}",
        pct(l12.overall.overlap.precision()),
        pct(l12.overall.overlap.recall()),
        pct(l12.overall.overlap.f1()),
        pct(l12.overall.exact.recall()),
        pct(l12.overall.covered_recall())
    );

    let base_path = eval_dir().join("baseline.json");
    let current = baseline_from(&l12);
    if std::env::var("QVAULT_EVAL_UPDATE_BASELINE").map(|v| v == "1").unwrap_or(false) {
        std::fs::write(&base_path, serde_json::to_string_pretty(&current).unwrap() + "\n").unwrap();
        eprintln!("baseline updated: {}", base_path.display());
        return;
    }
    let base: Baseline = serde_json::from_slice(&std::fs::read(&base_path).expect("eval/baseline.json"))
        .expect("valid baseline");
    let mut fails = Vec::new();
    let tol = base.recall_tolerance;
    for (name, now, was) in [
        ("overlap recall", current.overlap_recall, base.overlap_recall),
        ("covered recall", current.covered_recall, base.covered_recall),
        ("exact recall", current.exact_recall, base.exact_recall),
    ] {
        if now < was - tol {
            fails.push(format!("{}: {} < baseline {}", name, pct(now), pct(was)));
        }
    }
    if current.overlap_precision < base.overlap_precision - base.precision_tolerance {
        fails.push(format!(
            "overlap precision: {} < baseline {}",
            pct(current.overlap_precision),
            pct(base.overlap_precision)
        ));
    }
    for (group, now_map, base_map) in [
        ("category", &current.by_category_overlap_recall, &base.by_category_overlap_recall),
        ("language", &current.by_lang_overlap_recall, &base.by_lang_overlap_recall),
    ] {
        for (k, was) in base_map {
            let now = now_map.get(k).copied().unwrap_or(0.0);
            if now < was - tol {
                fails.push(format!("{} {}: recall {} < baseline {}", group, k, pct(now), pct(*was)));
            }
        }
    }
    assert!(
        fails.is_empty(),
        "Layers 1-2 eval regressed against eval/baseline.json:\n  {}\n(If the change is intended, re-run with QVAULT_EVAL_UPDATE_BASELINE=1 and commit the new baseline.)",
        fails.join("\n  ")
    );
}

#[test]
fn scoring_rules() {
    let page = FixturePage {
        text: "Call Ann Lee or Bob at 555-0100 today.".into(),
        spans: vec![
            Span { start: 5, end: 12, category: "PersonName".into(), text: "Ann Lee".into(), optional: false },
            Span { start: 16, end: 19, category: "PersonName".into(), text: "Bob".into(), optional: true },
            Span { start: 23, end: 31, category: "PhoneNumber".into(), text: "555-0100".into(), optional: false },
        ],
    };
    let fx = Fixture {
        id: "t".into(),
        lang: "en".into(),
        split: "dev".into(),
        kind: "t".into(),
        description: "t".into(),
        source: "t".into(),
        pages: vec![page],
    };
    assert!(validate(&fx).is_empty());
    let p = |s: usize, e: usize, c: &str| Pred {
        start: s,
        end: e,
        category: c.into(),
        text: fx.pages[0].text[s..e].into(),
        layer: "x".into(),
        confidence: 1.0,
    };
    // "Ann" partial (overlap, not exact, not covered), Bob optional (ignored),
    // phone exact with a wrong type, "today" a false positive.
    let preds = vec![vec![vec![p(5, 8, "PersonName"), p(16, 19, "PersonName"), p(23, 31, "Date"), p(32, 37, "Date")]]];
    let r = score(&[fx.clone()], &preds);
    assert_eq!(r.overall.overlap.labels, 2);
    assert_eq!(r.overall.overlap.labels_found, 2);
    assert_eq!(r.overall.exact.labels_found, 1);
    assert_eq!(r.overall.covered, (2, 1));
    assert_eq!(r.overall.typed, (2, 1));
    assert_eq!(r.overall.overlap.preds, 3, "the optional-only prediction is ignored");
    assert_eq!(r.overall.overlap.preds_correct, 2);
    assert_eq!(r.false_positives.len(), 1);
    assert_eq!(r.false_negatives.len(), 1);
    assert_eq!(r.false_negatives[0].kind, "partial");
    // A broken span is reported.
    let mut bad = fx.clone();
    bad.pages[0].spans[0].end = 11;
    assert_eq!(validate(&bad).len(), 1);
}
