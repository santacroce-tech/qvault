//! One page through Privacy Filter: tokenize, score in the sidecar, decode,
//! then turn the model's spans into Qvault detections.
//!
//! Post-processing (every step can be switched off in [`PfConfig`] so the
//! eval harness can measure it; `Default` is what the app uses):
//!
//! 1. **labels → categories**: `private_person` → PersonName, `private_email`
//!    → Email, `private_phone` → PhoneNumber, `private_address` → Address,
//!    `private_url` → Url, `private_date` → Date (DateOfBirth after a birth
//!    label, Layer 1's rule), `secret` → ClientId, `account_number` → typed
//!    by [`retype`](super::retype) (validators, then context; else
//!    BankAccount, untyped);
//! 2. **filters**: Layer 3's plausibility rules and legal-term stop-list
//!    (shared with the language model, `llm_scanner`);
//! 3. **Layers 1–2 first**: a span that overlaps a Layer 1–2 detection is
//!    dropped (as for the language model);
//! 4. **confidence** by category, from the precision of Privacy Filter's
//!    additions to Layers 1–2 on the eval's dev split (§15).
//!
//! Company names, money amounts and case numbers are not in the model's
//! label set; they stay with Layers 1–2 (and the language model when the
//! thorough pass is on).

use serde::Serialize;

use super::decoder::{self, Biases, Decode, RawSpan, NUM_CLASSES};
use super::retype::{self, TypedBy};
use super::tokenizer::{O200kTokenizer, Token};
use super::LAYER;
use crate::llm::llm_scanner::{self, Layer3Error, MatchOpts};
use crate::pipeline::types::{PiiCategory, PiiEntity};

/// Tokens per model call: opf's CPU window (fixed windows, no overlap).
pub const MAX_WINDOW: usize = 4096;

/// Runs the token classifier (in the app: `@qvac/onnx` in the sidecar).
pub trait PfEngine {
    /// Logits for one window of at most [`MAX_WINDOW`] token ids, row-major
    /// `[ids.len(), NUM_CLASSES]`.
    fn classify(&self, ids: &[u32]) -> Result<Vec<f32>, Layer3Error>;
}

/// A page's tokens and the model's logits for them.
#[derive(Debug, Clone, Default)]
pub struct PageScores {
    pub tokens: Vec<Token>,
    pub logits: Vec<f32>,
}

/// Tokenize and score one page (windows of [`MAX_WINDOW`] tokens, as opf).
pub fn score_page(engine: &dyn PfEngine, tok: &O200kTokenizer, text: &str) -> Result<PageScores, Layer3Error> {
    let tokens = tok.encode(text);
    let mut logits = Vec::with_capacity(tokens.len() * NUM_CLASSES);
    for window in tokens.chunks(MAX_WINDOW) {
        let ids: Vec<u32> = window.iter().map(|t| t.id).collect();
        let out = engine.classify(&ids)?;
        if out.len() != ids.len() * NUM_CLASSES {
            return Err(Layer3Error::Chunk(format!(
                "the model returned {} scores for {} tokens",
                out.len(),
                ids.len()
            )));
        }
        logits.extend(out);
    }
    Ok(PageScores { tokens, logits })
}

/// Switches for every post-processing step (eval ablations). `Default` is
/// the app's configuration.
#[derive(Debug, Clone, Serialize)]
pub struct PfConfig {
    /// Added to the published calibration (`viterbi_calibration.json`, all
    /// zero in the pinned revision): Qvault's operating point.
    pub operating_point: Biases,
    /// Per-token argmax instead of Viterbi (eval only).
    pub argmax: bool,
    pub retype_accounts: bool,
    pub birth_dates: bool,
    /// Layer 3's per-category plausibility rules.
    pub plausibility: bool,
    /// Layer 3's legal-term stop-list (names, companies, addresses).
    pub stoplist: bool,
    /// Drop a person "name" that is a single word with fewer than this many
    /// letters (0 = off).
    pub min_single_name_letters: usize,
    /// Drop `account_number` spans that could not be typed.
    pub drop_untyped_accounts: bool,
    /// Also flag the other occurrences on the page of an accepted person name.
    pub page_wide_names: bool,
}

