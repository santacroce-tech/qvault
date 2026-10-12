use crate::pipeline::types::PiiCategory;
use super::global::PatternDef;

pub fn us_patterns() -> Vec<PatternDef> {
    vec![
        PatternDef::new(r"\b\d{3}-\d{2}-\d{4}\b", PiiCategory::SocialSecurityNumber, 10),
        PatternDef::new(r"\b\d{2}-\d{7}\b", PiiCategory::TaxId, 10),
    ]
}
