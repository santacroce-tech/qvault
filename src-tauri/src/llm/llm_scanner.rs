//! Layer 3: LLM-assisted PII detection.
//!
//! The page text is split into overlapping word chunks. Each chunk goes to a
//! [`Layer3Engine`] (in the app: a QVAC LLM in the sidecar, constrained by a
//! JSON schema to `{entities:[{text, category}]}`), which returns the model's
//! raw answer. This module then:
//!
//! 1. parses the answer strictly ([`parse_strict`]): only `{"entities":[…]}`
//!    with items that have exactly a string `text` and a string `category`;
//!    other items are dropped and counted. A truncated answer (the model hit
//!    its token limit) is salvaged item by item instead of losing the chunk;
//! 2. validates every item: known category, non-empty text, plausible for the
//!    category ([`plausible`]), not a legal term ([`stoplisted`]), and the
//!    text must occur in the chunk (case-, whitespace-, accent- and
//!    line-wrap-tolerant, on word boundaries);
//! 3. maps each accepted string to every occurrence (in the page, see
//!    [`PostConfig::page_wide`]), plus name variants ("Last, First";
//!    surname after an honorific);
//! 4. merges results across overlapping chunks (a longer span replaces a
//!    shorter one it contains; repeated finds raise the agreement count),
//!    drops ranges already found by Layers 1–2, and assigns a confidence.
//!
//! Every step is counted in [`DropStats`], and every heuristic can be
//! switched off in [`PostConfig`] so the eval harness can measure what it
//! contributes (eval/README.md). Detections go to the normal review queue
//! with `detection_layer = "llm"`; nothing is auto-applied.

use std::collections::BTreeSet;

use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::pipeline::types::{PiiCategory, PiiEntity};

/// Words per chunk and overlap between consecutive chunks.
pub const MAX_WORDS_PER_CHUNK: usize = 150;
pub const WORD_OVERLAP: usize = 20;

/// Phase 1B confidence for every Layer 3 detection (still used by
/// [`ConfidenceMode::Flat`]).
pub const CONFIDENCE: f64 = 0.70;
/// `detection_layer` value stored for Layer 3 detections (what the UI shows).
pub const LAYER: &str = "llm";

/// Progress callback: (count, phase label).
pub type ProgressFn = Box<dyn Fn(usize, &str) + Send>;

/// One raw entity as returned by the model.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
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

/// Something that can run the model on a chunk of text and return its raw
/// answer (the JSON text). Parsing and validation happen in this module.
pub trait Layer3Engine {
    fn complete(&self, chunk_text: &str) -> Result<String, Layer3Error>;
}

// ── prompt / schema ──────────────────────────────────────────────────────

/// Every PiiCategory variant name, in declaration order (serde names).
pub const CATEGORY_NAMES: &[&str] = &[
    "PersonName", "Email", "PhoneNumber", "Address", "DateOfBirth", "Date", "MonetaryAmount",
    "SocialSecurityNumber", "NationalId", "PassportNumber", "DriversLicense", "BankAccount",
    "CreditCard", "IBAN", "TaxId", "CaseNumber", "CourtFileNumber", "ClientId", "IpAddress",
    "Url", "LicensePlate", "MedicalRecordNumber", "CompanyName",
];

/// Prompt versions. Reports record which one produced their numbers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PromptVersion {
    /// Phase 1B: short system prompt, one English example.
    P1b,
    /// Phase 3: inclusion and exclusion rules, category glossary, three
    /// examples (EN/ES/PT) that each contain things *not* to extract.
    P3,
}

/// The prompt Layer 3 uses in the app.
pub const DEFAULT_PROMPT: PromptVersion = PromptVersion::P3;

impl PromptVersion {
    pub fn id(&self) -> &'static str {
        match self {
            PromptVersion::P1b => "p1b",
            PromptVersion::P3 => "p3",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "p1b" => Some(PromptVersion::P1b),
            "p3" => Some(PromptVersion::P3),
            _ => None,
        }
    }
}

