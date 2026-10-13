//! Learning from review decisions (replaces the abandoned Phase 5 LoRA
//! fine-tuning; see docs/qvac-integration.md §13).
//!
//! Three local mechanisms, none of which trains a model:
//!
//! * **Decision memory.** Every approve / reject in the review sidebar is
//!   remembered as (normalised text, category, detection layer). On later
//!   scans a detection whose text and category were rejected before (and
//!   never approved) is *marked*: it stays in the review list, pending like
//!   everything else, but gets a lower confidence, sorts after the other
//!   findings and can be rejected in one click. Nothing is hidden or decided
//!   automatically.
//! * **"Not personal data" list.** Strings the user explicitly put on the
//!   list (from a rejected finding, from Settings, or by importing a file)
//!   are suppressed in later scans for their category. Every suppression is
//!   counted, stored with its position and shown in the review sidebar,
//!   where it can be restored.
//! * **Few-shot examples** for Layer 3 built from the user's own decisions
//!   ([`select_examples`]). Measured with the eval harness before deciding
//!   whether the app uses them (§13).
//!
//! This module is pure (no database); [`service`] stores and loads.

pub mod service;

use std::collections::{BTreeMap, HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::entities::normalize::fold;
use crate::pipeline::types::{PiiCategory, PiiEntity};

/// Characters stripped from both ends of a string before comparing it.
const EDGE: &[char] = &[
    ' ', '\t', '\n', '\r', '.', ',', ';', ':', '"', '\'', '“', '”', '‘', '’', '«', '»', '(', ')', '[', ']',
];

/// Normalised form used to compare decisions and list items: accents and
/// case folded, whitespace collapsed, surrounding punctuation and quotes
/// removed. "  EFFECTIVE   Date," and "Effective Date" are the same key;
/// "Effective Dates" is not. Only whole strings are compared, never parts.
pub fn key(text: &str) -> String {
    let folded = fold(text);
    let collapsed = folded.split_whitespace().collect::<Vec<_>>().join(" ");
    collapsed.trim_matches(EDGE).to_string()
}

/// How the user decided about one string (category + key) so far.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DecisionStats {
    /// Occurrences rejected (each review of an occurrence counts once).
    pub rejected: usize,
    /// Documents in which it was rejected.
    pub rejected_documents: usize,
    /// Occurrences approved.
    pub approved: usize,
}

impl DecisionStats {
    /// "Rejected before": rejected at least once and never approved. A
    /// string the user approved anywhere is never marked.
    pub fn rejected_before(&self) -> bool {
        self.rejected > 0 && self.approved == 0
    }
}

/// Confidence of a detection that was rejected before: half of what the
/// layer gave it, at most 0.35 (below every layer's lowest, so it sorts
/// last when ordered by confidence).
pub fn lowered_confidence(c: f64) -> f64 {
    ((c * 0.5).min(0.35) * 100.0).round() / 100.0
}

/// What the scan knows from earlier reviews.
#[derive(Debug, Clone, Default)]
pub struct Memory {
    decisions: HashMap<(String, String), DecisionStats>,
    /// (category, key) → list item id.
    list: HashMap<(String, String), String>,
}

/// One review decision, for building a [`Memory`] in memory (the eval
/// harness; the app aggregates in SQL).
#[derive(Debug, Clone)]
pub struct Decision {
    pub document: String,
    pub category: String,
    pub text: String,
    pub approved: bool,
}

/// A detection that was rejected before.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Mark {
    pub redaction_id: String,
    pub stats: DecisionStats,
    pub original_confidence: f64,
}

/// Result of [`Memory::apply`].
#[derive(Debug, Default)]
pub struct Applied {
    /// Detections that go to the review list (marked ones included, with
    /// their lowered confidence).
    pub kept: Vec<PiiEntity>,
    pub marks: Vec<Mark>,
    /// Detections suppressed by the "Not personal data" list, with the id of
    /// the list item that matched.
    pub suppressed: Vec<(PiiEntity, String)>,
}

impl Memory {
    pub fn new() -> Self {
        Self::default()
    }

