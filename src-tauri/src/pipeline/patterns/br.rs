use crate::pipeline::types::PiiCategory;
use super::global::PatternDef;

pub fn br_patterns() -> Vec<PatternDef> {
    vec![
        // CPF: 000.000.000-00
        PatternDef::new(r"\b\d{3}\.\d{3}\.\d{3}-\d{2}\b", PiiCategory::NationalId, 10),
        // CNPJ: 00.000.000/0000-00
        PatternDef::new(r"\b\d{2}\.\d{3}\.\d{3}/\d{4}-\d{2}\b", PiiCategory::TaxId, 10),
        // RG (São Paulo style): 00.000.000-0
        PatternDef::new(r"\b\d{2}\.\d{3}\.\d{3}-[\dXx]\b", PiiCategory::NationalId, 10),
        // CEP (Brazilian postal code): 00000-000
        PatternDef::new(r"\b\d{5}-\d{3}\b", PiiCategory::Address, 7),
        // Mercosul / old plates after context is handled globally; bare old-style AAA-0000
        PatternDef::new(r"\b[A-Z]{3}-\d[A-Z0-9]\d{2}\b", PiiCategory::LicensePlate, 6),
    ]
}
