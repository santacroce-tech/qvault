//! Per-category analysis of an entity string ([`Form`]) and the rules that
//! decide whether two strings name the same entity ([`compare`]).
//!
//! The rules are deliberately conservative:
//! * [`Level::Same`] is returned only when nothing but formatting differs
//!   (case, accents, honorifics, "Last, First" order, a company's legal
//!   form being present or absent, separators in a number) or when one name
//!   extends the other with extra *full* names. These merges are applied
//!   automatically.
//! * Anything that relies on an initial, a surname alone, a partial company
//!   name or a near-miss spelling is [`Level::Likely`]: proposed for review,
//!   never applied silently.
//! * [`Level::Conflict`] marks pairs that are positively different (another
//!   given name with the same surname, Mr vs Ms, Jr vs Sr, another house
//!   number). A conflict anywhere between two groups blocks automatic
//!   merging and proposals between them, which keeps a short name from
//!   chaining two different people together.

use serde::Serialize;

use super::normalize::{self, fold, words};

/// How a category is compared.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Kind {
    Person,
    Company,
    Address,
    Email,
    Phone,
    /// IDs, account and card numbers, plates: alphanumerics only.
    Identifier,
    Url,
    /// Dates and amounts: equal text only.
    Literal,
}

pub fn kind_of(category: &str) -> Kind {
    match category {
        "PersonName" => Kind::Person,
        "CompanyName" => Kind::Company,
        "Address" => Kind::Address,
        "Email" => Kind::Email,
        "PhoneNumber" => Kind::Phone,
        "Url" => Kind::Url,
        "Date" | "DateOfBirth" | "MonetaryAmount" => Kind::Literal,
        _ => Kind::Identifier,
    }
}