    /// Aggregate individual decisions (counts per string, documents).
    pub fn from_decisions(decisions: &[Decision]) -> Self {
        let mut m = Memory::new();
        let mut docs: HashMap<(String, String), HashSet<&str>> = HashMap::new();
        for d in decisions {
            let k = (d.category.clone(), key(&d.text));
            if k.1.is_empty() {
                continue;
            }
            let s = m.decisions.entry(k.clone()).or_default();
            if d.approved {
                s.approved += 1;
            } else {
                s.rejected += 1;
                docs.entry(k).or_default().insert(d.document.as_str());
            }
        }
        for (k, set) in docs {
            if let Some(s) = m.decisions.get_mut(&k) {
                s.rejected_documents = set.len();
            }
        }
        m
    }

    pub fn set_stats(&mut self, category: &str, norm_key: &str, stats: DecisionStats) {
        self.decisions.insert((category.to_string(), norm_key.to_string()), stats);
    }

    pub fn add_list_item(&mut self, category: &str, text: &str, id: &str) {
        let k = key(text);
        if !k.is_empty() {
            self.list.insert((category.to_string(), k), id.to_string());
        }
    }

    pub fn list_len(&self) -> usize {
        self.list.len()
    }

    pub fn is_empty(&self) -> bool {
        self.decisions.is_empty() && self.list.is_empty()
    }

    pub fn stats(&self, category: &str, text: &str) -> Option<&DecisionStats> {
        self.decisions.get(&(category.to_string(), key(text)))
    }

    /// The list item suppressing `text` in `category`, if any.
    pub fn listed(&self, category: &str, text: &str) -> Option<&str> {
        self.list.get(&(category.to_string(), key(text))).map(|s| s.as_str())
    }

    /// Mark detections that were rejected before and set aside the ones on
    /// the "Not personal data" list. Order is preserved.
    pub fn apply(&self, entities: Vec<PiiEntity>) -> Applied {
        let mut out = Applied::default();
        for mut e in entities {
            let cat = e.category.to_string();
            if let Some(id) = self.listed(&cat, &e.text) {
                let id = id.to_string();
                out.suppressed.push((e, id));
                continue;
            }
            if let Some(s) = self.stats(&cat, &e.text).filter(|s| s.rejected_before()) {
                out.marks.push(Mark { redaction_id: e.id.clone(), stats: s.clone(), original_confidence: e.confidence });
                e.confidence = lowered_confidence(e.confidence);
            }
            out.kept.push(e);
        }
        out
    }

    /// Strings rejected at least `min_rejections` times and never approved
    /// that are not on the list yet (suggested for the list in Settings).
    pub fn candidates(&self, min_rejections: usize) -> Vec<(String, String, DecisionStats)> {
        let mut v: Vec<(String, String, DecisionStats)> = self
            .decisions
            .iter()
            .filter(|(k, s)| s.rejected_before() && s.rejected >= min_rejections && !self.list.contains_key(*k))
            .map(|((c, k), s)| (c.clone(), k.clone(), s.clone()))
            .collect();
        v.sort_by(|a, b| b.2.rejected.cmp(&a.2.rejected).then(a.0.cmp(&b.0)).then(a.1.cmp(&b.1)));
        v
    }
}

// ── "Not personal data" list file format ─────────────────────────────────

