//! Typing Privacy Filter's `account_number` spans.
//!
//! The model has one label, `account_number`, for every identifier: bank
//! accounts, but also national IDs, tax numbers, passports, plates, client
//! numbers… (docs/qvac-integration.md §14). Spans that Layers 1–2 already
//! found never reach the reviewer from Layer 3 (they are dropped as covered),
//! so what is left are identifiers Layer 1's patterns did not match. They are
//! typed here, in this order:
//!
//! 1. by structure and checksum, with Layer 1's validators (IBAN mod-97,
//!    Luhn cards, ES DNI/NIE, BR CPF/CNPJ, PT NIF, SWIFT/BIC, ABA routing) and
//!    unambiguous shapes (US SSN `ddd-dd-dddd`, US EIN `dd-ddddddd`, IPv4,
//!    EU VAT `CC` + digits);
//! 2. by the words just before the span ("passport", "matrícula", "NIF",
//!    "cliente", "processo", …);
//! 3. otherwise it stays `BankAccount` (the model's own label) and is marked
//!    untyped, so it gets a low confidence.

use regex::Regex;

use crate::pipeline::patterns::checks;
use crate::pipeline::types::PiiCategory;

/// How a span got its type (drives the confidence).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum TypedBy {
    /// A checksum or a shape no other identifier has.
    Validator,
    /// A label word right before the span.
    Context,
    /// Nothing: the model's `account_number` → BankAccount.
    Untyped,
}

fn re(p: &str) -> Regex {
    Regex::new(p).unwrap()
}

struct Shapes {
    ssn: Regex,
    ein: Regex,
    ipv4: Regex,
    vat: Regex,
    bic: Regex,
}

fn shapes() -> &'static Shapes {
    static S: std::sync::OnceLock<Shapes> = std::sync::OnceLock::new();
    S.get_or_init(|| Shapes {
        ssn: re(r"^\d{3}-\d{2}-\d{4}$"),
        ein: re(r"^\d{2}-\d{7}$"),
        ipv4: re(r"^(\d{1,3})\.(\d{1,3})\.(\d{1,3})\.(\d{1,3})$"),
        vat: re(r"^([A-Z]{2})[ -]?([0-9A-Z][0-9A-Z .-]{6,14}[0-9A-Z])$"),
        bic: re(r"^[A-Z]{4}[A-Z]{2}[A-Z0-9]{2}(?:[A-Z0-9]{3})?$"),
    })
}

/// Context words (lower-case, matched in the ~48 bytes before the span, on
/// word boundaries) → category. First match in this list wins.
const CONTEXT: &[(&str, PiiCategory)] = &[
    (r"passport|pasaporte|passaporte|reisepass", PiiCategory::PassportNumber),
    (r"driver'?s? licen[cs]e|driving licen[cs]e|licencia de conducir|permiso de conducir|carta de condu[cç][aã]o|carteira de habilita[cç][aã]o|cnh|f[uü]hrerschein", PiiCategory::DriversLicense),
    (r"plate|license plate|matr[ií]cula|placa|kennzeichen", PiiCategory::LicensePlate),
    (r"mrn|medical record|historia cl[ií]nica|prontu[aá]rio|patient id|n[º°] de paciente", PiiCategory::MedicalRecordNumber),
    (r"ssn|social security", PiiCategory::SocialSecurityNumber),
    (r"ein|tin|vat|nif|cif|nipc|cnpj|contribuinte|steuer-?id|steuernummer|ust-?idnr|tax id|tax number|identificaci[oó]n fiscal", PiiCategory::TaxId),
    (r"cpf|rg|dni|nie|nino|national insurance|id card|identity card|documento de identidad|carteira de identidade|cart[aã]o de cidad[aã]o|bilhete de identidade|personalausweis|national id", PiiCategory::NationalId),
    (r"case|docket|processo|proceso|autos|procedimiento|aktenzeichen", PiiCategory::CaseNumber),
    (r"client|matter|claim|policy|cliente|p[oó]liza|ap[oó]lice|sinistro|siniestro|employee|empleado|funcion[aá]rio|reference|ref\.?", PiiCategory::ClientId),
    (r"account|acct|cuenta|conta|ag[eê]ncia|routing|aba|swift|bic|iban|konto", PiiCategory::BankAccount),
];