/// Categories where an embedding of the text can help (names, companies,
/// addresses). Numbers and e-mails are compared exactly.
pub fn embeddable(category: &str) -> bool {
    matches!(kind_of(category), Kind::Person | Kind::Company | Kind::Address)
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Tok {
    /// Folded text (lowercase, no accents).
    pub s: String,
    /// A single letter ("M." / "M").
    pub initial: bool,
}

/// An analysed entity string.
#[derive(Debug, Clone, Serialize)]
pub struct Form {
    pub kind: Kind,
    /// Identity key: two strings of one category with the same key are the
    /// same entity (formatting differences only).
    pub key: String,
    /// Person: given names then surnames (connectors dropped). Company: the
    /// name without its legal form. Address: canonical tokens.
    pub toks: Vec<Tok>,
    /// Person / company: folded tokens as written after dropping honorifics
    /// (connectors kept, legal form kept for companies), in reading order
    /// "First … Last". Used to search page text.
    pub surface: Vec<String>,
    /// Person: gender implied by an honorific (Mr / Ms, Sr / Sra …).
    pub gender: Option<char>,
    /// Person: Jr / Filho / III …
    pub generation: Option<&'static str>,
    /// Person: written with an honorific.
    pub honorific: bool,
    /// Person: written "Last, First".
    pub reordered: bool,
    /// Company: canonical legal form(s), e.g. "llc", "sa+cv".
    pub legal: Option<String>,
    /// Company: the surface tokens that make up the legal form (suffix).
    pub legal_len: usize,
}

impl Form {
    fn empty(kind: Kind, key: String) -> Self {
        Form {
            kind,
            key,
            toks: vec![],
            surface: vec![],
            gender: None,
            generation: None,
            honorific: false,
            reordered: false,
            legal: None,
            legal_len: 0,
        }
    }

    pub fn full_tokens(&self) -> impl Iterator<Item = &Tok> {
        self.toks.iter().filter(|t| !t.initial)
    }

    pub fn has_initials(&self) -> bool {
        self.toks.iter().any(|t| t.initial)
    }
}

pub fn analyse(category: &str, text: &str) -> Form {
    match kind_of(category) {
        Kind::Person => person(text),
        Kind::Company => company(text),
        Kind::Address => address(text),
        Kind::Email => {
            let k = fold(text.trim()).trim_start_matches("mailto:").trim_matches(|c: char| !c.is_alphanumeric()).to_string();
            Form::empty(Kind::Email, k)
        }
        Kind::Phone => Form::empty(Kind::Phone, text.chars().filter(|c| c.is_ascii_digit()).collect()),
        Kind::Identifier => Form::empty(
            Kind::Identifier,
            fold(text).chars().filter(|c| c.is_alphanumeric()).collect::<String>().to_uppercase(),
        ),
        Kind::Url => {
            let mut k = fold(text.trim());
            for p in ["https://", "http://"] {
                if let Some(r) = k.strip_prefix(p) {
                    k = r.to_string();
                }
            }
            let k = k.trim_start_matches("www.").trim_end_matches('/').to_string();
            Form::empty(Kind::Url, k)
        }
        Kind::Literal => Form::empty(Kind::Literal, fold(text).split_whitespace().collect::<Vec<_>>().join(" ")),
    }
}

// ── people ───────────────────────────────────────────────────────────────

/// One whitespace-separated piece of the original, folded, with whether it
/// carried a dot or an ordinal mark ("Dª", "Da.") — that disambiguates the
/// honorific "Da." from the connector "da".
struct Raw {
    folded: String,
    marked: bool,
}

fn raw_tokens(s: &str) -> Vec<Raw> {
    let mut out = Vec::new();
    for piece in s.split(|c: char| c.is_whitespace() || c == ',' || c == ';' || c == '(' || c == ')') {
        if piece.is_empty() {
            continue;
        }
        let marked = piece.contains('.') || piece.contains('ª') || piece.contains('º');
        let f = fold(piece);
        // "J.M." → initials "j", "m"; "Sr." → "sr"
        let parts: Vec<String> = f.split('.').map(|p| p.trim_matches(|c: char| !c.is_alphanumeric() && c != '-' && c != '\'').to_string()).filter(|p| !p.is_empty()).collect();
        if parts.len() > 1 && parts.iter().all(|p| p.chars().count() == 1) {
            for p in parts {
                out.push(Raw { folded: p, marked: true });
            }
        } else {
            let joined: String = parts.join("");
            if !joined.is_empty() {
                out.push(Raw { folded: joined, marked });
            }
        }
    }
    out
}

fn person(text: &str) -> Form {
    let text = text.trim().trim_matches(|c: char| c == '"' || c == '“' || c == '”' || c == '\'');
    // "Last, First" — unless what follows the comma is only a suffix
    // ("Smith, Jr.", "Smith, Esq.").
    let (mut first_part, mut last_part, mut reordered) = (text.to_string(), String::new(), false);
    if let Some((l, r)) = text.split_once(',') {
        let rt = raw_tokens(r);
        let meaningful = rt
            .iter()
            .any(|t| normalize::generational(&t.folded).is_none() && !normalize::POSTNOMINALS.contains(&t.folded.as_str()));
        if meaningful && !raw_tokens(l).is_empty() {
            first_part = r.to_string();
            last_part = l.to_string();
            reordered = true;
        }
    }
    let mut toks: Vec<Raw> = raw_tokens(&first_part);
    // Honorifics can sit on either part ("Santos, Sra. Maria").
    let mut last_toks = raw_tokens(&last_part);
    let mut gender = None;
    let mut honorific = false;
    for list in [&mut toks, &mut last_toks] {
        loop {
            let Some(t) = list.first() else { break };
            let h = match normalize::honorific(&t.folded) {
                Some(h) => h,
                None => break,
            };
            // "D." / "Da." are honorifics only when marked and followed by a
            // full name; "D. Santos" is an initial, "da Silva" a connector.
            let ambiguous = matches!(t.folded.as_str(), "d" | "da");
            if ambiguous && (!t.marked || list.len() < 3) {
                break;
            }
            if list.len() < 2 {
                break;
            }
            honorific = true;
            if h.is_some() {
                gender = h;
            }
            list.remove(0);
        }
    }
    let mut all: Vec<Raw> = toks;
    all.extend(last_toks);
    // Trailing generational suffixes and post-nominals.
    let mut generation = None;
    while let Some(t) = all.last() {
        if all.len() < 2 {
            break;
        }
        if let Some(g) = normalize::generational(&t.folded) {
            generation = Some(g);
            all.pop();
        } else if matches!(t.folded.as_str(), "sr" | "senior") && all.len() >= 3 {
            // A trailing "Sr." is the generational suffix, not the Spanish title.
            generation = Some("sr");
            all.pop();
        } else if normalize::POSTNOMINALS.contains(&t.folded.as_str()) {
            all.pop();
        } else {
            break;
        }
    }
    let surface: Vec<String> = all.iter().map(|t| t.folded.clone()).collect();
    let mut name: Vec<Tok> = all
        .iter()
        .filter(|t| !normalize::is_connector(&t.folded) || all.len() == 1)
        .map(|t| Tok { initial: t.folded.chars().count() == 1 && t.folded.chars().all(|c| c.is_alphabetic()), s: t.folded.clone() })
        .collect();
    if name.is_empty() {
        name = all.iter().map(|t| Tok { initial: false, s: t.folded.clone() }).collect();
    }
    let mut key = name.iter().map(|t| t.s.as_str()).collect::<Vec<_>>().join(" ");
    if let Some(g) = generation {
        key.push_str(" #");
        key.push_str(g);
    }
    Form {
        kind: Kind::Person,
        key,
        toks: name,
        surface,
        gender,
        generation,
        honorific,
        reordered,
        legal: None,
        legal_len: 0,
    }
}

// ── companies ────────────────────────────────────────────────────────────

fn company(text: &str) -> Form {
    // "S.A." → "SA", "S/A" → "SA", "L.L.C." → "LLC". Keep "&".
    let cleaned: String = fold(text.trim()).chars().filter(|c| *c != '.' && *c != '/').collect();
    let mut tks: Vec<String> = words(&cleaned);
    // Join runs of single letters at the end ("s a" → "sa", "s de r l").
    let mut joined: Vec<String> = Vec::new();
    for t in tks.drain(..) {
        let single = t.chars().count() == 1 && t != "&";
        if single {
            if let Some(prev) = joined.last_mut() {
                if prev.chars().all(|c| c.is_alphabetic()) && prev.chars().count() < 4 && legal_prefix(prev) {
                    prev.push_str(&t);
                    continue;
                }
            }
        }
        joined.push(t);
    }
    let surface = joined.clone();
    let mut core = joined;
    // Strip legal forms (and written-out phrases) from the end.
    let mut legal: Vec<&'static str> = Vec::new();
    let mut legal_len = 0usize;
    loop {
        if core.len() <= 1 {
            break;
        }
        let tail = core.join(" ");
        if let Some((phrase, class)) = normalize::LEGAL_PHRASES.iter().find(|(p, _)| tail.ends_with(&format!(" {}", p))) {
            let n = phrase.split(' ').count();
            if core.len() > n {
                core.truncate(core.len() - n);
                legal.push(class);
                legal_len += n;
                continue;
            }
        }
        let last = core.last().unwrap().clone();
        if let Some(class) = normalize::legal_form(&last) {
            core.pop();
            legal.push(class);
            legal_len += 1;
            continue;
        }
        // "S.A. de C.V.", "& Co."
        if !legal.is_empty() && matches!(last.as_str(), "de" | "&" | "and" | "y" | "e") {
            core.pop();
            legal_len += 1;
            continue;
        }
        break;
    }
    legal.reverse();
    legal.dedup();
    let core: Vec<String> = core
        .into_iter()
        .enumerate()
        .filter(|(i, t)| !(normalize::COMPANY_FILLERS.contains(&t.as_str()) && (*i == 0 || t == "&" || t == "and" || t == "und" || t == "y" || t == "e")))
        .map(|(_, t)| t)
        .collect();
    let legal = if legal.is_empty() { None } else { Some(legal.join("+")) };
    let key = format!("{}|{}", core.join(" "), legal.clone().unwrap_or_default());
    Form {
        kind: Kind::Company,
        key,
        toks: core.into_iter().map(|s| Tok { s, initial: false }).collect(),
        surface,
        gender: None,
        generation: None,
        honorific: false,
        reordered: false,
        legal,
        legal_len,
    }
}

/// Could `p` be the start of a dotted legal form ("s" of "s.a.", "sde" …)?
fn legal_prefix(p: &str) -> bool {
    matches!(p, "s" | "l" | "c" | "n" | "b" | "sa" | "sd" | "sde" | "sder" | "ll" | "lt" | "e" | "r")
}

// ── addresses ────────────────────────────────────────────────────────────

fn address(text: &str) -> Form {
    let cleaned: String = fold(text.trim()).chars().map(|c| if c == '.' || c == '/' || c == '#' { ' ' } else { c }).collect();
    let toks: Vec<Tok> = words(&cleaned)
        .into_iter()
        .filter(|t| !normalize::is_connector(t))
        .map(|t| {
            let canon = normalize::ADDRESS_ABBREV.iter().find(|(a, _)| *a == t).map(|(_, c)| c.to_string()).unwrap_or(t);
            Tok { s: canon, initial: false }
        })
        .collect();
    let key = toks.iter().map(|t| t.s.as_str()).collect::<Vec<_>>().join(" ");
    let surface = words(&fold(text));
    Form { surface, toks, ..Form::empty(Kind::Address, key) }
}

// ── comparison ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Level {
    Conflict,
    Unrelated,
    Likely,
    Same,
}

