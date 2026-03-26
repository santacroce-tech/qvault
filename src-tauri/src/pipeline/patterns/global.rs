use regex::Regex;
use crate::pipeline::types::PiiCategory;

pub struct PatternDef {
    pub regex: Regex,
    pub category: PiiCategory,
    /// Higher priority patterns are matched first; matches excluded from lower priority
    pub priority: u8,
}

pub fn global_patterns() -> Vec<PatternDef> {
    vec![
        // High priority — specific formats
        PatternDef {
            regex: Regex::new(r"[a-zA-Z0-9._%+\-]+@[a-zA-Z0-9.\-]+\.[a-zA-Z]{2,}").unwrap(),
            category: PiiCategory::Email,
            priority: 10,
        },
        PatternDef {
            regex: Regex::new(
                r"\b(?:4[0-9]{12}(?:[0-9]{3})?|5[1-5][0-9]{14}|3[47][0-9]{13})\b",
            )
            .unwrap(),
            category: PiiCategory::CreditCard,
            priority: 10,
        },
        PatternDef {
            regex: Regex::new(r"\b[A-Z]{2}\d{2}[A-Z0-9]{4,30}\b").unwrap(),
            category: PiiCategory::IBAN,
            priority: 10,
        },
        PatternDef {
            regex: Regex::new(
                r"\b(?:(?:25[0-5]|2[0-4]\d|[01]?\d\d?)\.){3}(?:25[0-5]|2[0-4]\d|[01]?\d\d?)\b",
            )
            .unwrap(),
            category: PiiCategory::IpAddress,
            priority: 10,
        },
        PatternDef {
            regex: Regex::new(
                r"https?://[a-zA-Z0-9\-._~:/?#\[\]@!$&'()*+,;=%]+",
            )
            .unwrap(),
            category: PiiCategory::Url,
            priority: 10,
        },
        // Monetary amounts: $50,000,000 / USD $1,234.56 / EUR 500 / R$ 1.000,00
        PatternDef {
            regex: Regex::new(
                r"(?:USD|EUR|GBP|BRL|R)\s*\$?\s*[\d,\.]+(?:\.\d{2})?"
            ).unwrap(),
            category: PiiCategory::MonetaryAmount,
            priority: 8,
        },
        PatternDef {
            regex: Regex::new(
                r"\$\s*[\d,]+(?:\.\d{2})?"
            ).unwrap(),
            category: PiiCategory::MonetaryAmount,
            priority: 8,
        },
        // Dates: "January 1, 2024" / "March 9th, 2026" / "June 4, 2024"
        PatternDef {
            regex: Regex::new(
                r"\b(?:January|February|March|April|May|June|July|August|September|October|November|December)\s+\d{1,2}(?:st|nd|rd|th)?,?\s*\d{4}\b"
            ).unwrap(),
            category: PiiCategory::Date,
            priority: 8,
        },
        // Dates: "01/15/2024" or "15-01-2024"
        PatternDef {
            regex: Regex::new(
                r"\b\d{1,2}[/\-]\d{1,2}[/\-]\d{2,4}\b"
            ).unwrap(),
            category: PiiCategory::Date,
            priority: 7,
        },
        // Lower priority — phone numbers (broad pattern, matched after specific ones)
        PatternDef {
            regex: Regex::new(r"\+\d[\d\s\-\.]{6,18}\d").unwrap(),
            category: PiiCategory::PhoneNumber,
            priority: 5,
        },
        // Also match common US format without +
        PatternDef {
            regex: Regex::new(r"\b\d{3}[\-\.]\d{3}[\-\.]\d{4}\b").unwrap(),
            category: PiiCategory::PhoneNumber,
            priority: 5,
        },
    ]
}
