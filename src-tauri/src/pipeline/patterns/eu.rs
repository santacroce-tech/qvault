use regex::Regex;
use crate::pipeline::types::PiiCategory;
use super::global::PatternDef;

pub fn eu_patterns() -> Vec<PatternDef> {
    vec![
        PatternDef {
            regex: Regex::new(r"\b[A-Z]{2}\d{8,12}\b").unwrap(),
            category: PiiCategory::TaxId,
            priority: 10,
        },
    ]
}