/// Why two strings were linked (shown to the reviewer).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Rule {
    /// Identical after case/accent/honorific/spacing normalisation.
    SameName,
    /// "Last, First" vs "First Last".
    Reordered,
    /// Extra full given names or surnames ("Ana Ferrándiz" / "Ana Ferrándiz Olmedo").
    NameExtension,
    /// A company with and without its legal form ("Acme" / "Acme LLC").
    LegalForm,
    /// Same number written with other separators / a country code.
    SameNumber,
    /// Initials match ("M. Santos" / "Maria Santos").
    Initial,
    /// Only the surname matches ("Ms. Santos" / "Maria Santos").
    SurnameOnly,
    /// Part of a name matches (a middle name used as first name, a shorter company name).
    PartialName,
    /// Near-identical spelling (typos, OCR).
    SimilarSpelling,
    /// Same company name with a different legal form (possibly another legal entity).
    DifferentLegalForm,
    /// An acronym of the other name.
    Acronym,
    /// Most of an address matches, same numbers.
    SimilarAddress,
    /// The embedding model found the two very similar.
    Embedding,
    /// Not linked.
    None,
}

impl Rule {
    /// Order for picking the best explanation (lower = stronger).
    pub fn strength(self) -> u8 {
        match self {
            Rule::SameName => 0,
            Rule::Reordered => 1,
            Rule::SameNumber => 2,
            Rule::LegalForm => 3,
            Rule::NameExtension => 4,
            Rule::SimilarSpelling => 5,
            Rule::Initial => 6,
            Rule::SimilarAddress => 7,
            Rule::SurnameOnly => 8,
            Rule::PartialName => 9,
            Rule::DifferentLegalForm => 10,
            Rule::Acronym => 11,
            Rule::Embedding => 12,
            Rule::None => 13,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Rule::SameName => "same-name",
            Rule::Reordered => "reordered",
            Rule::NameExtension => "name-extension",
            Rule::LegalForm => "legal-form",
            Rule::SameNumber => "same-number",
            Rule::Initial => "initial",
            Rule::SurnameOnly => "surname-only",
            Rule::PartialName => "partial-name",
            Rule::SimilarSpelling => "similar-spelling",
            Rule::DifferentLegalForm => "different-legal-form",
            Rule::Acronym => "acronym",
            Rule::SimilarAddress => "similar-address",
            Rule::Embedding => "embedding",
            Rule::None => "none",
        }
    }

