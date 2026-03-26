use regex::Regex;
use crate::pipeline::types::PiiCategory;
use super::global::PatternDef;

pub fn de_patterns() -> Vec<PatternDef> {
    vec![
        PatternDef {
            regex: Regex::new(r"\b\d{2}\s?\d{3}\s?\d{5}\b").unwrap(),
            category: PiiCategory::TaxId,
            priority: 10,
        },
    ]
}