fn system_prompt_p1b() -> String {
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

const SYSTEM_P3: &str = "You find personal and confidential data in legal documents (English, Spanish or Portuguese) so that a lawyer can redact it.
Answer with JSON only: {\"entities\":[{\"text\":\"...\",\"category\":\"...\"}]}.

Extract:
- names of people, including a surname on its own, initials (\"J. M. Ortuño\") and \"LAST, First\" forms;
- names of private companies, firms and organisations that are parties or employers;
- postal addresses, e-mail addresses, phone numbers, dates, amounts of money;
- identifying numbers: ID cards, tax numbers, passports, driving licences, bank accounts, cards, IBANs, case and file numbers, client/claim/policy numbers, medical record numbers, licence plates, IP addresses, URLs.

Do NOT extract: legal terms and defined terms (\"Agreement\", \"Effective Date\", \"Confidential Information\", \"the Tenant\", \"el Arrendatario\", \"a Contratante\"), clause and section headings or numbers, job titles, names of laws, statutes and regulations and their numbers, courts, government agencies and public bodies, case-law citations (\"Smith v. Jones, 123 F.3d 456\"), a city or country on its own, percentages, durations.

Rules:
- \"text\" is copied exactly from the document (same spelling, accents and punctuation) and is only the item itself, never a sentence.
- List each distinct item once. If there is nothing to extract, answer {\"entities\":[]}.

Categories: PersonName, CompanyName, Address, Email, PhoneNumber, Date (any calendar date), DateOfBirth (only a birth date), MonetaryAmount, SocialSecurityNumber, NationalId (identity card, DNI, NIE, CPF, RG, NINO), PassportNumber, DriversLicense, BankAccount (account, agency, routing number, SWIFT/BIC), CreditCard, IBAN, TaxId (EIN, VAT, NIF, CIF, CNPJ, Steuer-ID), CaseNumber, CourtFileNumber, ClientId (client, matter, claim, policy or employee number), IpAddress, Url, LicensePlate, MedicalRecordNumber.";

/// Worked examples sent as prior user/assistant turns.
fn few_shots(v: PromptVersion) -> Vec<(String, Value)> {
    match v {
        PromptVersion::P1b => vec![(
            "The tenant, Robert Klein, of 12 Oak Lane, Springfield, signed the lease with Acme Holdings LLC on behalf of client A-5521.".into(),
            json!({"entities": [
                {"text": "Robert Klein", "category": "PersonName"},
                {"text": "12 Oak Lane, Springfield", "category": "Address"},
                {"text": "Acme Holdings LLC", "category": "CompanyName"},
                {"text": "A-5521", "category": "ClientId"}
            ]}),
        )],
        PromptVersion::P3 => vec![
            (
                "SECTION 4. PAYMENT. Pursuant to Smith v. Jones, 123 F.3d 456 (2d Cir. 1999), the Tenant, ROBERT KLEIN (\"Klein\"), born 02/14/1980, shall pay Acme Holdings LLC $2,400.00 on March 1, 2025 to account no. 0012-339-8841. Notices to Mr. Klein at 12 Oak Lane, Springfield, IL 62704, tel. (217) 555-0110, rklein@example.net.".into(),
                json!({"entities": [
                    {"text": "ROBERT KLEIN", "category": "PersonName"},
                    {"text": "Klein", "category": "PersonName"},
                    {"text": "02/14/1980", "category": "DateOfBirth"},
                    {"text": "Acme Holdings LLC", "category": "CompanyName"},
                    {"text": "$2,400.00", "category": "MonetaryAmount"},
                    {"text": "March 1, 2025", "category": "Date"},
                    {"text": "0012-339-8841", "category": "BankAccount"},
                    {"text": "12 Oak Lane, Springfield, IL 62704", "category": "Address"},
                    {"text": "(217) 555-0110", "category": "PhoneNumber"},
                    {"text": "rklein@example.net", "category": "Email"}
                ]}),
            ),
            (
                "CLÁUSULA TERCERA. Conforme al artículo 1.544 del Código Civil, Dña. Lucía Ortega Viñas, con DNI 12345678Z y domicilio en Calle Mayor 5, 28013 Madrid, abonará a Construcciones Leira S.A. la cantidad de 3.000 € antes del 30 de junio de 2025 (expediente CL-88/2025).".into(),
                json!({"entities": [
                    {"text": "Lucía Ortega Viñas", "category": "PersonName"},
                    {"text": "12345678Z", "category": "NationalId"},
                    {"text": "Calle Mayor 5, 28013 Madrid", "category": "Address"},
                    {"text": "Construcciones Leira S.A.", "category": "CompanyName"},
                    {"text": "3.000 €", "category": "MonetaryAmount"},
                    {"text": "30 de junio de 2025", "category": "Date"},
                    {"text": "CL-88/2025", "category": "ClientId"}
                ]}),
            ),
            (
                "Processo nº 0801234-56.2023.8.19.0001 – Tribunal de Justiça do Estado do Rio de Janeiro. O autor, SOUZA, Paulo Roberto, CPF 222.333.444-05, nascido em 5 de maio de 1975, telefone (21) 99999-0000, reside na Rua das Flores, 10, Rio de Janeiro, nos termos do art. 319 do CPC.".into(),
                json!({"entities": [
                    {"text": "0801234-56.2023.8.19.0001", "category": "CaseNumber"},
                    {"text": "SOUZA, Paulo Roberto", "category": "PersonName"},
                    {"text": "222.333.444-05", "category": "NationalId"},
                    {"text": "5 de maio de 1975", "category": "DateOfBirth"},
                    {"text": "(21) 99999-0000", "category": "PhoneNumber"},
                    {"text": "Rua das Flores, 10, Rio de Janeiro", "category": "Address"}
                ]}),
            ),
        ],
    }
}

pub fn user_prompt(chunk_text: &str) -> String {
    format!("Document:\n{}", chunk_text)
}

/// Chat messages (role, content) for one chunk.
pub fn messages(v: PromptVersion, chunk_text: &str) -> Vec<(&'static str, String)> {
    let system = match v {
        PromptVersion::P1b => system_prompt_p1b(),
        PromptVersion::P3 => SYSTEM_P3.to_string(),
    };
    let mut out = vec![("system", system)];
    for (u, a) in few_shots(v) {
        out.push(("user", user_prompt(&u)));
        out.push(("assistant", a.to_string()));
    }
    out.push(("user", user_prompt(chunk_text)));
    out
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

// ── parsing ──────────────────────────────────────────────────────────────

/// A parsed model answer.
#[derive(Debug, Clone, PartialEq)]
pub struct Parsed {
    pub items: Vec<RawEntity>,
    /// Array elements that were not `{text: string, category: string}`.
    pub invalid_items: usize,
    /// The answer was not valid JSON (truncated); complete items were
    /// recovered one by one.
    pub salvaged: bool,
}

fn item_re() -> &'static Regex {
    static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r#"\{\s*"(text|category)"\s*:\s*"((?:[^"\\]|\\.)*)"\s*,\s*"(text|category)"\s*:\s*"((?:[^"\\]|\\.)*)"\s*\}"#).unwrap()
    })
}

/// Parse the model's answer strictly: a JSON object whose `entities` is an
/// array of objects with exactly a string `text` and a string `category`.
/// Invalid elements are dropped and counted. If the answer is not JSON at
/// all (typically cut off at the token limit), complete items are salvaged;
/// with none, the chunk fails.
pub fn parse_strict(raw: &str) -> Result<Parsed, String> {
    let trimmed = raw.trim();
    match serde_json::from_str::<Value>(trimmed) {
        Ok(v) => {
            let arr = v
                .as_object()
                .and_then(|o| o.get("entities"))
                .and_then(|e| e.as_array())
                .ok_or_else(|| "model answer has no \"entities\" array".to_string())?;
            let mut out = Parsed { items: Vec::new(), invalid_items: 0, salvaged: false };
            for el in arr {
                let ok = el.as_object().and_then(|o| {
                    if o.len() != 2 {
                        return None;
                    }
                    Some(RawEntity {
                        text: o.get("text")?.as_str()?.to_string(),
                        category: o.get("category")?.as_str()?.to_string(),
                    })
                });
                match ok {
                    Some(e) => out.items.push(e),
                    None => out.invalid_items += 1,
                }
            }
            Ok(out)
        }
        Err(e) => {
            let items: Vec<RawEntity> = item_re()
                .captures_iter(trimmed)
                .filter_map(|c| {
                    if c[1] == c[3] {
                        return None;
                    }
                    let (t, k) = if &c[1] == "text" { (&c[2], &c[4]) } else { (&c[4], &c[2]) };
                    let text: String = serde_json::from_str(&format!("\"{}\"", t)).ok()?;
                    let category: String = serde_json::from_str(&format!("\"{}\"", k)).ok()?;
                    Some(RawEntity { text, category })
                })
                .collect();
            if items.is_empty() {
                Err(format!("invalid JSON from model (line {}, column {})", e.line(), e.column()))
            } else {
                Ok(Parsed { items, invalid_items: 0, salvaged: true })
            }
        }
    }
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
        PersonName => words <= 6 && has_upper && !has_digit && !t.contains('@'),
        CompanyName => words <= 8 && has_upper && !t.contains('@'),
        Address => words <= 18 && (has_digit || words >= 2),
        Email => t.contains('@') && words == 1,
        Url => words == 1 && t.contains('.'),
        IpAddress => words == 1 && has_digit,
        PhoneNumber | SocialSecurityNumber | CreditCard | BankAccount | IBAN | TaxId | NationalId
        | PassportNumber | DriversLicense | MedicalRecordNumber | CaseNumber | CourtFileNumber
        | ClientId | LicensePlate => words <= 8 && has_digit,
        Date | DateOfBirth => words <= 6 && has_digit,
        MonetaryAmount => words <= 6 && has_digit,
    }
}

/// Legal and generic terms models return as names or companies. Compared
/// case-insensitively after dropping a leading article.
const STOPLIST: &[&str] = &[
    // EN
    "agreement", "party", "parties", "plaintiff", "plaintiffs", "defendant", "defendants", "court",
    "client", "consultant", "company", "employee", "employer", "effective date", "confidential information",
    "purchaser", "seller", "buyer", "lessor", "lessee", "tenant", "landlord", "member", "members", "manager",
    "managers", "board of managers", "judgment debtor", "judgment creditor", "garnishee", "claimant",
    "respondent", "petitioner", "licensor", "licensee", "contractor", "guarantor", "borrower", "lender",
    "affiliate", "attorney", "counsel", "notary public", "witness", "witnesses", "the project", "project",
    "data incident", "release payment", "purchase price", "sold shares", "employment period", "notary",
    // ES
    "arrendador", "arrendadora", "arrendatario", "arrendataria", "demandante", "demandado", "demandada",
    "demandados", "actora", "parte actora", "juzgado", "mi mandante", "mi representada", "fiadora", "fiador",
    "el presidente", "la secretaria", "socio", "socios", "mercantil", "la sociedad",
    // PT
    "contratante", "contratada", "contratado", "autor", "autora", "réu", "ré", "requerente", "requerido",
    "notificante", "notificado", "outorgante", "outorgado", "procuradora", "procurador", "compradora",
    "comprador", "vendedor", "vendedora", "testemunhas", "inquilino", "locador", "locatário",
];

const ARTICLES: &[&str] = &["the ", "el ", "la ", "los ", "las ", "o ", "a ", "os ", "as ", "al ", "do ", "da "];

/// Is `text` (claimed as `category`) a legal role or generic term rather
/// than an identifying item? Only names, companies and addresses are
/// checked: numeric categories are covered by `plausible`.
pub fn stoplisted(text: &str, category: &PiiCategory) -> bool {
    use PiiCategory::*;
    if !matches!(category, PersonName | CompanyName | Address) {
        return false;
    }
    let mut t = text.trim().trim_matches(|c: char| c == '"' || c == '“' || c == '”').to_lowercase();
    for a in ARTICLES {
        if let Some(rest) = t.strip_prefix(a) {
            t = rest.to_string();
            break;
        }
    }
    if STOPLIST.contains(&t.as_str()) {
        return true;
    }
    // A "name" with no capital letter at all ("the tenant", "socio").
    matches!(category, PersonName | CompanyName) && !text.chars().any(|c| c.is_uppercase())
}

// ── offset mapping ───────────────────────────────────────────────────────

/// Accent-insensitive class for a Latin letter (lowercase input).
fn fold_class(c: char) -> Option<&'static str> {
    Some(match c {
        'a' | 'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' => "[aàáâãäå]",
        'e' | 'è' | 'é' | 'ê' | 'ë' => "[eèéêë]",
        'i' | 'ì' | 'í' | 'î' | 'ï' => "[iìíîï]",
        'o' | 'ò' | 'ó' | 'ô' | 'õ' | 'ö' => "[oòóôõö]",
        'u' | 'ù' | 'ú' | 'û' | 'ü' => "[uùúûü]",
        'c' | 'ç' => "[cç]",
        'n' | 'ñ' => "[nñ]",
        _ => return None,
    })
}

/// How tolerant [`find_occurrences_with`] is.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct MatchOpts {
    /// Accented and unaccented letters match each other.
    pub fold_accents: bool,
    /// A hyphen may be followed by a line break / spaces ("atlas-\n digital.com").
    pub hyphen_wrap: bool,
}

