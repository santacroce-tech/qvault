//! Layer 3: LLM-assisted PII detection.
//!
//! The page text is split into overlapping word chunks. Each chunk goes to an
//! [`Layer3Engine`] (in the app: a QVAC LLM in the sidecar, constrained by a
//! JSON schema to `{entities:[{text, category}]}`), the answer is validated,
//! and every returned string is mapped back to *all* of its occurrences in
//! the chunk (case- and whitespace-tolerant). Ranges already found by Layers
//! 1–2 are skipped and duplicates across overlapping chunks are dropped.
//! Detections go to the normal review queue with confidence 0.70 and
//! `detection_layer = "llm"`; nothing is auto-applied.

use regex::Regex;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::pipeline::types::{PiiCategory, PiiEntity};

/// Words per chunk and overlap between consecutive chunks.
pub const MAX_WORDS_PER_CHUNK: usize = 150;
pub const WORD_OVERLAP: usize = 20;

/// Confidence assigned to every Layer 3 detection.
pub const CONFIDENCE: f64 = 0.70;
/// `detection_layer` value stored for Layer 3 detections (what the UI shows).
pub const LAYER: &str = "llm";

/// Progress callback: (count, phase label).
pub type ProgressFn = Box<dyn Fn(usize, &str) + Send>;

/// One raw entity as returned by the model.
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct RawEntity {
    pub text: String,
    pub category: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Layer3Error {
    /// The engine is gone (sidecar unavailable, model not loadable): stop.
    Fatal(String),
    /// This chunk failed (timeout, invalid output): skip it, keep going.
    Chunk(String),
}

/// Something that can extract entities from a chunk of text.
pub trait Layer3Engine {
    fn extract(&self, chunk_text: &str) -> Result<Vec<RawEntity>, Layer3Error>;
}

/// Result of scanning one page.
#[derive(Debug, Default)]
pub struct ScanReport {
    pub entities: Vec<PiiEntity>,
    pub chunks: usize,
    pub failed_chunks: usize,
    /// Set when the engine failed fatally; the remaining chunks were skipped.
    pub aborted: Option<String>,
}

// ── prompt / schema ──────────────────────────────────────────────────────

/// Every PiiCategory variant name, in declaration order (serde names).
pub const CATEGORY_NAMES: &[&str] = &[
    "PersonName", "Email", "PhoneNumber", "Address", "DateOfBirth", "Date", "MonetaryAmount",
    "SocialSecurityNumber", "NationalId", "PassportNumber", "DriversLicense", "BankAccount",
    "CreditCard", "IBAN", "TaxId", "CaseNumber", "CourtFileNumber", "ClientId", "IpAddress",
    "Url", "LicensePlate", "MedicalRecordNumber", "CompanyName",
];

pub fn system_prompt() -> String {
    format!(
        "You find personal and confidential data in legal documents so it can be redacted.\n\
Answer with JSON only: {{\"entities\":[{{\"text\":\"...\",\"category\":\"...\"}}]}}.\n\
Rules:\n\
- \"text\" is copied exactly from the document and is only the sensitive item itself (a name, address, number or identifier), never a whole sentence.\n\
- List each item once. If there is nothing sensitive, answer {{\"entities\":[]}}.\n\
Categories: PersonName (a person), CompanyName (a company or organisation), Address (a postal address), {}.",
        CATEGORY_NAMES
            .iter()
            .filter(|c| !matches!(**c, "PersonName" | "CompanyName" | "Address"))
            .cloned()
            .collect::<Vec<_>>()
            .join(", ")
    )
}

/// One worked example, sent as a prior user/assistant turn.
pub fn few_shot() -> (String, String) {
    (
        "Document:\nThe tenant, Robert Klein, of 12 Oak Lane, Springfield, signed the lease with Acme Holdings LLC on behalf of client A-5521.".to_string(),
        json!({"entities": [
            {"text": "Robert Klein", "category": "PersonName"},
            {"text": "12 Oak Lane, Springfield", "category": "Address"},
            {"text": "Acme Holdings LLC", "category": "CompanyName"},
            {"text": "A-5521", "category": "ClientId"}
        ]})
        .to_string(),
    )
}

pub fn user_prompt(chunk_text: &str) -> String {
    format!("Document:\n{}", chunk_text)
}

/// JSON schema for the constrained output (llama.cpp turns it into a grammar).
pub fn response_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "entities": {
                "type": "array",
                "maxItems": 40,
                "items": {
                    "type": "object",
                    "properties": {
                        "text": {"type": "string", "minLength": 2, "maxLength": 120},
                        "category": {"type": "string", "enum": CATEGORY_NAMES}
                    },
                    "required": ["text", "category"],
                    "additionalProperties": false
                }
            }
        },
        "required": ["entities"],
        "additionalProperties": false
    })
}