pub const LIST_FORMAT: &str = "qvault-not-personal-data";
pub const LIST_VERSION: u32 = 1;
/// Limits for imported files.
pub const LIST_MAX_ITEMS: usize = 10_000;
pub const ITEM_MAX_CHARS: usize = 200;
pub const NOTE_MAX_CHARS: usize = 500;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListFileItem {
    pub text: String,
    pub category: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListFile {
    pub format: String,
    pub version: u32,
    #[serde(default)]
    pub exported_at: Option<String>,
    pub items: Vec<ListFileItem>,
}

impl ListFile {
    pub fn new(items: Vec<ListFileItem>) -> Self {
        ListFile {
            format: LIST_FORMAT.into(),
            version: LIST_VERSION,
            exported_at: Some(chrono::Utc::now().to_rfc3339()),
            items,
        }
    }
}

/// Check one item: known category, 1–200 characters after trimming, no
/// control characters, an optional note of at most 500. Returns the cleaned
/// item or the reason it is rejected.
pub fn validate_item(item: &ListFileItem) -> Result<ListFileItem, String> {
    let text = item.text.split_whitespace().collect::<Vec<_>>().join(" ");
    if text.is_empty() || key(&text).is_empty() {
        return Err("empty text".into());
    }
    if text.chars().count() > ITEM_MAX_CHARS {
        return Err(format!("text longer than {} characters", ITEM_MAX_CHARS));
    }
    if text.chars().any(|c| c.is_control()) {
        return Err("text contains control characters".into());
    }
    if crate::llm::llm_scanner::parse_category(&item.category).is_none() {
        return Err(format!("unknown category {:?}", item.category));
    }
    let note = item.note.as_deref().map(str::trim).filter(|n| !n.is_empty()).map(String::from);
    if let Some(n) = &note {
        if n.chars().count() > NOTE_MAX_CHARS || n.chars().any(|c| c.is_control() && c != '\n') {
            return Err("invalid note".into());
        }
    }
    Ok(ListFileItem { text, category: item.category.clone(), note })
}

/// One line of an import preview.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InvalidItem {
    /// 1-based position in the file's `items`.
    pub index: usize,
    pub text: String,
    pub reason: String,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportPreview {
    /// Items that would be added.
    pub new_items: Vec<ListFileItem>,
    /// Items already on the list (same category and normalised text).
    pub already_listed: usize,
    /// Repeated inside the file.
    pub duplicates_in_file: usize,
    pub invalid: Vec<InvalidItem>,
}

/// Parse and validate a list file against the current list (category, key
/// pairs). Nothing is applied: the user sees the preview first.
pub fn preview_import(json: &str, existing: &HashSet<(String, String)>) -> Result<ImportPreview, String> {
    let file: ListFile = serde_json::from_str(json).map_err(|e| format!("not a Qvault list file: {}", e))?;
    if file.format != LIST_FORMAT {
        return Err(format!("not a Qvault list file (format {:?})", file.format));
    }
    if file.version != LIST_VERSION {
        return Err(format!("unsupported list version {}", file.version));
    }
    if file.items.len() > LIST_MAX_ITEMS {
        return Err(format!("too many items ({}; at most {})", file.items.len(), LIST_MAX_ITEMS));
    }
    let mut p = ImportPreview::default();
    let mut seen: HashSet<(String, String)> = HashSet::new();
    for (i, it) in file.items.iter().enumerate() {
        match validate_item(it) {
            Err(reason) => p.invalid.push(InvalidItem {
                index: i + 1,
                text: it.text.chars().take(80).collect(),
                reason,
            }),
            Ok(clean) => {
                let k = (clean.category.clone(), key(&clean.text));
                if existing.contains(&k) {
                    p.already_listed += 1;
                } else if !seen.insert(k) {
                    p.duplicates_in_file += 1;
                } else {
                    p.new_items.push(clean);
                }
            }
        }
    }
    Ok(p)
}

// ── few-shot examples for Layer 3 ────────────────────────────────────────

/// A string the user decided about, offered to the model as an example.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Example {
    pub text: String,
    pub category: PiiCategory,
    /// true: approved (personal data); false: rejected (not personal data).
    pub personal: bool,
}

/// A candidate example with how often it was decided.
#[derive(Debug, Clone)]
pub struct ExampleCandidate {
    pub text: String,
    pub category: PiiCategory,
    pub rejected: usize,
    pub approved: usize,
}

/// Which examples to offer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum ExampleMix {
    /// Half rejected, half approved (rejected first when odd).
    Mixed,
    /// Only rejected strings (what not to extract).
    RejectedOnly,
}

/// Longest example string offered to the model.
pub const EXAMPLE_MAX_CHARS: usize = 60;