    pub fn parse(s: &str) -> Rule {
        [
            Rule::SameName, Rule::Reordered, Rule::NameExtension, Rule::LegalForm, Rule::SameNumber, Rule::Initial,
            Rule::SurnameOnly, Rule::PartialName, Rule::SimilarSpelling, Rule::DifferentLegalForm, Rule::Acronym,
            Rule::SimilarAddress, Rule::Embedding,
        ]
        .into_iter()
        .find(|r| r.as_str() == s)
        .unwrap_or(Rule::None)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Verdict {
    pub level: Level,
    pub rule: Rule,
    /// String similarity behind the decision, 0..1.
    pub score: f32,
}

impl Verdict {
    fn new(level: Level, rule: Rule, score: f32) -> Self {
        Verdict { level, rule, score }
    }
    fn unrelated() -> Self {
        Verdict::new(Level::Unrelated, Rule::None, 0.0)
    }
    fn conflict() -> Self {
        Verdict::new(Level::Conflict, Rule::None, 0.0)
    }
    /// Does this pair forbid putting the two strings in one group without a
    /// human decision?
    pub fn blocks_auto(&self) -> bool {
        self.level == Level::Conflict || self.rule == Rule::DifferentLegalForm
    }
}

/// Compare two analysed strings of the **same category**. (Different
/// categories are never compared: callers group by category first.)
pub fn compare(a: &Form, b: &Form) -> Verdict {
    if a.kind != b.kind {
        return Verdict::conflict();
    }
    if a.key.is_empty() || b.key.is_empty() {
        return Verdict::unrelated();
    }
    match a.kind {
        Kind::Person => compare_people(a, b),
        Kind::Company => compare_companies(a, b),
        Kind::Address => compare_addresses(a, b),
        Kind::Phone => compare_phones(a, b),
        Kind::Email | Kind::Identifier | Kind::Url | Kind::Literal => {
            if a.key == b.key {
                Verdict::new(Level::Same, if a.kind == Kind::Identifier { Rule::SameNumber } else { Rule::SameName }, 1.0)
            } else {
                Verdict::unrelated()
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum TokMatch {
    Exact,
    Fuzzy(f32),
    Initial,
}

fn tok_match(x: &Tok, y: &Tok) -> Option<TokMatch> {
    match (x.initial, y.initial) {
        (false, false) => {
            if x.s == y.s {
                Some(TokMatch::Exact)
            } else if x.s.chars().count() >= 4 && y.s.chars().count() >= 4 {
                let s = jaro_winkler(&x.s, &y.s);
                // One edit in a long token; first letter must agree.
                (s >= 0.92 && x.s.chars().next() == y.s.chars().next() && edit_distance(&x.s, &y.s) <= 1 + (x.s.len().min(y.s.len()) >= 9) as usize)
                    .then_some(TokMatch::Fuzzy(s))
            } else {
                None
            }
        }
        (true, false) => y.s.starts_with(&x.s).then_some(TokMatch::Initial),
        (false, true) => x.s.starts_with(&y.s).then_some(TokMatch::Initial),
        (true, true) => (x.s == y.s).then_some(TokMatch::Initial),
    }
}

/// Order-preserving alignment of every token of `s` to a distinct token of
/// `l` (|s| ≤ |l|). Returns the target index of each token of `s` and the
/// match kinds, preferring the alignment with the most exact matches.
fn align(s: &[Tok], l: &[Tok]) -> Option<(Vec<usize>, Vec<TokMatch>)> {
    fn go(s: &[Tok], l: &[Tok], i: usize, from: usize, idx: &mut Vec<usize>, kinds: &mut Vec<TokMatch>, best: &mut Option<(usize, Vec<usize>, Vec<TokMatch>)>) {
        if i == s.len() {
            let exact = kinds.iter().filter(|k| **k == TokMatch::Exact).count();
            if best.as_ref().map(|b| exact > b.0).unwrap_or(true) {
                *best = Some((exact, idx.clone(), kinds.clone()));
            }
            return;
        }
        // Not enough tokens left in `l`.
        if l.len() - from < s.len() - i {
            return;
        }
        for j in from..l.len() {
            if let Some(k) = tok_match(&s[i], &l[j]) {
                idx.push(j);
                kinds.push(k);
                go(s, l, i + 1, j + 1, idx, kinds, best);
                idx.pop();
                kinds.pop();
            }
        }
    }
    if s.is_empty() || s.len() > l.len() || l.len() > 10 {
        return None;
    }
    let mut best = None;
    go(s, l, 0, 0, &mut Vec::new(), &mut Vec::new(), &mut best);
    best.map(|(_, i, k)| (i, k))
}

fn compare_people(a: &Form, b: &Form) -> Verdict {
    let gender_conflict = matches!((a.gender, b.gender), (Some(x), Some(y)) if x != y);
    let gen_conflict = matches!((a.generation, b.generation), (Some(x), Some(y)) if x != y);
    if gender_conflict || gen_conflict {
        return Verdict::conflict();
    }
    if a.key == b.key {
        let rule = if a.reordered != b.reordered { Rule::Reordered } else { Rule::SameName };
        return Verdict::new(Level::Same, rule, 1.0);
    }
    let gen_differs = a.generation != b.generation; // one has Jr, the other nothing
    let (s, l) = if a.toks.len() < b.toks.len() || (a.toks.len() == b.toks.len() && a.has_initials()) { (a, b) } else { (b, a) };
    let (st, lt) = (&s.toks, &l.toks);
    let shared_exact = st.iter().filter(|t| !t.initial && lt.iter().any(|u| !u.initial && u.s == t.s)).count();

    if let Some((idx, kinds)) = align(st, lt) {
        let n_init = kinds.iter().filter(|k| **k == TokMatch::Initial).count();
        let fuzzy: Vec<f32> = kinds.iter().filter_map(|k| if let TokMatch::Fuzzy(x) = k { Some(*x) } else { None }).collect();
        let n_exact = kinds.iter().filter(|k| **k == TokMatch::Exact).count();
        let last_s = *idx.last().unwrap();
        let surname_ok = last_s + 1 == lt.len() || (lt.len() >= 3 && last_s + 2 == lt.len());
        let first_ok = idx[0] == 0;
        let min_fuzzy = fuzzy.iter().cloned().fold(1.0f32, f32::min);
        let cap = |v: Verdict| if gen_differs && v.level == Level::Same { Verdict::new(Level::Likely, v.rule, v.score) } else { v };

        if st.len() == 1 {
            // A single token: a surname alone ("Santos", "Ms. Santos").
            if surname_ok && !st[0].initial && lt.len() >= 2 {
                return cap(Verdict::new(Level::Likely, Rule::SurnameOnly, if fuzzy.is_empty() { 0.6 } else { 0.5 * min_fuzzy }));
            }
            return Verdict::unrelated();
        }
        if !surname_ok {
            return Verdict::unrelated();
        }
        // The surname itself must be a real match (not an initial).
        if kinds.last() == Some(&TokMatch::Initial) {
            return Verdict::unrelated();
        }
        if n_init == 0 && fuzzy.is_empty() && first_ok {
            // Same full names; the longer adds full names.
            if lt.iter().any(|t| t.initial) {
                // "Maria Santos" vs "Maria J. Santos": the extra is an initial.
                return cap(Verdict::new(Level::Likely, Rule::Initial, 0.8));
            }
            return cap(Verdict::new(Level::Same, Rule::NameExtension, 0.9));
        }
        if n_init > 0 && n_exact >= 1 && fuzzy.is_empty() {
            return cap(Verdict::new(Level::Likely, Rule::Initial, 0.7));
        }
        if !fuzzy.is_empty() && n_init == 0 && first_ok {
            return cap(Verdict::new(Level::Likely, Rule::SimilarSpelling, min_fuzzy));
        }
        if !first_ok && n_exact >= 2 {
            return cap(Verdict::new(Level::Likely, Rule::PartialName, 0.6));
        }
        return Verdict::unrelated();
    }
    // No alignment. Sharing a full name token while disagreeing elsewhere is
    // a positive difference when both carry a given name: "Maria Santos" vs
    // "João Santos", "J. Santos" vs "M. Santos".
    if shared_exact >= 1 && st.len() >= 2 && lt.len() >= 2 {
        return Verdict::conflict();
    }
    Verdict::unrelated()
}

fn compare_companies(a: &Form, b: &Form) -> Verdict {
    let ca: Vec<&str> = a.toks.iter().map(|t| t.s.as_str()).collect();
    let cb: Vec<&str> = b.toks.iter().map(|t| t.s.as_str()).collect();
    if ca.is_empty() || cb.is_empty() {
        return Verdict::unrelated();
    }
    if ca == cb {
        return match (&a.legal, &b.legal) {
            (Some(x), Some(y)) if x != y => Verdict::new(Level::Likely, Rule::DifferentLegalForm, 0.8),
            (x, y) if x == y => Verdict::new(Level::Same, Rule::SameName, 1.0),
            _ => Verdict::new(Level::Same, Rule::LegalForm, 0.95),
        };
    }
    let ja = ca.join(" ");
    let jb = cb.join(" ");
    if ja.chars().count() >= 6 && jb.chars().count() >= 6 && ca.len() == cb.len() {
        let s = jaro_winkler(&ja, &jb);
        if s >= 0.95 && edit_distance(&ja, &jb) <= 2 {
            return Verdict::new(Level::Likely, Rule::SimilarSpelling, s);
        }
    }
    let (s, l) = if ca.len() <= cb.len() { (&ca, &cb) } else { (&cb, &ca) };
    // Acronym: "ACME" vs "Atlas Cargo Maritime Enterprises".
    if s.len() == 1 && l.len() >= 2 {
        let initials: String = l.iter().filter_map(|t| t.chars().next()).collect();
        if s[0].chars().count() >= 2 && s[0] == initials {
            return Verdict::new(Level::Likely, Rule::Acronym, 0.5);
        }
    }
    // A shorter name that starts the longer one ("Meridian" / "Meridian Consulting").
    if l.starts_with(s) && s.iter().any(|t| t.chars().count() >= 4) {
        return Verdict::new(Level::Likely, Rule::PartialName, 0.6);
    }
    // Same first word, different rest: two companies of one family.
    if s.len() >= 2 && l.len() >= 2 && s[0] == l[0] {
        return Verdict::conflict();
    }
    Verdict::unrelated()
}

fn compare_addresses(a: &Form, b: &Form) -> Verdict {
    if a.key == b.key {
        return Verdict::new(Level::Same, Rule::SameName, 1.0);
    }
    fn digits(f: &Form) -> Vec<&str> {
        let mut d: Vec<&str> = f.toks.iter().filter(|t| t.s.chars().any(|c| c.is_ascii_digit())).map(|t| t.s.as_str()).collect();
        d.sort();
        d
    }
    let (da, db) = (digits(a), digits(b));
    let sa: std::collections::BTreeSet<&str> = a.toks.iter().map(|t| t.s.as_str()).collect();
    let sb: std::collections::BTreeSet<&str> = b.toks.iter().map(|t| t.s.as_str()).collect();
    let inter = sa.intersection(&sb).count();
    let union = sa.union(&sb).count().max(1);
    let jac = inter as f32 / union as f32;
    let alpha_shared = sa.intersection(&sb).filter(|t| !t.chars().any(|c| c.is_ascii_digit())).count();
    let (small, large) = if sa.len() <= sb.len() { (&sa, &sb) } else { (&sb, &sa) };
    let subset = small.is_subset(large) && small.len() >= 3;
    if !da.is_empty() && !db.is_empty() {
        // The street number (first number) must agree.
        let num = |f: &Form| f.toks.iter().find(|t| t.s.chars().any(|c| c.is_ascii_digit())).map(|t| t.s.clone());
        if num(a) != num(b) {
            return if alpha_shared >= 2 { Verdict::conflict() } else { Verdict::unrelated() };
        }
    }
    if subset || (jac >= 0.75 && da == db) {
        return Verdict::new(Level::Likely, Rule::SimilarAddress, jac.max(0.6));
    }
    Verdict::unrelated()
}

fn compare_phones(a: &Form, b: &Form) -> Verdict {
    let (x, y) = (&a.key, &b.key);
    if x == y {
        return Verdict::new(Level::Same, Rule::SameNumber, 1.0);
    }
    let (s, l) = if x.len() <= y.len() { (x, y) } else { (y, x) };
    // A country code / trunk prefix in front of the same national number.
    if s.len() >= 8 && l.ends_with(s.as_str()) && l.len() - s.len() <= 3 {
        return Verdict::new(Level::Same, Rule::SameNumber, 0.95);
    }
    Verdict::unrelated()
}

// ── string similarity ────────────────────────────────────────────────────

pub fn jaro_winkler(a: &str, b: &str) -> f32 {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    if a.is_empty() && b.is_empty() {
        return 1.0;
    }
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    let range = (a.len().max(b.len()) / 2).saturating_sub(1);
    let mut am = vec![false; a.len()];
    let mut bm = vec![false; b.len()];
    let mut m = 0usize;
    for i in 0..a.len() {
        let lo = i.saturating_sub(range);
        let hi = (i + range + 1).min(b.len());
        for j in lo..hi {
            if !bm[j] && a[i] == b[j] {
                am[i] = true;
                bm[j] = true;
                m += 1;
                break;
            }
        }
    }
    if m == 0 {
        return 0.0;
    }
    let mut t = 0usize;
    let mut k = 0usize;
    for i in 0..a.len() {
        if am[i] {
            while !bm[k] {
                k += 1;
            }
            if a[i] != b[k] {
                t += 1;
            }
            k += 1;
        }
    }
    let m = m as f32;
    let jaro = (m / a.len() as f32 + m / b.len() as f32 + (m - t as f32 / 2.0) / m) / 3.0;
    let prefix = a.iter().zip(b.iter()).take(4).take_while(|(x, y)| x == y).count() as f32;
    jaro + prefix * 0.1 * (1.0 - jaro)
}

pub fn edit_distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for i in 1..=a.len() {
        let mut cur = vec![i; b.len() + 1];
        for j in 1..=b.len() {
            let c = if a[i - 1] == b[j - 1] { 0 } else { 1 };
            cur[j] = (prev[j] + 1).min(cur[j - 1] + 1).min(prev[j - 1] + c);
        }
        prev = cur;
    }
    prev[b.len()]
}

/// Similarity of two whole strings for gating embedding proposals.
pub fn string_similarity(a: &Form, b: &Form) -> f32 {
    jaro_winkler(&a.key, &b.key)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(cat: &str, a: &str, b: &str) -> Verdict {
        compare(&analyse(cat, a), &analyse(cat, b))
    }

    fn lvl(cat: &str, a: &str, b: &str) -> (Level, Rule) {
        let r = v(cat, a, b);
        (r.level, r.rule)
    }

    #[test]
    fn person_forms() {
        let f = analyse("PersonName", "Sra. María Inmaculada Ferrándiz Olmedo");
        assert_eq!(f.key, "maria inmaculada ferrandiz olmedo");
        assert_eq!(f.gender, Some('f'));
        assert!(f.honorific);
        let f = analyse("PersonName", "SANTOS, Maria");
        assert_eq!(f.key, "maria santos");
        assert!(f.reordered);
        let f = analyse("PersonName", "Maria da Silva Santos");
        assert_eq!(f.key, "maria silva santos");
        assert_eq!(f.surface, vec!["maria", "da", "silva", "santos"]);
        let f = analyse("PersonName", "J.M. Holloway");
        assert_eq!(f.key, "j m holloway");
        assert!(f.toks[0].initial && f.toks[1].initial);
        let f = analyse("PersonName", "John Smith, Jr.");
        assert_eq!(f.generation, Some("jr"));
        assert!(!f.reordered);
        // "D. Santos" is an initial, "Dª Carmen Ruiz" an honorific, "da Silva" a connector.
        assert_eq!(analyse("PersonName", "D. Santos").key, "d santos");
        assert_eq!(analyse("PersonName", "Dª Carmen Ruiz").key, "carmen ruiz");
        assert_eq!(analyse("PersonName", "Da Silva").key, "silva");
    }

    #[test]
    fn person_same() {
        assert_eq!(lvl("PersonName", "Maria Santos", "Santos, Maria"), (Level::Same, Rule::Reordered));
        assert_eq!(lvl("PersonName", "Maria Santos", "MARIA SANTOS"), (Level::Same, Rule::SameName));
        assert_eq!(lvl("PersonName", "María Santos", "Ms. Maria Santos"), (Level::Same, Rule::SameName));
        assert_eq!(lvl("PersonName", "Ana Ferrándiz", "Ana Ferrándiz Olmedo"), (Level::Same, Rule::NameExtension));
        assert_eq!(lvl("PersonName", "Maria Santos", "Maria Fernanda Santos"), (Level::Same, Rule::NameExtension));
        assert_eq!(lvl("PersonName", "FERRÁNDIZ OLMEDO, María Inmaculada", "María Inmaculada Ferrándiz Olmedo").0, Level::Same);
    }

    #[test]
    fn person_proposed_not_applied() {
        assert_eq!(lvl("PersonName", "M. Santos", "Maria Santos"), (Level::Likely, Rule::Initial));
        assert_eq!(lvl("PersonName", "Ms. Santos", "Maria Santos"), (Level::Likely, Rule::SurnameOnly));
        assert_eq!(lvl("PersonName", "Santos", "Maria Santos"), (Level::Likely, Rule::SurnameOnly));
        assert_eq!(lvl("PersonName", "Santos, M.", "Maria Santos"), (Level::Likely, Rule::Initial));
        assert_eq!(lvl("PersonName", "Kowalski, Jan", "Jan Kowalsky"), (Level::Likely, Rule::SimilarSpelling));
        assert_eq!(lvl("PersonName", "John Smith", "John Smith Jr."), (Level::Likely, Rule::NameExtension));
    }

    #[test]
    fn person_hard_negatives() {
        assert_eq!(lvl("PersonName", "Maria Santos", "João Santos").0, Level::Conflict);
        assert_eq!(lvl("PersonName", "J. Santos", "M. Santos").0, Level::Conflict);
        assert_eq!(lvl("PersonName", "Maria Fernanda Santos", "Maria Clara Santos").0, Level::Conflict);
        assert_eq!(lvl("PersonName", "Mr. Santos", "Ms. Santos").0, Level::Conflict);
        assert_eq!(lvl("PersonName", "John Smith Jr.", "John Smith Sr.").0, Level::Conflict);
        assert_eq!(lvl("PersonName", "Maria Santos", "Maria Silva").0, Level::Conflict);
        // A given name alone says nothing.
        assert_eq!(lvl("PersonName", "Maria", "Maria Santos").0, Level::Unrelated);
        assert_eq!(lvl("PersonName", "Dana Holloway", "Peter Novak").0, Level::Unrelated);
    }

    #[test]
    fn companies() {
        assert_eq!(lvl("CompanyName", "Meridian Consulting LLC", "Meridian Consulting, L.L.C."), (Level::Same, Rule::SameName));
        assert_eq!(lvl("CompanyName", "Meridian Consulting LLC", "Meridian Consulting"), (Level::Same, Rule::LegalForm));
        assert_eq!(lvl("CompanyName", "Construtora Horizonte Ltda.", "CONSTRUTORA HORIZONTE LTDA"), (Level::Same, Rule::SameName));
        assert_eq!(lvl("CompanyName", "Inversiones Albar, S.A.", "Inversiones Albar SA"), (Level::Same, Rule::SameName));
        assert_eq!(lvl("CompanyName", "Acme Holdings Inc.", "Acme Holdings Incorporated"), (Level::Same, Rule::SameName));
        assert_eq!(lvl("CompanyName", "Tecnologia Brava S/A", "Tecnologia Brava S.A."), (Level::Same, Rule::SameName));
        assert_eq!(lvl("CompanyName", "Meridian Consulting LLC", "Meridian Consulting GmbH"), (Level::Likely, Rule::DifferentLegalForm));
        assert!(v("CompanyName", "Meridian Consulting LLC", "Meridian Consulting GmbH").blocks_auto());
        assert_eq!(lvl("CompanyName", "Meridian", "Meridian Consulting LLC"), (Level::Likely, Rule::PartialName));
        assert_eq!(lvl("CompanyName", "Atlas Digital Inc.", "Atlas Logistics Inc.").0, Level::Conflict);
        assert_eq!(lvl("CompanyName", "The Smith & Jones Company", "Smith and Jones Co.").0, Level::Same);
        let f = analyse("CompanyName", "Grupo Norte, S.A. de C.V.");
        assert_eq!(f.legal.as_deref(), Some("sa+cv"));
        assert_eq!(f.key, "grupo norte|sa+cv");
    }

    #[test]
    fn numbers_and_addresses() {
        assert_eq!(lvl("SocialSecurityNumber", "123-45-6789", "123 45 6789"), (Level::Same, Rule::SameNumber));
        assert_eq!(lvl("IBAN", "DE89 3704 0044 0532 0130 00", "DE89370400440532013000").0, Level::Same);
        assert_eq!(lvl("PhoneNumber", "+1 (415) 555-0187", "415.555.0187"), (Level::Same, Rule::SameNumber));
        assert_eq!(lvl("PhoneNumber", "415-555-0187", "415-555-0188").0, Level::Unrelated);
        assert_eq!(lvl("Email", "M.Santos@Example.com", "m.santos@example.com").0, Level::Same);
        assert_eq!(lvl("Address", "2287 Juniper Hollow Road", "2287 Juniper Hollow Rd.").0, Level::Same);
        assert_eq!(lvl("Address", "2287 Juniper Hollow Road", "2287 Juniper Hollow Road, Oakland, CA 94611"), (Level::Likely, Rule::SimilarAddress));
        assert_eq!(lvl("Address", "12 Juniper Hollow Road", "14 Juniper Hollow Road").0, Level::Conflict);
        assert_eq!(lvl("Date", "12/03/2024", "12/03/2024").0, Level::Same);
    }

    #[test]
    fn similarity_helpers() {
        assert!(jaro_winkler("kowalski", "kowalsky") > 0.92);
        assert!(jaro_winkler("santos", "silva") < 0.8);
        assert_eq!(edit_distance("kitten", "sitting"), 3);
    }
}