impl Default for PfConfig {
    fn default() -> Self {
        PfConfig {
            operating_point: Biases::default(),
            argmax: false,
            retype_accounts: true,
            birth_dates: true,
            plausibility: true,
            stoplist: true,
            min_single_name_letters: 0,
            drop_untyped_accounts: false,
            page_wide_names: false,
        }
    }
}

impl PfConfig {
    /// opf's own output, mapped to categories and nothing else (reference).
    pub fn raw() -> Self {
        PfConfig {
            operating_point: Biases::default(),
            argmax: false,
            retype_accounts: false,
            birth_dates: false,
            plausibility: false,
            stoplist: false,
            min_single_name_letters: 0,
            drop_untyped_accounts: false,
            page_wide_names: false,
        }
    }
}

/// What happened to the model's spans on a page (summed by the harness).
#[derive(Debug, Default, Clone, Serialize, PartialEq)]
pub struct PfStats {
    pub tokens: usize,
    pub spans: usize,
    pub implausible: usize,
    pub stoplisted: usize,
    pub short_names: usize,
    pub accounts_by_validator: usize,
    pub accounts_by_context: usize,
    pub accounts_untyped: usize,
    pub untyped_dropped: usize,
    pub page_wide_hits: usize,
    pub covered_by_layers_1_2: usize,
    pub kept: usize,
}

impl PfStats {
    pub fn add(&mut self, o: &PfStats) {
        self.tokens += o.tokens;
        self.spans += o.spans;
        self.implausible += o.implausible;
        self.stoplisted += o.stoplisted;
        self.short_names += o.short_names;
        self.accounts_by_validator += o.accounts_by_validator;
        self.accounts_by_context += o.accounts_by_context;
        self.accounts_untyped += o.accounts_untyped;
        self.untyped_dropped += o.untyped_dropped;
        self.page_wide_hits += o.page_wide_hits;
        self.covered_by_layers_1_2 += o.covered_by_layers_1_2;
        self.kept += o.kept;
    }
}

#[derive(Debug, Default)]
pub struct PfReport {
    pub entities: Vec<PiiEntity>,
    pub stats: PfStats,
}

/// Confidence of a Privacy Filter detection, by category and (for
/// identifiers) by how it was typed. Calibrated on the precision of Privacy
/// Filter's additions to Layers 1–2 on the eval's dev split (§15.5); always
/// below Layer 1's 0.95, nothing is auto-applied.
pub fn confidence(category: &PiiCategory, typed: Option<TypedBy>) -> f64 {
    use PiiCategory::*;
    match typed {
        Some(TypedBy::Validator) => return 0.80,
        Some(TypedBy::Context) => return 0.65,
        Some(TypedBy::Untyped) => return 0.40,
        None => {}
    }
    match category {
        PersonName => 0.80,
        Email | PhoneNumber | Url => 0.75,
        DateOfBirth => 0.75,
        Date => 0.60,
        Address => 0.55,
        _ => 0.50,
    }
}

fn base_category(label: &str) -> PiiCategory {
    match label {
        "private_person" => PiiCategory::PersonName,
        "private_email" => PiiCategory::Email,
        "private_phone" => PiiCategory::PhoneNumber,
        "private_address" => PiiCategory::Address,
        "private_url" => PiiCategory::Url,
        "private_date" => PiiCategory::Date,
        "account_number" => PiiCategory::BankAccount,
        _ => PiiCategory::ClientId, // "secret": passwords, keys, tokens
    }
}

/// The model's spans for a scored page, decoded at the configured
/// operating point on top of `calibration`.
pub fn raw_spans(text: &str, scores: &PageScores, calibration: &Biases, cfg: &PfConfig) -> Vec<RawSpan> {
    let how = if cfg.argmax { Decode::Argmax } else { Decode::Viterbi(calibration.plus(&cfg.operating_point)) };
    decoder::decode(text, &scores.tokens, &scores.logits, &how)
}

