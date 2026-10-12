use regex::Regex;
use crate::pipeline::types::PiiCategory;
use super::checks::{self, Check};

pub struct PatternDef {
    pub regex: Regex,
    pub category: PiiCategory,
    /// Higher priority patterns are matched first; matches excluded from lower priority
    pub priority: u8,
    /// Capture group that holds the value (0 = the whole match). Used by
    /// "label: value" patterns, where the label is context only.
    pub group: usize,
    /// Optional checksum / structure validator (see `checks`).
    pub check: Option<Check>,
    /// Confidence when no validator sets one.
    pub confidence: f64,
}

impl PatternDef {
    pub fn new(re: &str, category: PiiCategory, priority: u8) -> Self {
        PatternDef { regex: Regex::new(re).unwrap(), category, priority, group: 0, check: None, confidence: 0.95 }
    }
    pub fn group(mut self, g: usize) -> Self {
        self.group = g;
        self
    }
    pub fn check(mut self, c: Check) -> Self {
        self.check = Some(c);
        self
    }
    pub fn confidence(mut self, c: f64) -> Self {
        self.confidence = c;
        self
    }
}

// Shared fragments.
const MONTHS_EN: &str = "January|February|March|April|May|June|July|August|September|October|November|December";
const MONTHS_ES_PT: &str = "enero|febrero|marzo|abril|mayo|junio|julio|agosto|septiembre|setiembre|octubre|noviembre|diciembre|janeiro|fevereiro|março|maio|junho|julho|setembro|outubro|novembro|dezembro";