/// Pick up to `max` short examples: unanimous decisions only (rejected and
/// never approved, or approved and never rejected), most frequent first,
/// spread over categories (round-robin), deterministic.
pub fn select_examples(cands: &[ExampleCandidate], max: usize, mix: ExampleMix) -> Vec<Example> {
    let usable = |c: &&ExampleCandidate| {
        let n = c.text.trim().chars().count();
        (2..=EXAMPLE_MAX_CHARS).contains(&n) && !c.text.contains('\n')
    };
    let pick = |personal: bool, n: usize| -> Vec<Example> {
        let mut by_cat: BTreeMap<String, Vec<&ExampleCandidate>> = BTreeMap::new();
        for c in cands.iter().filter(usable).filter(|c| {
            if personal { c.approved > 0 && c.rejected == 0 } else { c.rejected > 0 && c.approved == 0 }
        }) {
            by_cat.entry(c.category.to_string()).or_default().push(c);
        }
        for v in by_cat.values_mut() {
            v.sort_by(|a, b| {
                let (x, y) = if personal { (a.approved, b.approved) } else { (a.rejected, b.rejected) };
                y.cmp(&x).then(a.text.cmp(&b.text))
            });
            v.dedup_by(|a, b| key(&a.text) == key(&b.text));
        }
        // categories with the most decisions first
        let mut cats: Vec<(String, usize)> = by_cat
            .iter()
            .map(|(k, v)| (k.clone(), v.iter().map(|c| if personal { c.approved } else { c.rejected }).sum()))
            .collect();
        cats.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        let mut out = Vec::new();
        let mut round = 0;
        while out.len() < n {
            let mut any = false;
            for (c, _) in &cats {
                if out.len() >= n {
                    break;
                }
                if let Some(x) = by_cat[c].get(round) {
                    out.push(Example { text: x.text.trim().to_string(), category: x.category.clone(), personal });
                    any = true;
                }
            }
            if !any {
                break;
            }
            round += 1;
        }
        out
    };
    match mix {
        ExampleMix::RejectedOnly => pick(false, max),
        ExampleMix::Mixed => {
            let mut rej = pick(false, max - max / 2);
            let app = pick(true, max - rej.len());
            rej.extend(app);
            rej
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ent(id: &str, text: &str, cat: PiiCategory, conf: f64) -> PiiEntity {
        PiiEntity {
            id: id.into(),
            text: text.into(),
            start: 0,
            end: text.len(),
            page: 0,
            replacement: cat.replacement_label().into(),
            category: cat,
            layer: "heuristic".into(),
            confidence: conf,
        }
    }

    fn dec(doc: &str, cat: &str, text: &str, approved: bool) -> Decision {
        Decision { document: doc.into(), category: cat.into(), text: text.into(), approved }
    }

    #[test]
    fn keys_fold_case_accents_spacing_and_edges() {
        assert_eq!(key("  EFFECTIVE   Date,"), "effective date");
        assert_eq!(key("“Código Civil”"), "codigo civil");
        assert_ne!(key("Effective Dates"), key("Effective Date"));
        assert_eq!(key(" .,; "), "");
    }

    #[test]
    fn marks_rejected_before_and_never_hides_them() {
        let m = Memory::from_decisions(&[
            dec("d1", "PersonName", "Effective Date", false),
            dec("d2", "PersonName", "effective date", false),
            dec("d2", "PersonName", "Effective Date", false),
            dec("d1", "PersonName", "Santos", false),
            dec("d3", "PersonName", "Santos", true),
        ]);
        let s = m.stats("PersonName", "EFFECTIVE DATE").unwrap();
        assert_eq!((s.rejected, s.rejected_documents, s.approved), (3, 2, 0));
        let a = m.apply(vec![
            ent("a", "Effective Date", PiiCategory::PersonName, 0.6),
            ent("b", "Effective Date", PiiCategory::CompanyName, 0.7),
            ent("c", "Santos", PiiCategory::PersonName, 0.6),
        ]);
        assert_eq!(a.kept.len(), 3, "memory never hides anything");
        assert!(a.suppressed.is_empty());
        assert_eq!(a.marks.len(), 1, "other category and approved-once strings are not marked");
        assert_eq!(a.marks[0].redaction_id, "a");
        assert_eq!(a.kept[0].confidence, 0.30);
        assert_eq!(a.kept[1].confidence, 0.7);
        assert_eq!(lowered_confidence(0.95), 0.35);
    }

    #[test]
    fn list_suppresses_only_whole_strings_of_its_category() {
        let mut m = Memory::new();
        m.add_list_item("PersonName", "Effective Date", "i1");
        let a = m.apply(vec![
            ent("a", "EFFECTIVE  DATE", PiiCategory::PersonName, 0.6),
            ent("b", "Effective Date", PiiCategory::CompanyName, 0.6),
            ent("c", "Effective Dates", PiiCategory::PersonName, 0.6),
            ent("d", "John Effective Date", PiiCategory::PersonName, 0.6),
        ]);
        assert_eq!(a.suppressed.len(), 1);
        assert_eq!(a.suppressed[0].1, "i1");
        assert_eq!(a.kept.iter().map(|e| e.id.as_str()).collect::<Vec<_>>(), vec!["b", "c", "d"]);
    }

    #[test]
    fn candidates_need_three_rejections_and_no_approval() {
        let mut ds = vec![];
        for d in ["d1", "d2", "d3"] {
            ds.push(dec(d, "PersonName", "Managing Partner", false));
            ds.push(dec(d, "PersonName", "Santos", false));
        }
        ds.push(dec("d4", "PersonName", "Santos", true));
        ds.push(dec("d1", "CompanyName", "New York", false));
        let mut m = Memory::from_decisions(&ds);
        let c = m.candidates(3);
        assert_eq!(c.len(), 1);
        assert_eq!(c[0].1, "managing partner");
        m.add_list_item("PersonName", "Managing Partner", "x");
        assert!(m.candidates(3).is_empty(), "listed strings are not suggested again");
    }

    #[test]
    fn import_preview_validates_and_dedupes() {
        let existing: HashSet<(String, String)> = [("PersonName".to_string(), "effective date".to_string())].into();
        let json = r#"{"format":"qvault-not-personal-data","version":1,"items":[
            {"text":"Effective Date","category":"PersonName"},
            {"text":"  Managing   Partner ","category":"PersonName","note":"role"},
            {"text":"managing partner","category":"PersonName"},
            {"text":"Managing Partner","category":"CompanyName"},
            {"text":"","category":"PersonName"},
            {"text":"x","category":"Nope"},
            {"text":"bad\u0007bell","category":"PersonName"}
        ]}"#;
        let p = preview_import(json, &existing).unwrap();
        assert_eq!(p.new_items.len(), 2);
        assert_eq!(p.new_items[0].text, "Managing Partner");
        assert_eq!(p.new_items[0].note.as_deref(), Some("role"));
        assert_eq!(p.already_listed, 1);
        assert_eq!(p.duplicates_in_file, 1);
        assert_eq!(p.invalid.iter().map(|i| i.index).collect::<Vec<_>>(), vec![5, 6, 7]);
        assert!(preview_import(r#"{"format":"other","version":1,"items":[]}"#, &existing).is_err());
        assert!(preview_import(r#"{"format":"qvault-not-personal-data","version":9,"items":[]}"#, &existing).is_err());
        assert!(preview_import("[]", &existing).is_err());
        // Round trip of an export.
        let f = ListFile::new(vec![ListFileItem { text: "Board of Directors".into(), category: "CompanyName".into(), note: None }]);
        let p = preview_import(&serde_json::to_string(&f).unwrap(), &HashSet::new()).unwrap();
        assert_eq!(p.new_items.len(), 1);
    }

    #[test]
    fn example_selection_is_unanimous_short_and_spread() {
        let c = |t: &str, cat: PiiCategory, r: usize, a: usize| ExampleCandidate { text: t.into(), category: cat, rejected: r, approved: a };
        let cands = vec![
            c("Effective Date", PiiCategory::PersonName, 5, 0),
            c("Managing Partner", PiiCategory::PersonName, 3, 0),
            c("New York", PiiCategory::CompanyName, 2, 0),
            c("Santos", PiiCategory::PersonName, 2, 1),
            c("Dana Holloway", PiiCategory::PersonName, 0, 4),
            c("Atlas Digital LLC", PiiCategory::CompanyName, 0, 2),
            c(&"x".repeat(70), PiiCategory::Address, 0, 9),
        ];
        let ex = select_examples(&cands, 6, ExampleMix::Mixed);
        let names: Vec<(&str, bool)> = ex.iter().map(|e| (e.text.as_str(), e.personal)).collect();
        assert_eq!(names, vec![
            ("Effective Date", false), ("New York", false), ("Managing Partner", false),
            ("Dana Holloway", true), ("Atlas Digital LLC", true),
        ]);
        let ex = select_examples(&cands, 2, ExampleMix::RejectedOnly);
        assert_eq!(ex.len(), 2);
        assert!(ex.iter().all(|e| !e.personal));
        assert!(select_examples(&[], 6, ExampleMix::Mixed).is_empty());
    }
}
