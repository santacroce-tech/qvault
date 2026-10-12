use crate::pipeline::types::PiiCategory;
use super::global::PatternDef;

pub fn de_patterns() -> Vec<PatternDef> {
    vec![
        // Steuernummer (10 digits, optionally spaced)
        PatternDef::new(r"\b\d{2}\s?\d{3}\s?\d{5}\b", PiiCategory::TaxId, 10),
        // Steuer-ID (11 digits written 2-3-3-3) and Steuernummer with slashes (27/614/04418)
        PatternDef::new(r"\b\d{2} \d{3} \d{3} \d{3}\b", PiiCategory::TaxId, 10),
        PatternDef::new(r"\b\d{2,3}/\d{3,4}/\d{4,5}\b", PiiCategory::TaxId, 9),
        // Personalausweis number after a label: 9 characters from [CFGHJKLMNPRTVWXYZ0-9]
        PatternDef::new(
            r"(?i)\b(?:identity\s+card|ID\s+card|Personalausweis|Ausweis)(?:\s+(?:no\.?|nr\.?|number))?[\s:#.]*([CFGHJKLMNPRTVWXYZ0-9]{9})\b",
            PiiCategory::NationalId,
            10,
        )
        .group(1),
    ]
}