pub fn global_patterns() -> Vec<PatternDef> {
    use PiiCategory::*;
    vec![
        // High priority — specific formats
        PatternDef::new(r"[a-zA-Z0-9._%+\-]+@[a-zA-Z0-9.\-]+\.[a-zA-Z]{2,}", Email, 10),
        // Cards. Undelimited: Luhn required. Grouped 4-4-4-4 / Amex 4-6-5:
        // grouping is evidence enough, Luhn sets the confidence.
        PatternDef::new(r"\b(?:4[0-9]{12}(?:[0-9]{3})?|5[1-5][0-9]{14}|3[47][0-9]{13}|6(?:011|5[0-9]{2})[0-9]{12})\b", CreditCard, 10)
            .check(checks::card_plain),
        PatternDef::new(r"\b\d{4}([ -])\d{4}([ -])\d{4}([ -])\d{4}\b", CreditCard, 10).check(checks::card_grouped),
        PatternDef::new(r"\b3[47]\d{2}[ -]\d{6}[ -]\d{5}\b", CreditCard, 10).check(checks::card_grouped),
        // IBAN, compact or in groups of four, validated with mod-97.
        PatternDef::new(r"\b[A-Z]{2}\d{2}(?: ?[A-Z0-9]{4}){2,7}(?: ?[A-Z0-9]{1,4})?\b", IBAN, 11).check(checks::iban),
        PatternDef::new(
            r"\b(?:(?:25[0-5]|2[0-4]\d|[01]?\d\d?)\.){3}(?:25[0-5]|2[0-4]\d|[01]?\d\d?)\b",
            IpAddress,
            10,
        ),
        PatternDef::new(r"https?://[a-zA-Z0-9\-._~:/?#\[\]@!$&'()*+,;=%]+", Url, 10),
        // SWIFT/BIC after a label, or bare when it carries a digit / branch.
        PatternDef::new(r"\b(?:SWIFT|BIC|Swift|swift)(?:/BIC)?(?:\s+(?:[Cc]ode|[Nn]o\.?))?[\s:]*([A-Z]{6}[A-Z0-9]{2}(?:[A-Z0-9]{3})?)\b", BankAccount, 10)
            .group(1)
            .check(checks::bic_labelled),
        PatternDef::new(r"\b[A-Z]{6}[A-Z0-9]{2}(?:[A-Z0-9]{3})?\b", BankAccount, 6).check(checks::bic_bare),
        // Labelled identifiers ("Passport no. X", "Account Number: 123").
        PatternDef::new(
            r"(?i)\b(?:passport|pasaporte|passaporte)(?:\s+(?:no\.?|n\.?º|number|n[uú]mero))?[\s:#.]*([A-Z0-9]{6,9})\b",
            PassportNumber,
            9,
        )
        .group(1),
        PatternDef::new(
            r"(?i)\b(?:driver'?s?\s+licen[sc]e|carta\s+de\s+condu[çc][ãa]o|permiso\s+de\s+conducir|carteira\s+de\s+habilita[çc][ãa]o|CNH)(?:[\s:#,.]*(?:no\.?|n\.?º|number|is))*[\s:#,.]*([A-Z]?-?\d[\d -]{4,12}\d|[A-Z]\d{6,9})\b",
            DriversLicense,
            9,
        )
        .group(1),
        PatternDef::new(
            r"(?i)\b(?:account\s+(?:number|no\.?|#)|acct\.?\s*(?:no\.?|#)|conta(?:\s+corrente)?|ag[eê]ncia|cuenta|CLABE)[\s:#nº.]*(\d[\d .-]{2,22}\d)\b",
            BankAccount,
            9,
        )
        .group(1),
        PatternDef::new(r"(?i)\b(?:routing|ABA|RTN)(?:\s+(?:number|no\.?|#))?[\s:#]*(\d{9})\b", BankAccount, 9)
            .group(1)
            .check(checks::aba_routing_labelled),
        PatternDef::new(r"\b\d{9}\b", BankAccount, 4).check(checks::aba_routing),
        PatternDef::new(
            r"(?i)\b(?:MRN|medical\s+record\s+(?:number|no\.?)|patient\s+ID|historia\s+cl[ií]nica|prontu[aá]rio(?:\s+n[º.o]*)?)[\s:#nº.]*([A-Z]{0,4}-?\d[A-Z0-9-]{3,})",
            MedicalRecordNumber,
            9,
        )
        .group(1),
        PatternDef::new(
            r"(?i)\bcourt\s+file\s+(?:no\.?|number)[\s:#]*([A-Z0-9][A-Z0-9./-]*\d[A-Z0-9./-]*)",
            CourtFileNumber,
            9,
        )
        .group(1),
        PatternDef::new(
            r"(?i)\b(?:case|charge|docket|processo|proceso|procedimiento(?::?\s+(?:juicio\s+verbal|ordinario|abreviado))?|juicio\s+verbal|expediente\s+judicial)(?:\s+(?:no\.?|n\.?º|number|nº))?[\s:#.]*([A-Z0-9][A-Z0-9:./-]*\d[A-Z0-9:./-]*)",
            CaseNumber,
            8,
        )
        .group(1),
        // Brazilian unified case number (CNJ): NNNNNNN-DD.AAAA.J.TR.OOOO
        PatternDef::new(r"\b\d{7}-\d{2}\.\d{4}\.\d\.\d{2}\.\d{4}\b", CaseNumber, 10),
        // Spanish general identification number of a proceeding (NIG), spaced digits
        PatternDef::new(r"\bNIG[\s:]*(\d[\d ]{8,24}\d)\b", CaseNumber, 9).group(1),
        PatternDef::new(
            r"(?i)\b(?:(?:client|claim|policy|matter|employee|referencia)\s+(?:id|no\.?|number|n\.?º|nº)(?:\s+(?:was|is))?|expediente)[\s:#.]*([A-Z0-9][A-Z0-9-]*\d[A-Z0-9-]*)",
            ClientId,
            8,
        )
        .group(1),
        PatternDef::new(
            r"(?i)\b(?:plate|placa|matr[ií]cula|registration\s+number|license\s+plate)(?:\s+n[º.o]*)?[\s:#]*([A-Z0-9]{1,4}[ -]?[A-Z0-9]{1,4}[ -]?[A-Z0-9]{1,4})\b",
            LicensePlate,
            8,
        )
        .group(1),
        // Monetary amounts: $50,000,000 / USD $1,234.56 / EUR 500 / R$ 1.000,00 / € 1.250,00 / £3,600
        PatternDef::new(r"(?:USD|EUR|GBP|BRL|MXN)[ \t]*\$?[ \t]*\d[\d,\.]*\d|R\$[ \t]*\d[\d,\.]*\d", MonetaryAmount, 8),
        PatternDef::new(r"\$\s*[\d,]+(?:\.\d{2})?", MonetaryAmount, 8),
        PatternDef::new(r"[€£]\s?\d[\d.,]*\d|[€£]\s?\d", MonetaryAmount, 8),
        PatternDef::new(r"\b\d[\d.,]*\s?(?:€|euros?\b|EUR\b)", MonetaryAmount, 8),
        // Dates: "January 1, 2024" / "March 9th, 2026" / "June 4, 2024"
        PatternDef::new(&format!(r"\b(?:{})\s+\d{{1,2}}(?:st|nd|rd|th)?,?\s*\d{{4}}\b", MONTHS_EN), Date, 8),
        // "12 June 2025"
        PatternDef::new(&format!(r"\b\d{{1,2}}(?:st|nd|rd|th)?\s+(?:{})\s+\d{{4}}\b", MONTHS_EN), Date, 8),
        // "15 de marzo de 2024", "1º de fevereiro de 2024", "mayo de 2024"
        PatternDef::new(
            &format!(r"(?i)\b(?:\d{{1,2}}[ºo]?\s+de\s+)?(?:{})\s+de\s+\d{{4}}\b", MONTHS_ES_PT),
            Date,
            8,
        ),
        // Dates: "01/15/2024" or "15-01-2024"
        PatternDef::new(r"\b\d{1,2}[/\-]\d{1,2}[/\-]\d{2,4}\b", Date, 7),
        // Street addresses (same line). EN: number + street name + suffix,
        // then optional unit, city, state + ZIP or UK postcode.
        PatternDef::new(
            r"\b\d{1,5}[ \t]+(?:[A-Z][a-z]+\.?[ \t]+){1,4}(?:Street|St\.|Avenue|Ave\.?|Road|Rd\.|Boulevard|Blvd\.?|Lane|Ln\.|Drive|Dr\.|Way|Court|Ct\.|Place|Pl\.|Close|Plaza|Parkway|Pkwy\.?|Terrace|Square)(?:,?[ \t]+(?:Suite|Ste\.?|Floor|Fl\.|Apt\.?|Apartment|Unit|Room|\#)[ \t]*[\w-]+|,[ \t]+\d+(?:st|nd|rd|th)[ \t]+Floor)?(?:,[ \t]*[A-Z][A-Za-z]+(?:[ \t][A-Z][A-Za-z]+){0,2}){0,2}(?:,?[ \t]+[A-Z]{2}[ \t]+\d{5}(?:-\d{4})?|[ \t]+[A-Z]{1,2}\d[A-Z\d]?[ \t]*\d[A-Z]{2}|,?[ \t]+\d{5})?",
            Address,
            6,
        ),
        // ES/LatAm: "Calle de Alcalá, 214, 3.º B, 28028 Madrid", "Paseo de la Reforma 2620"
        PatternDef::new(
            r"\b(?:Calle|C/|Avenida|Avda\.|Paseo|Plaza|Ronda|Camino|Carretera|Travesía)[ \t]+(?:(?:de|del|la|las|los|el)[ \t]+)*[A-ZÁÉÍÓÚÑ][\wáéíóúñ.]*(?:[ \t]+(?:(?:de|del|la|las|los|el)[ \t]+)*[A-ZÁÉÍÓÚÑ][\wáéíóúñ.]*){0,3},?[ \t]+\d{1,5}(?:,[ \t]*(?:\d+\.?[ºª]|piso|int\.|izda\.|dcha\.)[^,\n]{0,12})*(?:,[ \t]*(?:Col\.[ \t]+)?[A-ZÁÉÍÓÚÑ][\wáéíóúñ]*(?:[ \t]+[A-ZÁÉÍÓÚÑ][\wáéíóúñ]*){0,2})?(?:,[ \t]*\d{5}[ \t]+[A-ZÁÉÍÓÚÑ][\wáéíóúñ]*(?:[ \t]+(?:de[ \t]+)?[A-ZÁÉÍÓÚÑ][\wáéíóúñ]*){0,2})?",
            Address,
            6,
        ),
        // PT/BR: "Rua de Santa Catarina, 312, 2.º Esq., 4000-447 Porto",
        // "Rua dos Pinheiros, 1.288, apto. 54, Pinheiros, São Paulo/SP, CEP 05422-002".
        // Segments after the number are taken only up to a postcode.
        PatternDef::new(
            r"\b(?:Rua|Avenida|Av\.|Alameda|Travessa|Praça|Largo|Rodovia|Estrada)[ \t]+(?:(?:de|da|do|dos|das)[ \t]+)*[A-ZÀ-Ú][\wà-ú.]*(?:[ \t]+(?:(?:de|da|do|dos|das|e)[ \t]+)*[A-ZÀ-Ú][\wà-ú.]*){0,4},[ \t]*(?:km[ \t]+)?\d[\d.]*(?:(?:,[ \t]*[^,\n]{1,32}?){0,4},[ \t]*(?:CEP[ \t]*)?\d{4,5}-\d{3}(?:[ \t]+[A-ZÀ-Ú][\wà-ú]*(?:[ \t]+[A-ZÀ-Ú][\wà-ú]*)?)?)?",
            Address,
            7,
        ),
        // DE: "Lindenallee 42, 10719 Berlin"
        PatternDef::new(
            r"\b[A-ZÄÖÜ][a-zäöüß]+(?:straße|strasse|str\.|weg|allee|platz|gasse|ring|damm|ufer)[ \t]+\d{1,4}[a-z]?(?:,[ \t]*\d{5}[ \t]+[A-ZÄÖÜ][a-zäöüß]+)?",
            Address,
            6,
        ),
        // Lower priority — phone numbers (broad pattern, matched after specific ones)
        PatternDef::new(r"\+\d[\d\s\-\.]{6,18}\d", PhoneNumber, 5),
        // US: (415) 555-0187, 415-555-0123, 415.555.0142, 1-800-555-0100
        PatternDef::new(r"(?:\b1[\-\. ])?\(\d{3}\)[ ]?\d{3}[\-\. ]\d{4}\b", PhoneNumber, 5),
        PatternDef::new(r"\b(?:1[\-\.])?\d{3}[\-\.]\d{3}[\-\.]\d{4}\b", PhoneNumber, 5),
        // BR: (11) 98765-4321 / (51) 3555-0198
        PatternDef::new(r"\(\d{2}\)[ ]?\d{4,5}-\d{4}\b", PhoneNumber, 5),
        // ES/PT national 9-digit numbers in 3-3-3 groups: 915 550 142, 213 555 019
        PatternDef::new(r"\b[2-9]\d{2} \d{3} \d{3}\b", PhoneNumber, 4),
    ]
}