/// Qvault detections from a scored page. `existing` are the Layer 1–2
/// ranges on the page.
pub fn detections(
    text: &str,
    page: u32,
    scores: &PageScores,
    calibration: &Biases,
    existing: &[(usize, usize)],
    cfg: &PfConfig,
) -> PfReport {
    let spans = raw_spans(text, scores, calibration, cfg);
    let mut rep = PfReport::default();
    rep.stats.tokens = scores.tokens.len();
    rep.stats.spans = spans.len();
    let mut found: Vec<(usize, usize, PiiCategory, Option<TypedBy>)> = Vec::new();
    for s in &spans {
        let span_text = &text[s.start..s.end];
        let mut category = base_category(s.label_name());
        let mut typed = None;
        if category == PiiCategory::BankAccount && cfg.retype_accounts {
            let (c, how) = retype::retype_account(span_text, retype::context_before(text, s.start));
            match how {
                TypedBy::Validator => rep.stats.accounts_by_validator += 1,
                TypedBy::Context => rep.stats.accounts_by_context += 1,
                TypedBy::Untyped => rep.stats.accounts_untyped += 1,
            }
            if how == TypedBy::Untyped && cfg.drop_untyped_accounts {
                rep.stats.untyped_dropped += 1;
                continue;
            }
            category = c;
            typed = Some(how);
        }
        if category == PiiCategory::Date && cfg.birth_dates && crate::pipeline::regex_scanner::after_dob_label(text, s.start) {
            category = PiiCategory::DateOfBirth;
        }
        if cfg.plausibility && !llm_scanner::plausible(span_text, &category, 0) {
            rep.stats.implausible += 1;
            continue;
        }
        if cfg.stoplist && llm_scanner::stoplisted(span_text, &category) {
            rep.stats.stoplisted += 1;
            continue;
        }
        if category == PiiCategory::PersonName && cfg.min_single_name_letters > 0 && span_text.split_whitespace().count() == 1 {
            let letters = span_text.chars().filter(|c| c.is_alphabetic()).count();
            if letters < cfg.min_single_name_letters {
                rep.stats.short_names += 1;
                continue;
            }
        }
        found.push((s.start, s.end, category, typed));
    }
    if cfg.page_wide_names {
        let names: Vec<String> = found
            .iter()
            .filter(|f| f.2 == PiiCategory::PersonName)
            .map(|f| text[f.0..f.1].to_string())
            .collect();
        for name in names {
            for (s, e) in llm_scanner::find_occurrences_with(text, &name, MatchOpts::default()) {
                if !found.iter().any(|f| s < f.1 && f.0 < e) {
                    found.push((s, e, PiiCategory::PersonName, None));
                    rep.stats.page_wide_hits += 1;
                }
            }
        }
        found.sort_by_key(|f| f.0);
    }
    for (start, end, category, typed) in found {
        if existing.iter().any(|(es, ee)| start < *ee && *es < end) {
            rep.stats.covered_by_layers_1_2 += 1;
            continue;
        }
        rep.entities.push(PiiEntity {
            id: uuid::Uuid::new_v4().to_string(),
            text: text[start..end].to_string(),
            start,
            end,
            page,
            replacement: category.replacement_label().to_string(),
            confidence: confidence(&category, typed),
            category,
            layer: LAYER.to_string(),
        });
    }
    rep.stats.kept = rep.entities.len();
    rep
}

/// Score and post-process one page.
pub fn scan(
    engine: &dyn PfEngine,
    tok: &O200kTokenizer,
    calibration: &Biases,
    text: &str,
    page: u32,
    existing: &[(usize, usize)],
    cfg: &PfConfig,
) -> Result<PfReport, Layer3Error> {
    if text.trim().is_empty() {
        return Ok(PfReport::default());
    }
    let scores = score_page(engine, tok, text)?;
    Ok(detections(text, page, &scores, calibration, existing, cfg))
}

/// What [`merge_detectors`] did.
#[derive(Debug, Default, Clone, Copy, PartialEq, Serialize)]
pub struct MergeStats {
    /// Language-model spans that overlap a Privacy Filter span and are not
    /// longer than it (dropped).
    pub second_dropped: usize,
    /// Privacy Filter spans replaced by a longer overlapping language-model
    /// span.
    pub first_replaced: usize,
    /// Overlaps (both detectors found the item).
    pub agreements: usize,
}