fn context_res() -> &'static Vec<(Regex, PiiCategory)> {
    static R: std::sync::OnceLock<Vec<(Regex, PiiCategory)>> = std::sync::OnceLock::new();
    R.get_or_init(|| CONTEXT.iter().map(|(p, c)| (re(&format!(r"(?i)\b(?:{})\b", p)), c.clone())).collect())
}

/// The ~48 bytes before `start` (on a char boundary), up to the previous
/// line break or sentence end, which is where a label for the value sits.
pub fn context_before(text: &str, start: usize) -> &str {
    let mut from = start.saturating_sub(48);
    while !text.is_char_boundary(from) {
        from += 1;
    }
    let ctx = &text[from..start];
    match ctx.rfind(['\n', ';']) {
        Some(i) => &ctx[i + 1..],
        None => ctx,
    }
}

/// Type an `account_number` span. `span` is the detected text, `before` the
/// text right before it (see [`context_before`]).
pub fn retype_account(span: &str, before: &str) -> (PiiCategory, TypedBy) {
    let s = span.trim();
    let upper = s.to_uppercase();
    let compact: String = upper.chars().filter(|c| c.is_ascii_alphanumeric()).collect();
    let digits = compact.chars().filter(|c| c.is_ascii_digit()).count();
    let only_digit_groups = s.chars().all(|c| c.is_ascii_digit() || matches!(c, ' ' | '-' | '.' | '/'));
    let sh = shapes();

    if compact.len() >= 15 && compact.len() <= 34 && checks::iban(&compact).map(|r| r.0 == compact.len()).unwrap_or(false) {
        return (PiiCategory::IBAN, TypedBy::Validator);
    }
    if let Some(c) = sh.ipv4.captures(s) {
        if (1..=4).all(|i| c[i].parse::<u32>().map(|v| v <= 255).unwrap_or(false)) {
            return (PiiCategory::IpAddress, TypedBy::Validator);
        }
    }
    if only_digit_groups && (13..=19).contains(&digits) && checks::card_plain(&compact).is_some() {
        return (PiiCategory::CreditCard, TypedBy::Validator);
    }
    if sh.ssn.is_match(s) && !s.starts_with("000") && !s.starts_with("666") && !s.starts_with('9') {
        return (PiiCategory::SocialSecurityNumber, TypedBy::Validator);
    }
    if sh.ein.is_match(s) {
        return (PiiCategory::TaxId, TypedBy::Validator);
    }
    if only_digit_groups && digits == 11 && checks::br_cpf(s).is_some() {
        return (PiiCategory::NationalId, TypedBy::Validator);
    }
    if only_digit_groups && digits == 14 && checks::br_cnpj(s).is_some() {
        return (PiiCategory::TaxId, TypedBy::Validator);
    }
    if compact.len() == 9 && checks::es_dni_nie(&compact).is_some() {
        return (PiiCategory::NationalId, TypedBy::Validator);
    }
    if sh.bic.is_match(&compact) && compact.len() == s.len() && checks::bic_bare(&compact).is_some() {
        return (PiiCategory::BankAccount, TypedBy::Validator);
    }
    if let Some(c) = sh.vat.captures(&upper) {
        let cc = &c[1];
        let rest_digits = c[2].chars().filter(|c| c.is_ascii_digit()).count();
        if checks::is_country(cc) && rest_digits >= 7 && !upper.contains(' ') {
            return (PiiCategory::TaxId, TypedBy::Validator);
        }
    }
    // Context labels come before the weaker numeric checks below (a 9-digit
    // number after "Passport No." is a passport, whatever its checksum).
    let before_l = before.to_lowercase();
    let mut best: Option<(usize, &PiiCategory)> = None;
    for (r, cat) in context_res() {
        // the label closest to the span wins
        if let Some(m) = r.find_iter(&before_l).last() {
            if best.map(|(end, _)| m.end() > end).unwrap_or(true) {
                best = Some((m.end(), cat));
            }
        }
    }
    if let Some((_, cat)) = best {
        return (cat.clone(), TypedBy::Context);
    }
    if only_digit_groups && digits == 9 && checks::pt_nif(s).is_some() {
        return (PiiCategory::TaxId, TypedBy::Validator);
    }
    if only_digit_groups && digits == 9 && checks::aba_routing(&compact).is_some() {
        return (PiiCategory::BankAccount, TypedBy::Validator);
    }
    (PiiCategory::BankAccount, TypedBy::Untyped)
}