#[derive(Deserialize)]
struct ResponseShape {
    entities: Vec<RawEntity>,
}

/// Parse the model's JSON answer.
pub fn parse_response(text: &str) -> Result<Vec<RawEntity>, String> {
    let trimmed = text.trim();
    serde_json::from_str::<ResponseShape>(trimmed)
        .map(|r| r.entities)
        .map_err(|e| format!("invalid JSON from model (line {}, column {})", e.line(), e.column()))
}

pub fn parse_category(name: &str) -> Option<PiiCategory> {
    serde_json::from_value(Value::String(name.to_string())).ok()
}

// ── validation ───────────────────────────────────────────────────────────

/// Reject outputs that cannot be a single PII item of the claimed category
/// (small models sometimes return whole sentences when there is nothing).
pub fn plausible(text: &str, category: &PiiCategory, chunk_len: usize) -> bool {
    let t = text.trim();
    let chars = t.chars().count();
    if chars < 2 || chars > 120 {
        return false;
    }
    if chunk_len > 0 && t.len() * 10 > chunk_len * 6 && chars > 40 {
        return false; // most of the chunk: a sentence, not an item
    }
    let words = t.split_whitespace().count();
    let has_digit = t.chars().any(|c| c.is_ascii_digit());
    let has_upper = t.chars().any(|c| c.is_uppercase());
    use PiiCategory::*;
    match category {
        PersonName => words <= 5 && has_upper && !has_digit && !t.contains('@'),
        CompanyName => words <= 8 && has_upper && !t.contains('@'),
        Address => words <= 15 && (has_digit || words >= 2),
        Email => t.contains('@') && words == 1,
        Url => words == 1 && t.contains('.'),
        IpAddress => words == 1 && has_digit,
        PhoneNumber | SocialSecurityNumber | CreditCard | BankAccount | IBAN | TaxId | NationalId
        | PassportNumber | DriversLicense | MedicalRecordNumber | CaseNumber | CourtFileNumber
        | ClientId | LicensePlate => words <= 6 && has_digit,
        Date | DateOfBirth => words <= 6 && has_digit,
        MonetaryAmount => words <= 6 && has_digit,
    }
}

// ── offset mapping ───────────────────────────────────────────────────────

/// All (start, end) byte ranges in `hay` matching `needle`, ignoring case and
/// treating any run of whitespace as equivalent. Matches must not start or
/// end inside a word.
pub fn find_occurrences(hay: &str, needle: &str) -> Vec<(usize, usize)> {
    let tokens: Vec<String> = needle.split_whitespace().map(regex::escape).collect();
    if tokens.is_empty() {
        return vec![];
    }
    let pattern = format!("(?i){}", tokens.join(r"\s+"));
    let re = match Regex::new(&pattern) {
        Ok(r) => r,
        Err(_) => return vec![],
    };
    let first_alnum = needle.trim().chars().next().map(|c| c.is_alphanumeric()).unwrap_or(false);
    let last_alnum = needle.trim().chars().last().map(|c| c.is_alphanumeric()).unwrap_or(false);
    re.find_iter(hay)
        .filter(|m| {
            let before_ok = !first_alnum
                || hay[..m.start()].chars().next_back().map(|c| !c.is_alphanumeric()).unwrap_or(true);
            let after_ok = !last_alnum
                || hay[m.end()..].chars().next().map(|c| !c.is_alphanumeric()).unwrap_or(true);
            before_ok && after_ok
        })
        .map(|m| (m.start(), m.end()))
        .collect()
}