impl Default for MatchOpts {
    fn default() -> Self {
        // Accent folding never changed a result on the Phase 3 eval (models
        // copied accents faithfully), so it is off by default.
        MatchOpts { fold_accents: false, hyphen_wrap: true }
    }
}

fn needle_pattern(needle: &str, o: MatchOpts) -> Option<String> {
    let tokens: Vec<String> = needle
        .split_whitespace()
        .map(|tok| {
            let mut s = String::new();
            for c in tok.chars() {
                let lc = c.to_lowercase().next().unwrap_or(c);
                if o.fold_accents {
                    if let Some(class) = fold_class(lc) {
                        s.push_str(class);
                        continue;
                    }
                }
                if c == '-' && o.hyphen_wrap {
                    s.push_str(r"-\s*");
                    continue;
                }
                s.push_str(&regex::escape(&c.to_string()));
            }
            s
        })
        .collect();
    if tokens.is_empty() {
        return None;
    }
    Some(format!("(?i){}", tokens.join(r"\s+")))
}

/// All (start, end) byte ranges in `hay` matching `needle`, ignoring case and
/// treating any run of whitespace as equivalent. Matches must not start or
/// end inside a word.
pub fn find_occurrences(hay: &str, needle: &str) -> Vec<(usize, usize)> {
    find_occurrences_with(hay, needle, MatchOpts { fold_accents: false, hyphen_wrap: false })
}

