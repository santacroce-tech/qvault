use regex::Regex;
use crate::pipeline::types::PiiCategory;
use super::global::PatternDef;

pub fn us_patterns() -> Vec<PatternDef> {
    vec![
        PatternDef {
            regex: Regex::new(r"\b\d{3}-\d{2}-\d{4}\b").unwrap(),
            category: PiiCategory::SocialSecurityNumber,
            priority: 10,
        },
        PatternDef {
            regex: Regex::new(r"\b\d{2}-\d{7}\b").unwrap(),
            category: PiiCategory::TaxId,
            priority: 10,
        },
    ]
}