// ── chunking ─────────────────────────────────────────────────────────────

/// A whitespace-split word with its byte offsets in the original text.
#[derive(Debug, Clone)]
pub struct Word {
    pub text: String,
    pub start: usize,
    pub end: usize,
}

pub fn tokenize_words(text: &str) -> Vec<Word> {
    let mut words = Vec::new();
    let mut start: Option<usize> = None;
    for (i, c) in text.char_indices() {
        if c.is_whitespace() {
            if let Some(s) = start.take() {
                words.push(Word { text: text[s..i].to_string(), start: s, end: i });
            }
        } else if start.is_none() {
            start = Some(i);
        }
    }
    if let Some(s) = start {
        words.push(Word { text: text[s..].to_string(), start: s, end: text.len() });
    }
    words
}

pub fn chunk_words(words: &[Word]) -> Vec<Vec<Word>> {
    if words.len() <= MAX_WORDS_PER_CHUNK {
        return vec![words.to_vec()];
    }
    let mut chunks = Vec::new();
    let mut start = 0;
    while start < words.len() {
        let end = (start + MAX_WORDS_PER_CHUNK).min(words.len());
        chunks.push(words[start..end].to_vec());
        if end >= words.len() {
            break;
        }
        start = end - WORD_OVERLAP;
    }
    chunks
}

// ── scan ─────────────────────────────────────────────────────────────────