pub fn find_occurrences_with(hay: &str, needle: &str, o: MatchOpts) -> Vec<(usize, usize)> {
    let Some(pattern) = needle_pattern(needle, o) else { return vec![] };
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

const HONORIFICS: &[&str] = &[
    "mr", "mrs", "ms", "miss", "dr", "dra", "prof", "sr", "sra", "srta", "d", "dña", "dª", "don", "doña", "eng",
    "lic", "ldo", "lda", "herr", "frau",
];

/// "First [Middle] Last" ⇄ "Last, First [Middle]".
fn last_first_variants(name: &str) -> Vec<String> {
    let mut out = Vec::new();
    if let Some((last, first)) = name.split_once(',') {
        let (last, first) = (last.trim(), first.trim());
        if !last.is_empty() && !first.is_empty() {
            out.push(format!("{} {}", first, last));
        }
        return out;
    }
    let toks: Vec<&str> = name.split_whitespace().collect();
    if toks.len() >= 2 {
        let last = toks[toks.len() - 1];
        let rest = toks[..toks.len() - 1].join(" ");
        out.push(format!("{}, {}", last, rest));
        if toks.len() >= 3 {
            out.push(format!("{}, {}", last, toks[0]));
            // two-part surnames (ES/PT): "Ferrándiz Olmedo, María Inmaculada"
            out.push(format!("{} {}, {}", toks[toks.len() - 2], last, toks[..toks.len() - 2].join(" ")));
        }
    }
    out
}

/// Surname occurrences right after an honorific ("Mr. Feeney", "Sra.
/// Ferrándiz Olmedo"), for a multi-word name. Returns the surname ranges.
fn honorific_surname_hits(hay: &str, name: &str, o: MatchOpts) -> Vec<(usize, usize)> {
    let toks: Vec<&str> = name.split(',').next().unwrap_or(name).split_whitespace().collect();
    let toks: Vec<&str> = if name.contains(',') {
        // "LAST, First": the surname is before the comma
        name.split(',').next().unwrap_or("").split_whitespace().collect()
    } else {
        toks
    };
    if toks.is_empty() || (!name.contains(',') && toks.len() < 2) {
        return vec![];
    }
    let mut out = Vec::new();
    let n = toks.len();
    let tails: Vec<String> = if name.contains(',') {
        vec![toks.join(" ")]
    } else {
        let mut v = vec![toks[n - 1].to_string()];
        if n >= 3 {
            v.push(format!("{} {}", toks[n - 2], toks[n - 1]));
        }
        v
    };
    for surname in tails {
        if surname.chars().filter(|c| c.is_alphabetic()).count() < 3 {
            continue;
        }
        for (s, e) in find_occurrences_with(hay, &surname, o) {
            let before = hay[..s].trim_end();
            let word: String = before
                .chars()
                .rev()
                .take_while(|c| c.is_alphabetic() || *c == 'ª' || *c == '.')
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect();
            let word = word.trim_end_matches('.').to_lowercase();
            if HONORIFICS.contains(&word.as_str()) {
                out.push((s, e));
            }
        }
    }
    out
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
    chunk_words_with(words, MAX_WORDS_PER_CHUNK, WORD_OVERLAP)
}

pub fn chunk_words_with(words: &[Word], size: usize, overlap: usize) -> Vec<Vec<Word>> {
    let size = size.max(overlap + 1);
    if words.len() <= size {
        return vec![words.to_vec()];
    }
    let mut chunks = Vec::new();
    let mut start = 0;
    while start < words.len() {
        let end = (start + size).min(words.len());
        chunks.push(words[start..end].to_vec());
        if end >= words.len() {
            break;
        }
        start = end - overlap;
    }
    chunks
}

// ── post-processing configuration and confidence ─────────────────────────

/// How Layer 3 detections get their confidence.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub enum ConfidenceMode {
    /// Every detection 0.70 (Phase 1B).
    Flat,
    /// Per category, from the measured Layer 3 precision on the eval
    /// fixtures (see `category_confidence`).
    ByCategory,
}

