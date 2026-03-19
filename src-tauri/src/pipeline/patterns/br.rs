use regex::Regex;
use crate::pipeline::types::PiiCategory;
use super::global::PatternDef;

pub fn br_patterns() -> Vec<PatternDef> {
    vec![
        PatternDef {
            regex: Regex::new(r"\b\d{3}\.\d{3}\.\d{3}-\d{2}\b").unwrap(),
            category: PiiCategory::NationalId,
            priority: 10,
        },
        PatternDef {
            regex: Regex::new(r"\b\d{2}\.\d{3}\.\d{3}/\d{4}-\d{2}\b").unwrap(),
            category: PiiCategory::TaxId,
            priority: 10,
        },
    ]
}