/// Run Layer 3 on one page of text.
pub fn scan(
    engine: &dyn Layer3Engine,
    text: &str,
    page: u32,
    existing_ranges: &[(usize, usize)],
    on_progress: Option<&ProgressFn>,
) -> ScanReport {
    let mut report = ScanReport::default();
    if text.trim().is_empty() {
        return report;
    }
    let words = tokenize_words(text);
    if words.is_empty() {
        return report;
    }
    let chunks = chunk_words(&words);
    report.chunks = chunks.len();

    for (i, chunk) in chunks.iter().enumerate() {
        if let Some(cb) = on_progress {
            cb(i, &format!("chunk {}/{}", i + 1, chunks.len()));
        }
        let c_start = chunk[0].start;
        let c_end = chunk[chunk.len() - 1].end;
        let chunk_text = &text[c_start..c_end];

        let raw = match engine.extract(chunk_text) {
            Ok(r) => r,
            Err(Layer3Error::Chunk(reason)) => {
                log::warn!("Layer 3: chunk {}/{} on page {} skipped: {}", i + 1, chunks.len(), page, reason);
                report.failed_chunks += 1;
                continue;
            }
            Err(Layer3Error::Fatal(reason)) => {
                log::warn!("Layer 3: stopped on page {} at chunk {}: {}", page, i + 1, reason);
                report.failed_chunks += chunks.len() - i;
                report.aborted = Some(reason);
                break;
            }
        };

        for ent in raw {
            let category = match parse_category(&ent.category) {
                Some(c) => c,
                None => continue,
            };
            let needle = ent.text.trim();
            if !plausible(needle, &category, chunk_text.len()) {
                continue;
            }
            for (s, e) in find_occurrences(chunk_text, needle) {
                let (start, end) = (c_start + s, c_start + e);
                if existing_ranges.iter().any(|(es, ee)| start < *ee && *es < end) {
                    continue;
                }
                if report.entities.iter().any(|x| start < x.end && x.start < end) {
                    continue;
                }
                report.entities.push(PiiEntity {
                    id: uuid::Uuid::new_v4().to_string(),
                    text: text[start..end].to_string(),
                    start,
                    end,
                    page,
                    category: category.clone(),
                    layer: LAYER.to_string(),
                    confidence: CONFIDENCE,
                    replacement: category.replacement_label().to_string(),
                });
            }
        }
    }

    if let Some(cb) = on_progress {
        cb(report.entities.len(), "done");
    }
    report.entities.sort_by_key(|e| e.start);
    report
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    #[test]
    fn test_tokenize_words_basic() {
        let text = "John  Smith lives at 123 Main St.";
        let words = tokenize_words(text);
        assert_eq!(words.len(), 7);
        assert_eq!(words[0].text, "John");
        assert_eq!(&text[words[0].start..words[0].end], "John");
        assert_eq!(words[1].text, "Smith");
        assert_eq!(&text[words[1].start..words[1].end], "Smith");
    }

    #[test]
    fn test_chunk_words_short() {
        let words = vec![
            Word { text: "a".into(), start: 0, end: 1 },
            Word { text: "b".into(), start: 2, end: 3 },
        ];
        assert_eq!(chunk_words(&words).len(), 1);
    }

    #[test]
    fn test_chunk_words_long() {
        let words: Vec<Word> = (0..400)
            .map(|i| Word { text: format!("w{}", i), start: i * 4, end: i * 4 + 3 })
            .collect();
        let chunks = chunk_words(&words);
        assert!(chunks.len() >= 3);
        assert_eq!(chunks[0][chunks[0].len() - WORD_OVERLAP].text, chunks[1][0].text);
    }

    /// Fake engine: returns canned answers per call and records chunk texts.
    struct Fake {
        answers: RefCell<Vec<Result<Vec<RawEntity>, Layer3Error>>>,
        seen: RefCell<Vec<String>>,
    }
    impl Fake {
        fn new(answers: Vec<Result<Vec<RawEntity>, Layer3Error>>) -> Self {
            Self { answers: RefCell::new(answers), seen: RefCell::new(vec![]) }
        }
    }
    impl Layer3Engine for Fake {
        fn extract(&self, chunk_text: &str) -> Result<Vec<RawEntity>, Layer3Error> {
            self.seen.borrow_mut().push(chunk_text.to_string());
            let mut a = self.answers.borrow_mut();
            if a.is_empty() { Ok(vec![]) } else { a.remove(0) }
        }
    }
    fn ent(text: &str, cat: &str) -> RawEntity {
        RawEntity { text: text.into(), category: cat.into() }
    }

    #[test]
    fn maps_all_occurrences_case_and_whitespace_tolerant() {
        let text = "Maria GONZALEZ signed. Later maria  gonzalez\nand Maria\tGonzalez agreed. Mariana Gonzalezz no.";
        let fake = Fake::new(vec![Ok(vec![ent("Maria Gonzalez", "PersonName")])]);
        let r = scan(&fake, text, 0, &[], None);
        let found: Vec<&str> = r.entities.iter().map(|e| &text[e.start..e.end]).collect();
        assert_eq!(found, vec!["Maria GONZALEZ", "maria  gonzalez", "Maria\tGonzalez"]);
        for e in &r.entities {
            assert_eq!(e.layer, "llm");
            assert_eq!(e.confidence, 0.70);
            assert_eq!(e.category, PiiCategory::PersonName);
            assert_eq!(e.replacement, "[REDACTED NAME]");
        }
    }

    #[test]
    fn skips_layer_1_2_ranges_and_invalid_output() {
        let text = "Contact jane@example.com or Jane Roe. The court finds the motion without merit.";
        let email = text.find("jane@example.com").unwrap();
        let existing = vec![(email, email + "jane@example.com".len())];
        let fake = Fake::new(vec![Ok(vec![
            ent("jane@example.com", "Email"),                                 // already found by L1
            ent("Jane Roe", "PersonName"),                                    // new
            ent("The court finds the motion without merit.", "CaseNumber"),   // hallucinated sentence
            ent("Jane Roe", "NotACategory"),                                  // unknown category
            ent("Nobody Here", "PersonName"),                                 // not in text
        ])]);
        let r = scan(&fake, text, 2, &existing, None);
        assert_eq!(r.entities.len(), 1);
        assert_eq!(r.entities[0].text, "Jane Roe");
        assert_eq!(r.entities[0].page, 2);
    }

    #[test]
    fn dedupes_across_overlapping_chunks_and_keeps_offsets_global() {
        // 300 words → 3 chunks; the name sits in the overlap of chunks 0 and 1.
        let mut words: Vec<String> = (0..300).map(|i| format!("w{}", i)).collect();
        words[140] = "Olga".into();
        words[141] = "Petrova".into();
        let text = words.join(" ");
        let name = ent("Olga Petrova", "PersonName");
        let fake = Fake::new(vec![Ok(vec![name.clone()]), Ok(vec![name.clone()]), Ok(vec![])]);
        let r = scan(&fake, &text, 0, &[], None);
        assert_eq!(fake.seen.borrow().len(), 3);
        assert_eq!(r.entities.len(), 1);
        let e = &r.entities[0];
        assert_eq!(&text[e.start..e.end], "Olga Petrova");
    }

    #[test]
    fn chunk_errors_are_skipped_fatal_errors_abort() {
        let words: Vec<String> = (0..300).map(|i| format!("w{}", i)).collect();
        let text = words.join(" ");
        let fake = Fake::new(vec![
            Err(Layer3Error::Chunk("timeout".into())),
            Err(Layer3Error::Fatal("sidecar unavailable".into())),
        ]);
        let r = scan(&fake, &text, 0, &[], None);
        assert_eq!(r.chunks, 3);
        assert_eq!(r.failed_chunks, 3);
        assert_eq!(r.aborted.as_deref(), Some("sidecar unavailable"));
        assert_eq!(fake.seen.borrow().len(), 2, "no calls after a fatal error");
    }

    #[test]
    fn progress_callback_is_called_per_chunk() {
        let words: Vec<String> = (0..300).map(|i| format!("w{}", i)).collect();
        let text = words.join(" ");
        let calls = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let c2 = calls.clone();
        let cb: ProgressFn = Box::new(move |_, phase| c2.lock().unwrap().push(phase.to_string()));
        let fake = Fake::new(vec![]);
        scan(&fake, &text, 0, &[], Some(&cb));
        assert_eq!(*calls.lock().unwrap(), vec!["chunk 1/3", "chunk 2/3", "chunk 3/3", "done"]);
    }

    #[test]
    fn word_boundaries_are_respected() {
        assert_eq!(find_occurrences("Ann annexed Ann's file", "Ann"), vec![(0, 3), (12, 15)]);
        assert_eq!(find_occurrences("call 555-1234.", "555-1234"), vec![(5, 13)]);
        assert!(find_occurrences("abc", "   ").is_empty());
    }

    #[test]
    fn schema_and_categories_cover_every_pii_category() {
        for name in CATEGORY_NAMES {
            let cat = parse_category(name).unwrap_or_else(|| panic!("{} is not a PiiCategory", name));
            assert_eq!(&cat.to_string(), name);
        }
        assert_eq!(CATEGORY_NAMES.len(), 23);
        let schema = response_schema();
        assert_eq!(schema["properties"]["entities"]["items"]["properties"]["category"]["enum"].as_array().unwrap().len(), 23);
        assert!(system_prompt().contains("MedicalRecordNumber"));
    }

    #[test]
    fn parse_response_validates_shape() {
        assert_eq!(
            parse_response(r#"{"entities":[{"text":"A B","category":"PersonName"}]}"#).unwrap(),
            vec![ent("A B", "PersonName")]
        );
        assert!(parse_response("not json").is_err());
        assert!(parse_response(r#"{"items":[]}"#).is_err());
    }

    #[test]
    fn plausibility_rules() {
        use PiiCategory::*;
        assert!(plausible("Dr. Amara Okafor", &PersonName, 500));
        assert!(plausible("Jonathan P. Whitmore", &PersonName, 500));
        assert!(!plausible("The parties shall bear their own costs.", &ClientId, 500));
        assert!(!plausible("john smith 42", &PersonName, 500));
        assert!(plausible("4417 Birchwood Avenue, Portland, Oregon 97205", &Address, 500));
        assert!(!plausible("March", &Date, 500));
        assert!(plausible("14/02/1979", &DateOfBirth, 500));
        assert!(!plausible("not an email", &Email, 500));
    }
}