/// Switches for every post-processing heuristic, so the eval harness can
/// measure each one (ablation). `Default` is what the app uses.
#[derive(Debug, Clone, Serialize)]
pub struct PostConfig {
    pub chunk_words: usize,
    pub chunk_overlap: usize,
    /// Drop legal roles / generic terms returned as names or companies.
    pub stoplist: bool,
    /// Also map "Last, First" ⇄ "First Last".
    pub last_first: bool,
    /// Also map the surname after an honorific ("Mr. Feeney").
    pub honorific_surname: bool,
    /// Map an accepted string to its occurrences in the whole page, not
    /// only in the chunk the model saw.
    pub page_wide: bool,
    pub matching: MatchOpts,
    pub confidence: ConfidenceMode,
}

impl Default for PostConfig {
    fn default() -> Self {
        PostConfig {
            chunk_words: MAX_WORDS_PER_CHUNK,
            chunk_overlap: WORD_OVERLAP,
            stoplist: true,
            last_first: true,
            honorific_surname: true,
            page_wide: true,
            matching: MatchOpts::default(),
            confidence: ConfidenceMode::ByCategory,
        }
    }
}

impl PostConfig {
    /// Phase 1B behaviour (for before/after comparisons).
    pub fn phase1b() -> Self {
        PostConfig {
            chunk_words: MAX_WORDS_PER_CHUNK,
            chunk_overlap: WORD_OVERLAP,
            stoplist: false,
            last_first: false,
            honorific_surname: false,
            page_wide: false,
            matching: MatchOpts { fold_accents: false, hyphen_wrap: false },
            confidence: ConfidenceMode::Flat,
        }
    }
}


/// Confidence by category for [`ConfidenceMode::ByCategory`], from the
/// precision of Layer 3's *additions* to Layers 1–2 on the eval fixtures,
/// pooled over the 7 models measured (docs/qvac-integration.md §11.3):
/// PersonName 96 % (n=224), BankAccount 100 % (21), NationalId 92 % (12),
/// DateOfBirth 100 % (9) → 0.80; CompanyName 89 % (37) → 0.70; Date 61 %
/// (23) → 0.60; Address 42 % (36), CaseNumber 13 % (23) → 0.50. Categories
/// Layer 3 rarely adds (too few samples) get 0.65. Two agreeing chunks add
/// 0.05; the maximum (0.85) stays below Layer 1's 0.95.
pub fn category_confidence(c: &PiiCategory) -> f64 {
    use PiiCategory::*;
    match c {
        PersonName | BankAccount | NationalId | DateOfBirth => 0.80,
        CompanyName => 0.70,
        Date => 0.60,
        Address | CaseNumber => 0.50,
        _ => 0.65,
    }
}

fn confidence_for(mode: ConfidenceMode, c: &PiiCategory, chunks_agreeing: usize) -> f64 {
    match mode {
        ConfidenceMode::Flat => CONFIDENCE,
        ConfidenceMode::ByCategory => {
            let base = category_confidence(c);
            let bonus = if chunks_agreeing >= 2 { 0.05 } else { 0.0 };
            ((base + bonus) * 100.0).round() / 100.0
        }
    }
}

// ── scan ─────────────────────────────────────────────────────────────────

/// What happened to the model's items, summed over a page.
#[derive(Debug, Default, Clone, Serialize, PartialEq)]
pub struct DropStats {
    /// Items returned by the model (after salvage).
    pub items: usize,
    /// Answers that were not valid JSON but had salvageable items.
    pub salvaged_chunks: usize,
    /// Elements of `entities` that were not `{text, category}`.
    pub invalid_items: usize,
    pub unknown_category: usize,
    pub empty_text: usize,
    pub implausible: usize,
    pub stoplisted: usize,
    /// Text does not occur in the chunk (hallucinated or rewritten).
    pub not_in_chunk: usize,
    /// Items accepted (each may map to several occurrences).
    pub accepted: usize,
    /// Occurrences added only through name variants.
    pub variant_hits: usize,
    /// Occurrences outside the chunk that produced them (page-wide mapping).
    pub page_wide_hits: usize,
    /// Detections dropped because Layers 1–2 already cover them.
    pub covered_by_layers_1_2: usize,
}

