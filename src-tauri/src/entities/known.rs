//! Known-entity suggestions: find occurrences of entities already in the
//! knowledge base (or already detected in this document) that no detection
//! layer flagged, including their usual variants ("Ms. Santos", "M. Santos",
//! "Santos, Maria", a company name without its legal form).
//!
//! Every hit becomes a *pending* suggestion with its own layer
//! ([`LAYER`]) and a confidence below every other layer's; the reviewer sees
//! why it was suggested. Nothing is approved automatically.

use std::collections::HashSet;

use regex::Regex;
use serde::Serialize;

use super::forms::{self, Form, Kind};
use super::normalize;
use crate::llm::llm_scanner::{self, MatchOpts};
use crate::pipeline::types::{PiiCategory, PiiEntity};

pub const LAYER: &str = "known-entity";

/// Confidence of a suggestion (all below Layer 3's lowest, 0.50 … 0.85, and
/// Layer 1's 0.95). The source decides the first number: a value the user
/// approved in some document, or one only detected (unreviewed) in this
/// document.
pub fn confidence(rule: FlagRule, approved: bool) -> f64 {
    let base: f64 = match rule {
        FlagRule::SameText | FlagRule::Reordered => 0.60,
        FlagRule::CompanyWithoutLegalForm => 0.50,
        FlagRule::HonorificSurname | FlagRule::InitialSurname => 0.45,
    };
    if approved {
        base
    } else {
        base - 0.10
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum FlagRule {
    /// The known text itself (case-, accent- and spacing-insensitive).
    SameText,
    /// "Last, First" ⇄ "First Last".
    Reordered,
    /// An honorific followed by the surname ("Sra. Santos").
    HonorificSurname,
    /// An initial followed by the surname ("M. Santos", "Santos, M.").
    InitialSurname,
    /// A known company without its legal form ("Meridian Consulting").
    CompanyWithoutLegalForm,
}

impl FlagRule {
    pub fn as_str(self) -> &'static str {
        match self {
            FlagRule::SameText => "same-text",
            FlagRule::Reordered => "reordered",
            FlagRule::HonorificSurname => "honorific-surname",
            FlagRule::InitialSurname => "initial-surname",
            FlagRule::CompanyWithoutLegalForm => "company-without-legal-form",
        }
    }
}

/// An entity string the knowledge base knows.
#[derive(Debug, Clone)]
pub struct Known {
    pub category: PiiCategory,
    pub text: String,
    /// Approved by the user in at least one document (else: detected, not
    /// yet reviewed, in the document being scanned).
    pub approved: bool,
    /// Documents in which this entity was approved.
    pub documents: usize,
    /// Display name of its group (canonical variant), if grouped.
    pub group_label: Option<String>,
    pub group_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Flag {
    pub start: usize,
    pub end: usize,
    pub text: String,
    #[serde(skip)]
    pub category: PiiCategory,
    pub rule: FlagRule,
    pub confidence: f64,
    /// The known string that produced the suggestion.
    pub source_text: String,
    pub source_approved: bool,
    pub documents: usize,
    pub group_label: Option<String>,
    pub group_id: Option<String>,
}

impl Flag {
    pub fn to_entity(&self, page: u32) -> PiiEntity {
        PiiEntity {
            id: uuid::Uuid::new_v4().to_string(),
            text: self.text.clone(),
            start: self.start,
            end: self.end,
            page,
            category: self.category.clone(),
            layer: LAYER.to_string(),
            confidence: self.confidence,
            replacement: self.category.replacement_label().to_string(),
        }
    }
}

/// Categories suggested from the knowledge base. Dates and amounts are not
/// entities: the same amount in another document is a coincidence.
pub fn flaggable(c: &PiiCategory) -> bool {
    !matches!(c, PiiCategory::Date | PiiCategory::DateOfBirth | PiiCategory::MonetaryAmount)
}

struct Pattern {
    needle: Needle,
    rule: FlagRule,
    /// The match must start with an uppercase letter (names).
    capitalised: bool,
    /// Folded token that must appear in the page for this pattern to be tried.
    anchor: String,
}

enum Needle {
    /// Words matched case- and accent-insensitively, any whitespace between.
    Words(String),
    /// Honorific + surname: the surname words; checked for a preceding honorific.
    AfterHonorific(String),
    Regex(Regex),
}

fn patterns(k: &Known) -> Vec<Pattern> {
    let cat = k.category.to_string();
    let form: Form = forms::analyse(&cat, &k.text);
    let mut out = Vec::new();
    let text = k.text.trim();
    if text.chars().filter(|c| c.is_alphanumeric()).count() < 3 {
        return out;
    }
    match form.kind {
        Kind::Person => {
            let full: Vec<&forms::Tok> = form.full_tokens().collect();
            if full.is_empty() {
                return out;
            }
            let anchor = full.last().unwrap().s.clone();
            // As written (honorific dropped) and reordered.
            let surface = &form.surface;
            if surface.len() >= 1 {
                out.push(Pattern { needle: Needle::Words(surface.join(" ")), rule: FlagRule::SameText, capitalised: true, anchor: anchor.clone() });
            }
            if surface.len() >= 2 && !form.has_initials() {
                // "Last, First …" and, for two-part surnames, "Last1 Last2, First …".
                let n = surface.len();
                let mut variants: Vec<String> = vec![format!("{}, {}", surface[n - 1], surface[..n - 1].join(" "))];
                if n >= 3 {
                    variants.push(format!("{} {}, {}", surface[n - 2], surface[n - 1], surface[..n - 2].join(" ")));
                    variants.push(format!("{}, {}", surface[n - 1], surface[0]));
                }
                for v in variants {
                    let re = format!(
                        r"(?i)(?:^|[^\p{{L}}\p{{N}}])({})(?:$|[^\p{{L}}\p{{N}}])",
                        v.split(", ").map(|p| words_regex(p)).collect::<Vec<_>>().join(r"\s*,\s*")
                    );
                    if let Ok(r) = Regex::new(&re) {
                        out.push(Pattern { needle: Needle::Regex(r), rule: FlagRule::Reordered, capitalised: true, anchor: anchor.clone() });
                    }
                }
                // "First Last" written from a known "Last, First" is covered by
                // the surface pattern (the surface is always "First … Last").
            }
            // Honorific / initial + surname need a full given name and surname.
            let toks = &form.toks;
            if toks.len() >= 2 && !toks[0].initial && !toks[toks.len() - 1].initial {
                let surnames: Vec<String> = {
                    let mut v = vec![toks[toks.len() - 1].s.clone()];
                    if toks.len() >= 3 && !toks[toks.len() - 2].initial {
                        // "Ferrándiz Olmedo"; keep connectors as written ("da Silva Santos").
                        let n = surface.len();
                        if n >= 2 {
                            v.push(format!("{} {}", surface[n - 2], surface[n - 1]));
                        }
                    }
                    v
                };
                for sn in &surnames {
                    if sn.chars().filter(|c| c.is_alphabetic()).count() < 3 {
                        continue;
                    }
                    out.push(Pattern { needle: Needle::AfterHonorific(sn.clone()), rule: FlagRule::HonorificSurname, capitalised: true, anchor: anchor.clone() });
                    let initial = regex::escape(&toks[0].s[..toks[0].s.chars().next().unwrap().len_utf8()]);
                    let snr = words_regex(sn);
                    let re = format!(
                        r"(?i)(?:^|[^\p{{L}}\p{{N}}])({i}\.?\s+(?:[\p{{L}}]\.?\s+)?{s}|{s}\s*,\s*{i}\.?)(?:$|[^\p{{L}}\p{{N}}])",
                        i = initial,
                        s = snr
                    );
                    if let Ok(r) = Regex::new(&re) {
                        out.push(Pattern { needle: Needle::Regex(r), rule: FlagRule::InitialSurname, capitalised: true, anchor: anchor.clone() });
                    }
                }
            }
        }
        Kind::Company => {
            let anchor = form.toks.iter().map(|t| t.s.clone()).max_by_key(|s| s.len()).unwrap_or_default();
            if anchor.is_empty() {
                return out;
            }
            out.push(Pattern { needle: Needle::Words(text.to_string()), rule: FlagRule::SameText, capitalised: true, anchor: anchor.clone() });
            if form.legal.is_some() && form.legal_len > 0 && form.surface.len() > form.legal_len {
                let core: Vec<String> = form.surface[..form.surface.len() - form.legal_len].to_vec();
                let core_letters: usize = core.iter().map(|t| t.chars().filter(|c| c.is_alphabetic()).count()).sum();
                let qualifies = (core.len() >= 2 && core_letters >= 6) || (core.len() == 1 && core_letters >= 5);
                if qualifies {
                    out.push(Pattern { needle: Needle::Words(core.join(" ")), rule: FlagRule::CompanyWithoutLegalForm, capitalised: true, anchor });
                }
            }
        }
        Kind::Identifier | Kind::Phone => {
            let key: Vec<char> = form.key.chars().collect();
            if key.len() < 5 {
                return out;
            }
            // Same characters, any of the usual separators between them.
            let body = key.iter().map(|c| regex::escape(&c.to_string())).collect::<Vec<_>>().join(r"[\s.\-/()]?");
            let re = format!(r"(?i)(?:^|[^\p{{L}}\p{{N}}])({})(?:$|[^\p{{L}}\p{{N}}])", body);
            if let Ok(r) = Regex::new(&re) {
                out.push(Pattern { needle: Needle::Regex(r), rule: FlagRule::SameText, capitalised: false, anchor: String::new() });
            }
        }
        Kind::Email | Kind::Url | Kind::Address => {
            let anchor = normalize::words(&normalize::fold(text)).into_iter().max_by_key(|s| s.len()).unwrap_or_default();
            out.push(Pattern { needle: Needle::Words(text.to_string()), rule: FlagRule::SameText, capitalised: false, anchor });
        }
        Kind::Literal => {}
    }
    out
}

/// A folded word sequence as an accent-insensitive regex (whitespace runs
/// between words).
fn words_regex(s: &str) -> String {
    s.split_whitespace()
        .map(|w| {
            w.chars()
                .map(|c| match c {
                    'a' => "[aàáâãäå]".to_string(),
                    'e' => "[eèéêë]".to_string(),
                    'i' => "[iìíîï]".to_string(),
                    'o' => "[oòóôõö]".to_string(),
                    'u' => "[uùúûü]".to_string(),
                    'c' => "[cç]".to_string(),
                    'n' => "[nñ]".to_string(),
                    _ => regex::escape(&c.to_string()),
                })
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join(r"\s+")
}

/// The honorific right before `start`, if any, with the gender it implies.
fn honorific_before(hay: &str, start: usize) -> Option<Option<char>> {
    let before = hay[..start].trim_end();
    let word: String = before
        .chars()
        .rev()
        .take_while(|c| c.is_alphabetic() || *c == 'ª' || *c == '.' || *c == 'º')
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    let w = normalize::fold(&word);
    let w = w.trim_end_matches('.');
    // "Sra." / "Dr.ª" / "Mr"; one-letter titles ("D.") are too ambiguous.
    w.split('.').filter(|p| p.len() >= 2).find_map(normalize::honorific)
}

fn overlaps(a: (usize, usize), b: (usize, usize)) -> bool {
    a.0 < b.1 && b.0 < a.1
}

/// Suggestions for one page. `existing` are the ranges any layer already
/// detected on it (never suggested again).
pub fn flag_page(page_text: &str, known: &[Known], existing: &[(usize, usize)]) -> Vec<Flag> {
    if page_text.is_empty() || known.is_empty() {
        return vec![];
    }
    let page_tokens: HashSet<String> = normalize::words(&normalize::fold(page_text)).into_iter().collect();
    let page_digits: String = page_text.chars().filter(|c| c.is_alphanumeric()).collect::<String>().to_uppercase();
    let mut out: Vec<Flag> = Vec::new();
    let opts = MatchOpts { fold_accents: true, hyphen_wrap: true };
    for k in known {
        if !flaggable(&k.category) {
            continue;
        }
        let known_gender = forms::analyse(&k.category.to_string(), &k.text).gender;
        for p in patterns(k) {
            // Cheap pre-filter: the anchor word (or the digits) must be on the page.
            if !p.anchor.is_empty() && !page_tokens.contains(&p.anchor) {
                continue;
            }
            if p.anchor.is_empty() {
                let key = forms::analyse(&k.category.to_string(), &k.text).key;
                if !page_digits.contains(&key) {
                    continue;
                }
            }
            let hits: Vec<(usize, usize)> = match &p.needle {
                Needle::Words(w) => llm_scanner::find_occurrences_with(page_text, w, opts),
                Needle::AfterHonorific(sn) => llm_scanner::find_occurrences_with(page_text, sn, opts)
                    .into_iter()
                    .filter(|(s, _)| match honorific_before(page_text, *s) {
                        // "Mr. Santos" is not "Ms. Maria Santos".
                        Some(g) => !matches!((g, known_gender), (Some(x), Some(y)) if x != y),
                        None => false,
                    })
                    .collect(),
                Needle::Regex(r) => r.captures_iter(page_text).filter_map(|c| c.get(1)).map(|m| (m.start(), m.end())).collect(),
            };
            for (s, e) in hits {
                let span = &page_text[s..e];
                if p.capitalised && !span.chars().next().map(|c| c.is_uppercase() || c.is_numeric()).unwrap_or(false) {
                    continue;
                }
                if existing.iter().any(|r| overlaps(*r, (s, e))) {
                    continue;
                }
                if llm_scanner::stoplisted(span, &k.category) {
                    continue;
                }
                let flag = Flag {
                    start: s,
                    end: e,
                    text: span.to_string(),
                    category: k.category.clone(),
                    rule: p.rule,
                    confidence: confidence(p.rule, k.approved),
                    source_text: k.text.clone(),
                    source_approved: k.approved,
                    documents: k.documents,
                    group_label: k.group_label.clone(),
                    group_id: k.group_id.clone(),
                };
                // Keep the longer / more confident suggestion where two overlap.
                if let Some(pos) = out.iter().position(|f| overlaps((f.start, f.end), (s, e))) {
                    let cur = &out[pos];
                    let better = (flag.end - flag.start, (flag.confidence * 100.0) as i64) > (cur.end - cur.start, (cur.confidence * 100.0) as i64);
                    if better {
                        out[pos] = flag;
                    }
                } else {
                    out.push(flag);
                }
            }
        }
    }
    out.sort_by_key(|f| f.start);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn known(cat: PiiCategory, text: &str, approved: bool) -> Known {
        Known { category: cat, text: text.into(), approved, documents: 1, group_label: None, group_id: None }
    }

    fn texts(f: &[Flag]) -> Vec<(&str, FlagRule)> {
        f.iter().map(|f| (f.text.as_str(), f.rule)).collect()
    }

    #[test]
    fn finds_variants_of_a_known_person() {
        let page = "Witness: SANTOS, María. Later Ms. Santos confirmed it; M. Santos signed. Santos Street is unrelated, as is santos.";
        let k = vec![known(PiiCategory::PersonName, "Maria Santos", true)];
        let f = flag_page(page, &k, &[]);
        let t = texts(&f);
        assert!(t.contains(&("SANTOS, María", FlagRule::Reordered)), "{:?}", t);
        assert!(t.contains(&("Santos", FlagRule::HonorificSurname)), "{:?}", t);
        assert!(t.contains(&("M. Santos", FlagRule::InitialSurname)), "{:?}", t);
        assert_eq!(f.len(), 3, "a bare surname is never suggested: {:?}", t);
        assert!(f.iter().all(|x| x.confidence < 0.7));
        // Already detected ranges are skipped.
        let first = f.iter().find(|x| x.rule == FlagRule::Reordered).unwrap();
        let again = flag_page(page, &k, &[(first.start, first.end)]);
        assert_eq!(again.len(), 2);
    }

    #[test]
    fn exact_text_and_companies() {
        let page = "Payment to Meridian Consulting and to MERIDIAN CONSULTING LLC; account 12 3456 7890 and 1234567890.";
        let k = vec![
            known(PiiCategory::CompanyName, "Meridian Consulting LLC", false),
            known(PiiCategory::BankAccount, "1234567890", true),
            known(PiiCategory::MonetaryAmount, "$50,000.00", true),
        ];
        let f = flag_page(page, &k, &[]);
        let t = texts(&f);
        assert!(t.contains(&("MERIDIAN CONSULTING LLC", FlagRule::SameText)), "{:?}", t);
        assert!(t.contains(&("Meridian Consulting", FlagRule::CompanyWithoutLegalForm)), "{:?}", t);
        assert!(t.contains(&("12 3456 7890", FlagRule::SameText)), "{:?}", t);
        assert!(t.contains(&("1234567890", FlagRule::SameText)), "{:?}", t);
        let unreviewed = f.iter().find(|x| x.category == PiiCategory::CompanyName).unwrap();
        assert!(unreviewed.confidence < 0.55);
    }

    #[test]
    fn gender_and_initial_mismatches_are_not_suggested() {
        let page = "J. Santos and Sr. Pereira attended.";
        let k = vec![known(PiiCategory::PersonName, "Maria Santos", true), known(PiiCategory::PersonName, "Ana Pereira", true)];
        let f = flag_page(page, &k, &[]);
        // "J. Santos" has the wrong initial. "Sr. Pereira" has an honorific and
        // the surname: suggested (the reviewer decides; the rule says why).
        assert_eq!(texts(&f), vec![("Pereira", FlagRule::HonorificSurname)]);
    }
}
