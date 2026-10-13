use crate::pipeline::types::PiiCategory;
use super::checks;
use super::global::PatternDef;

/// Europe (EU member states plus UK national insurance numbers) and the
/// Spanish/Portuguese-speaking formats that are not Brazilian.
pub fn eu_patterns() -> Vec<PatternDef> {
    vec![
        // VAT numbers: country prefix + 8-12 digits (NL adds B01 etc.)
        PatternDef::new(r"\b[A-Z]{2}\d{8,12}(?:B\d{2})?\b", PiiCategory::TaxId, 10),
        // ES DNI / NIE with mod-23 control letter
        PatternDef::new(r"\b(?:\d{8}|[XYZ]\d{7})[A-Z]\b", PiiCategory::NationalId, 10).check(checks::es_dni_nie),
        // ES company NIF/CIF: letter + 7 digits + control
        PatternDef::new(r"\b[ABCDEFGHJNPQRSUVW]-?\d{7}[0-9A-J]\b", PiiCategory::TaxId, 9),
        // PT NIF / NIPC after a label: 9 digits, possibly in 3-3-3 groups
        PatternDef::new(
            r"(?i)\b(?:NIF|NIPC|contribuinte(?:\s+fiscal)?)(?:\s+n\.?º)?[\s:#.]*(\d{3} ?\d{3} ?\d{3})\b",
            PiiCategory::TaxId,
            10,
        )
        .group(1),
        // PT Cartão de Cidadão: 8 digits, check digit, 2 letters + digit
        PatternDef::new(r"\b\d{8} ?\d ?[A-Z]{2}\d\b", PiiCategory::NationalId, 10),
        // UK National Insurance number
        PatternDef::new(r"\b[A-CEGHJ-PR-TW-Z]{2} ?\d{2} ?\d{2} ?\d{2} ?[A-D]\b", PiiCategory::NationalId, 10),
        // MX CURP
        PatternDef::new(r"\b[A-Z]{4}\d{6}[HM][A-Z]{5}[A-Z0-9]\d\b", PiiCategory::NationalId, 10),
    ]
}