impl DropStats {
    pub fn add(&mut self, o: &DropStats) {
        self.items += o.items;
        self.salvaged_chunks += o.salvaged_chunks;
        self.invalid_items += o.invalid_items;
        self.unknown_category += o.unknown_category;
        self.empty_text += o.empty_text;
        self.implausible += o.implausible;
        self.stoplisted += o.stoplisted;
        self.not_in_chunk += o.not_in_chunk;
        self.accepted += o.accepted;
        self.variant_hits += o.variant_hits;
        self.page_wide_hits += o.page_wide_hits;
        self.covered_by_layers_1_2 += o.covered_by_layers_1_2;
    }
}

/// Result of scanning one page.
#[derive(Debug, Default)]
pub struct ScanReport {
    pub entities: Vec<PiiEntity>,
    pub chunks: usize,
    pub failed_chunks: usize,
    /// Set when the engine failed fatally; the remaining chunks were skipped.
    pub aborted: Option<String>,
    pub stats: DropStats,
}

struct Found {
    start: usize,
    end: usize,
    category: PiiCategory,
    chunks: BTreeSet<usize>,
}

fn add_found(found: &mut Vec<Found>, start: usize, end: usize, category: &PiiCategory, chunk: usize) {
    for f in found.iter_mut() {
        if start < f.end && f.start < end {
            if start <= f.start && end >= f.end && (start, end) != (f.start, f.end) {
                // the new span contains the old one: widen it
                f.start = start;
                f.end = end;
                f.category = category.clone();
            }
            f.chunks.insert(chunk);
            return;
        }
    }
    let mut chunks = BTreeSet::new();
    chunks.insert(chunk);
    found.push(Found { start, end, category: category.clone(), chunks });
}

/// Turn one chunk's parsed answer into occurrences (page offsets).
fn map_items(
    text: &str,
    c_start: usize,
    c_end: usize,
    items: &[RawEntity],
    chunk_idx: usize,
    cfg: &PostConfig,
    stats: &mut DropStats,
    found: &mut Vec<Found>,
) {
    let chunk_text = &text[c_start..c_end];
    let mut seen: BTreeSet<(String, String)> = BTreeSet::new();
    for ent in items {
        stats.items += 1;
        if !seen.insert((ent.text.trim().to_lowercase(), ent.category.clone())) {
            continue; // listed twice in one answer
        }
        let Some(category) = parse_category(&ent.category) else {
            stats.unknown_category += 1;
            continue;
        };
        let needle = ent.text.trim();
        if needle.is_empty() {
            stats.empty_text += 1;
            continue;
        }
        if !plausible(needle, &category, chunk_text.len()) {
            stats.implausible += 1;
            continue;
        }
        if cfg.stoplist && stoplisted(needle, &category) {
            stats.stoplisted += 1;
            continue;
        }
        let in_chunk = find_occurrences_with(chunk_text, needle, cfg.matching);
        if in_chunk.is_empty() {
            stats.not_in_chunk += 1;
            continue;
        }
        stats.accepted += 1;
        let (hay, base) = if cfg.page_wide { (text, 0) } else { (chunk_text, c_start) };
        let mut hits: Vec<((usize, usize), bool)> =
            find_occurrences_with(hay, needle, cfg.matching).into_iter().map(|r| (r, false)).collect();
        if category == PiiCategory::PersonName {
            if cfg.last_first {
                for v in last_first_variants(needle) {
                    hits.extend(find_occurrences_with(hay, &v, cfg.matching).into_iter().map(|r| (r, true)));
                }
            }
            if cfg.honorific_surname {
                hits.extend(honorific_surname_hits(hay, needle, cfg.matching).into_iter().map(|r| (r, true)));
            }
        }
        for ((s, e), via_variant) in hits {
            let (start, end) = (base + s, base + e);
            let before = found.len();
            add_found(found, start, end, &category, chunk_idx);
            if found.len() > before {
                if via_variant {
                    stats.variant_hits += 1;
                }
                if start >= c_end || end <= c_start {
                    stats.page_wide_hits += 1;
                }
            }
        }
    }
}

/// Run Layer 3 on one page of text with the app's post-processing.
pub fn scan(
    engine: &dyn Layer3Engine,
    text: &str,
    page: u32,
    existing_ranges: &[(usize, usize)],
    on_progress: Option<&ProgressFn>,
) -> ScanReport {
    scan_with(engine, text, page, existing_ranges, on_progress, &PostConfig::default())
}

