use regex::Regex;
use crate::pipeline::types::PiiCategory;
use super::global::PatternDef;

pub fn br_patterns() -> Vec<PatternDef> {
    vec![
        // CPF: 000.000.000-00
        PatternDef {
            regex: Regex::new(r"\b\d{3}\.\d{3}\.\d{3}-\d{2}\b").unwrap(),
            category: PiiCategory::NationalId,
            priority: 10,
        },
        // CNPJ: 00.000.000/0000-00
        PatternDef {
            regex: Regex::new(r"\b\d{2}\.\d{3}\.\d{3}/\d{4}-\d{2}\b").unwrap(),
            category: PiiCategory::TaxId,
            priority: 10,
        },
        // CEP (Brazilian postal code): 00000-000
        PatternDef {
            regex: Regex::new(r"\b\d{5}-\d{3}\b").unwrap(),
            category: PiiCategory::Address,
            priority: 7,
        },
    ]
}