/// Merge the detections of two Layer 3 detectors (Privacy Filter first, the
/// language model second) the way Layer 3 is merged with Layers 1–2, except
/// that between the two detectors the longer span wins: a second-detector
/// span that overlaps first-detector spans replaces them when it is longer
/// than each of them, and is dropped otherwise. Each kept detection keeps its
/// own `detection_layer`, so the review shows which detector found it.
pub fn merge_detectors(first: Vec<PiiEntity>, second: Vec<PiiEntity>) -> (Vec<PiiEntity>, MergeStats) {
    let mut stats = MergeStats::default();
    let mut kept: Vec<PiiEntity> = first;
    for e in second {
        let overlapping: Vec<usize> = kept
            .iter()
            .enumerate()
            .filter(|(_, k)| e.start < k.end && k.start < e.end)
            .map(|(i, _)| i)
            .collect();
        if overlapping.is_empty() {
            kept.push(e);
            continue;
        }
        stats.agreements += 1;
        let len = e.end - e.start;
        if overlapping.iter().all(|&i| len > kept[i].end - kept[i].start) {
            stats.first_replaced += overlapping.len();
            for i in overlapping.into_iter().rev() {
                kept.remove(i);
            }
            kept.push(e);
        } else {
            stats.second_dropped += 1;
        }
    }
    kept.sort_by_key(|e| e.start);
    (kept, stats)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Engine whose "model" labels whole words by a fixed rule, so the
    /// pipeline can be tested without the real model: tokens are words.
    fn page_scores(text: &str, labels: &[(&str, usize)]) -> PageScores {
        // one token per whitespace-separated word, offsets exact
        let mut tokens = Vec::new();
        let mut logits = Vec::new();
        for w in llm_scanner::tokenize_words(text) {
            tokens.push(Token { id: 0, start: w.start, end: w.end });
            let mut row = vec![0f32; NUM_CLASSES];
            let class = labels.iter().find(|(t, _)| *t == w.text).map(|(_, c)| *c).unwrap_or(0);
            row[class] = 10.0;
            logits.extend(row);
        }
        PageScores { tokens, logits }
    }

    const PERSON_B: usize = 17;
    const PERSON_E: usize = 19;
    const PERSON_S: usize = 20;
    const ACCOUNT_S: usize = 4;
    const DATE_S: usize = 12;

    #[test]
    fn maps_types_filters_and_skips_layers_1_2() {
        let text = "Ms. Dana Holloway (born 04/02/1981) paid the Tenant from account 0012-339-8841; passport AB1234567 and Ref 777.";
        let scores = page_scores(
            text,
            &[
                ("Dana", PERSON_B),
                ("Holloway", PERSON_E),
                ("04/02/1981)", DATE_S),
                ("Tenant", PERSON_S),
                ("0012-339-8841;", ACCOUNT_S),
                ("AB1234567", ACCOUNT_S),
                ("777.", ACCOUNT_S),
            ],
        );
        let acct = text.find("0012").unwrap();
        let existing = vec![(acct, acct + 13)]; // Layer 1 found the account
        let r = detections(text, 3, &scores, &Biases::default(), &existing, &PfConfig::default());
        let got: Vec<(String, PiiCategory, f64)> =
            r.entities.iter().map(|e| (e.text.clone(), e.category.clone(), e.confidence)).collect();
        assert_eq!(
            got,
            vec![
                ("Dana Holloway".to_string(), PiiCategory::PersonName, 0.80),
                ("04/02/1981)".to_string(), PiiCategory::DateOfBirth, 0.75),
                ("AB1234567".to_string(), PiiCategory::PassportNumber, 0.65),
                ("777.".to_string(), PiiCategory::ClientId, 0.65),
            ]
        );
        assert!(r.entities.iter().all(|e| e.layer == "privacy-filter" && e.page == 3));
        assert_eq!(r.stats.stoplisted, 1, "'Tenant' is a legal role");
        assert_eq!(r.stats.covered_by_layers_1_2, 1);
        assert_eq!((r.stats.accounts_by_context, r.stats.accounts_untyped), (3, 0));
        // the raw configuration keeps the model's own labels
        let raw = detections(text, 3, &scores, &Biases::default(), &[], &PfConfig::raw());
        assert_eq!(raw.entities.len(), 6);
        assert!(raw.entities.iter().filter(|e| e.category == PiiCategory::BankAccount).count() == 3);
        assert!(raw.entities.iter().any(|e| e.text == "Tenant"));
    }

    #[test]
    fn options_drop_untyped_short_names_and_map_names_page_wide() {
        let text = "Sue met Ann Lee today. Later ANN LEE left; 777 kept.";
        let scores = page_scores(text, &[("Sue", PERSON_S), ("Ann", PERSON_B), ("Lee", PERSON_E), ("777", ACCOUNT_S)]);
        let cfg = PfConfig { min_single_name_letters: 4, drop_untyped_accounts: true, page_wide_names: true, ..PfConfig::default() };
        let r = detections(text, 0, &scores, &Biases::default(), &[], &cfg);
        let got: Vec<&str> = r.entities.iter().map(|e| e.text.as_str()).collect();
        // the model's span, then the other occurrence found by page-wide mapping
        assert_eq!(got, vec!["Ann Lee", "ANN LEE"]);
        assert_eq!((r.stats.short_names, r.stats.untyped_dropped, r.stats.page_wide_hits), (1, 1, 1));
    }

    #[test]
    fn longer_span_wins_between_detectors() {
        let e = |s: usize, t: usize, layer: &str| PiiEntity {
            id: format!("{}-{}", layer, s),
            text: String::new(),
            start: s,
            end: t,
            page: 0,
            replacement: String::new(),
            confidence: 0.8,
            category: PiiCategory::PersonName,
            layer: layer.into(),
        };
        let pf = vec![e(0, 5, "privacy-filter"), e(10, 20, "privacy-filter"), e(30, 34, "privacy-filter")];
        let llm = vec![e(2, 4, "llm"), e(8, 25, "llm"), e(40, 45, "llm"), e(30, 34, "llm")];
        let (m, st) = merge_detectors(pf, llm);
        let got: Vec<(usize, usize, &str)> = m.iter().map(|x| (x.start, x.end, x.layer.as_str())).collect();
        assert_eq!(got, vec![(0, 5, "privacy-filter"), (8, 25, "llm"), (30, 34, "privacy-filter"), (40, 45, "llm")]);
        assert_eq!(st, MergeStats { second_dropped: 2, first_replaced: 1, agreements: 3 });
    }

    struct FailingEngine;
    impl PfEngine for FailingEngine {
        fn classify(&self, _ids: &[u32]) -> Result<Vec<f32>, Layer3Error> {
            Ok(vec![0.0; 3])
        }
    }

    #[test]
    fn wrong_logit_count_fails_the_page() {
        let table_vocab = {
            // minimal byte-level tokenizer: every byte is a token
            let mut v = serde_json::Map::new();
            let printable = |b: u32| (0x21..=0x7e).contains(&b) || (0xa1..=0xac).contains(&b) || (0xae..=0xff).contains(&b);
            let mut n = 0;
            for b in 0u32..256 {
                let c = if printable(b) { char::from_u32(b).unwrap() } else { n += 1; char::from_u32(255 + n).unwrap() };
                v.insert(c.to_string(), serde_json::json!(b));
            }
            v
        };
        let json = serde_json::json!({"model": {"type": "BPE", "vocab": table_vocab}});
        let tok = O200kTokenizer::from_json(json.to_string().as_bytes()).unwrap();
        let r = scan(&FailingEngine, &tok, &Biases::default(), "Ann Lee", 0, &[], &PfConfig::default());
        assert!(matches!(r, Err(Layer3Error::Chunk(_))));
        // blank pages never call the engine
        assert!(scan(&FailingEngine, &tok, &Biases::default(), "  \n ", 0, &[], &PfConfig::default()).unwrap().entities.is_empty());
    }
}