pub fn scan_with(
    engine: &dyn Layer3Engine,
    text: &str,
    page: u32,
    existing_ranges: &[(usize, usize)],
    on_progress: Option<&ProgressFn>,
    cfg: &PostConfig,
) -> ScanReport {
    let mut report = ScanReport::default();
    if text.trim().is_empty() {
        return report;
    }
    let words = tokenize_words(text);
    if words.is_empty() {
        return report;
    }
    let chunks = chunk_words_with(&words, cfg.chunk_words, cfg.chunk_overlap);
    report.chunks = chunks.len();
    let mut found: Vec<Found> = Vec::new();

    for (i, chunk) in chunks.iter().enumerate() {
        if let Some(cb) = on_progress {
            cb(i, &format!("chunk {}/{}", i + 1, chunks.len()));
        }
        let c_start = chunk[0].start;
        let c_end = chunk[chunk.len() - 1].end;
        let raw = match engine.complete(&text[c_start..c_end]) {
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
        let parsed = match parse_strict(&raw) {
            Ok(p) => p,
            Err(reason) => {
                log::warn!("Layer 3: chunk {}/{} on page {} skipped: {}", i + 1, chunks.len(), page, reason);
                report.failed_chunks += 1;
                continue;
            }
        };
        report.stats.invalid_items += parsed.invalid_items;
        if parsed.salvaged {
            report.stats.salvaged_chunks += 1;
        }
        map_items(text, c_start, c_end, &parsed.items, i, cfg, &mut report.stats, &mut found);
    }

    for f in found {
        if existing_ranges.iter().any(|(es, ee)| f.start < *ee && *es < f.end) {
            report.stats.covered_by_layers_1_2 += 1;
            continue;
        }
        report.entities.push(PiiEntity {
            id: uuid::Uuid::new_v4().to_string(),
            text: text[f.start..f.end].to_string(),
            start: f.start,
            end: f.end,
            page,
            replacement: f.category.replacement_label().to_string(),
            confidence: confidence_for(cfg.confidence, &f.category, f.chunks.len()),
            category: f.category,
            layer: LAYER.to_string(),
        });
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
        answers: RefCell<Vec<Result<String, Layer3Error>>>,
        seen: RefCell<Vec<String>>,
    }
    impl Fake {
        fn new(answers: Vec<Result<String, Layer3Error>>) -> Self {
            Self { answers: RefCell::new(answers), seen: RefCell::new(vec![]) }
        }
        fn ents(list: Vec<Vec<(&str, &str)>>) -> Self {
            Self::new(list.into_iter().map(|l| Ok(answer(&l))).collect())
        }
    }
    impl Layer3Engine for Fake {
        fn complete(&self, chunk_text: &str) -> Result<String, Layer3Error> {
            self.seen.borrow_mut().push(chunk_text.to_string());
            let mut a = self.answers.borrow_mut();
            if a.is_empty() { Ok(r#"{"entities":[]}"#.into()) } else { a.remove(0) }
        }
    }
    fn answer(items: &[(&str, &str)]) -> String {
        json!({"entities": items.iter().map(|(t, c)| json!({"text": t, "category": c})).collect::<Vec<_>>()}).to_string()
    }
    fn found(text: &str, r: &ScanReport) -> Vec<String> {
        r.entities.iter().map(|e| text[e.start..e.end].to_string()).collect()
    }

    #[test]
    fn maps_all_occurrences_case_and_whitespace_tolerant() {
        let text = "Maria GONZALEZ signed. Later maria  gonzalez\nand Maria\tGonzalez agreed. Mariana Gonzalezz no.";
        let fake = Fake::ents(vec![vec![("Maria Gonzalez", "PersonName")]]);
        let r = scan_with(&fake, text, 0, &[], None, &PostConfig::phase1b());
        assert_eq!(found(text, &r), vec!["Maria GONZALEZ", "maria  gonzalez", "Maria\tGonzalez"]);
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
        let fake = Fake::ents(vec![vec![
            ("jane@example.com", "Email"),                               // already found by L1
            ("Jane Roe", "PersonName"),                                  // new
            ("The court finds the motion without merit.", "CaseNumber"), // hallucinated sentence
            ("Jane Roe", "NotACategory"),                                // unknown category
            ("Nobody Here", "PersonName"),                               // not in text
            ("The Court", "PersonName"),                                 // legal term
        ]]);
        let r = scan(&fake, text, 2, &existing, None);
        assert_eq!(found(text, &r), vec!["Jane Roe"]);
        assert_eq!(r.entities[0].page, 2);
        let s = &r.stats;
        assert_eq!((s.items, s.accepted), (6, 2));
        assert_eq!((s.implausible, s.unknown_category, s.not_in_chunk, s.stoplisted), (1, 1, 1, 1));
        assert_eq!(s.covered_by_layers_1_2, 1);
    }

    #[test]
    fn strict_parsing_drops_and_counts_bad_items() {
        let p = parse_strict(r#"{"entities":[{"text":"A B","category":"PersonName"},{"text":5,"category":"Email"},{"text":"x","category":"Email","extra":1},"str"]}"#).unwrap();
        assert_eq!(p.items, vec![RawEntity { text: "A B".into(), category: "PersonName".into() }]);
        assert_eq!(p.invalid_items, 3);
        assert!(!p.salvaged);
        assert!(parse_strict("not json").is_err());
        assert!(parse_strict(r#"{"items":[]}"#).is_err());
        assert!(parse_strict(r#"[]"#).is_err());
        // Truncated at the token limit: complete items are salvaged.
        let p = parse_strict(r#"{"entities":[{"text":"Ann \"Lee\"","category":"PersonName"},{"text":"12 Oak"#).unwrap();
        assert!(p.salvaged);
        assert_eq!(p.items, vec![RawEntity { text: "Ann \"Lee\"".into(), category: "PersonName".into() }]);
        // llama.cpp's grammar may emit the keys in schema order or not.
        let p = parse_strict(r#"{"entities":[{"category":"Address","text":"Alameda, CA 94501"},{"category":"Address","text":"Alam"#).unwrap();
        assert_eq!(p.items, vec![RawEntity { text: "Alameda, CA 94501".into(), category: "Address".into() }]);
    }

    #[test]
    fn dedupes_across_overlapping_chunks_and_keeps_offsets_global() {
        // 300 words → 3 chunks; the name sits in the overlap of chunks 0 and 1.
        let mut words: Vec<String> = (0..300).map(|i| format!("w{}", i)).collect();
        words[140] = "Olga".into();
        words[141] = "Petrova".into();
        let text = words.join(" ");
        let fake = Fake::ents(vec![vec![("Olga", "PersonName")], vec![("Olga Petrova", "PersonName")], vec![]]);
        let r = scan(&fake, &text, 0, &[], None);
        assert_eq!(fake.seen.borrow().len(), 3);
        assert_eq!(found(&text, &r), vec!["Olga Petrova"], "the longer span from chunk 2 replaces chunk 1's");
        assert_eq!(r.entities[0].confidence, 0.85, "two chunks agree: PersonName 0.80 + 0.05");
    }

    #[test]
    fn page_wide_mapping_and_name_variants() {
        let mut words: Vec<String> = (0..300).map(|i| format!("w{}", i)).collect();
        words[10] = "Dana".into();
        words[11] = "Holloway".into();
        words[250] = "HOLLOWAY,".into();
        words[251] = "Dana".into();
        words[270] = "Ms.".into();
        words[271] = "Holloway".into();
        words[280] = "Holloway".into(); // no honorific: not mapped
        let text = words.join(" ");
        let fake = Fake::ents(vec![vec![("Dana Holloway", "PersonName")]]);
        let r = scan(&fake, &text, 0, &[], None);
        assert_eq!(found(&text, &r), vec!["Dana Holloway", "HOLLOWAY, Dana", "Holloway"]);
        assert_eq!(r.stats.variant_hits, 2);
        assert_eq!(r.stats.page_wide_hits, 2);
        // Phase 1B behaviour: only the chunk, only the literal string.
        let fake = Fake::ents(vec![vec![("Dana Holloway", "PersonName")]]);
        let r = scan_with(&fake, &text, 0, &[], None, &PostConfig::phase1b());
        assert_eq!(found(&text, &r), vec!["Dana Holloway"]);
    }

    #[test]
    fn accent_and_line_wrap_tolerant_matching() {
        let o = MatchOpts { fold_accents: true, hyphen_wrap: true };
        let text = "Sra. Ferrandiz Olmedo; e-mail j.richardson@atlas-  \n  digital.com";
        assert_eq!(find_occurrences_with(text, "Ferrándiz Olmedo", o), vec![(5, 21)]);
        let (s, e) = find_occurrences_with(text, "j.richardson@atlas-digital.com", o)[0];
        assert_eq!(&text[s..e], "j.richardson@atlas-  \n  digital.com");
        assert!(find_occurrences(text, "Ferrándiz Olmedo").is_empty(), "strict matching is accent-sensitive");
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
        // Unparseable answers fail the chunk, not the page.
        let fake = Fake::new(vec![Ok("garbage".into())]);
        let r = scan(&fake, "Short text by Ann Lee.", 0, &[], None);
        assert_eq!((r.chunks, r.failed_chunks, r.aborted.is_none()), (1, 1, true));
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
    fn schema_prompts_and_categories_cover_every_pii_category() {
        for name in CATEGORY_NAMES {
            let cat = parse_category(name).unwrap_or_else(|| panic!("{} is not a PiiCategory", name));
            assert_eq!(&cat.to_string(), name);
        }
        assert_eq!(CATEGORY_NAMES.len(), 23);
        let schema = response_schema();
        assert_eq!(schema["properties"]["entities"]["items"]["properties"]["category"]["enum"].as_array().unwrap().len(), 23);
        for v in [PromptVersion::P1b, PromptVersion::P3] {
            let m = messages(v, "chunk");
            assert!(m[0].1.contains("MedicalRecordNumber"), "{:?}", v);
            assert_eq!(m.last().unwrap(), &("user", "Document:\nchunk".to_string()));
            // every example answer is valid, uses known categories and quotes its document
            for pair in m[1..m.len() - 1].chunks(2) {
                let p = parse_strict(&pair[1].1).unwrap();
                for it in p.items {
                    assert!(parse_category(&it.category).is_some());
                    assert!(pair[0].1.contains(&it.text), "{:?}: {} not in example", v, it.text);
                }
            }
            assert_eq!(PromptVersion::parse(v.id()), Some(v));
        }
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

    #[test]
    fn stoplist_rules() {
        use PiiCategory::*;
        assert!(stoplisted("the Tenant", &PersonName));
        assert!(stoplisted("EL ARRENDATARIO", &PersonName));
        assert!(stoplisted("Confidential Information", &CompanyName));
        assert!(stoplisted("socio", &PersonName), "no capital letter");
        assert!(!stoplisted("Dana Holloway", &PersonName));
        assert!(!stoplisted("Client", &ClientId), "numeric categories are not stop-listed");
    }

    #[test]
    fn last_first_variants_cover_both_orders() {
        assert_eq!(last_first_variants("Dana Holloway"), vec!["Holloway, Dana"]);
        assert_eq!(last_first_variants("Holloway, Dana"), vec!["Dana Holloway"]);
        assert!(last_first_variants("María Inmaculada Ferrándiz Olmedo").contains(&"Ferrándiz Olmedo, María Inmaculada".to_string()));
    }
}