#[cfg(test)]
mod tests {
    use super::*;
    use PiiCategory::*;

    #[test]
    fn validators_type_identifiers() {
        assert_eq!(retype_account("DE89 3704 0044 0532 0130 00", ""), (IBAN, TypedBy::Validator));
        assert_eq!(retype_account("4111 1111 1111 1111", ""), (CreditCard, TypedBy::Validator));
        assert_eq!(retype_account("4111 1111 1111 1112", "").1, TypedBy::Untyped, "Luhn fails");
        assert_eq!(retype_account("123-45-6789", ""), (SocialSecurityNumber, TypedBy::Validator));
        assert_eq!(retype_account("12-3456789", ""), (TaxId, TypedBy::Validator));
        assert_eq!(retype_account("222.333.444-05", ""), (NationalId, TypedBy::Validator));
        assert_eq!(retype_account("22233344405", ""), (NationalId, TypedBy::Validator));
        assert_eq!(retype_account("11.222.333/0001-81", ""), (TaxId, TypedBy::Validator));
        assert_eq!(retype_account("X1234567L", ""), (NationalId, TypedBy::Validator));
        assert_eq!(retype_account("192.168.10.25", ""), (IpAddress, TypedBy::Validator));
        assert_eq!(retype_account("DE298417365", ""), (TaxId, TypedBy::Validator));
        assert_eq!(retype_account("123456789", ""), (TaxId, TypedBy::Validator), "valid PT NIF");
        assert_eq!(retype_account("COBADEFFXXX", ""), (BankAccount, TypedBy::Validator));
    }

    #[test]
    fn context_types_the_rest() {
        assert_eq!(retype_account("AB1234567", "holder of Passport No. "), (PassportNumber, TypedBy::Context));
        assert_eq!(retype_account("12-AB-34", "veículo de matrícula "), (LicensePlate, TypedBy::Context));
        assert_eq!(retype_account("C-55102", "Client number: "), (ClientId, TypedBy::Context));
        assert_eq!(retype_account("2024/118", "processo n.º "), (CaseNumber, TypedBy::Context));
        assert_eq!(retype_account("0012-339-8841", "account no. "), (BankAccount, TypedBy::Context));
        // the closest label wins
        assert_eq!(retype_account("A-77", "client account, policy "), (ClientId, TypedBy::Context));
        // a label inside another word does not count ("cpf" in "xcpfz")
        assert_eq!(retype_account("12345", "xcpfz ").1, TypedBy::Untyped);
    }

    #[test]
    fn untyped_stays_bank_account() {
        assert_eq!(retype_account("0012-339-8841", "pay to "), (BankAccount, TypedBy::Untyped));
    }

    #[test]
    fn context_stops_at_line_breaks() {
        let text = "Passport\nRef 99812";
        let start = text.find("99812").unwrap();
        assert_eq!(context_before(text, start), "Ref ");
        let text = "é".repeat(40) + "x";
        let ctx = context_before(&text, text.len() - 1);
        assert!(ctx.len() <= 48 && ctx.chars().all(|c| c == 'é'));
    }
}
