use uuid::Uuid;
use crate::pipeline::patterns::global::{global_patterns, PatternDef};
use crate::pipeline::patterns::{us, eu, br, de};
use crate::pipeline::types::{PiiEntity, PiiCategory};

pub struct RegexScanner {
    patterns: Vec<PatternDef>,
}

impl RegexScanner {
    pub fn new(jurisdictions: &[&str]) -> Self {
        let mut patterns = global_patterns();

        for j in jurisdictions {
            match *j {
                "us" => patterns.extend(us::us_patterns()),
                "eu" => patterns.extend(eu::eu_patterns()),
                "br" => patterns.extend(br::br_patterns()),
                "de" => patterns.extend(de::de_patterns()),
                "global" => {}
                _ => {}
            }
        }

        // Sort by priority descending so high-priority patterns match first
        patterns.sort_by(|a, b| b.priority.cmp(&a.priority));

        RegexScanner { patterns }
    }

    pub fn scan(&self, text: &str, page: u32) -> Vec<PiiEntity> {
        let mut entities: Vec<PiiEntity> = Vec::new();
        // Track matched character ranges to prevent lower-priority overlaps
        let mut matched_ranges: Vec<(usize, usize)> = Vec::new();

        for pattern in &self.patterns {
            for mat in pattern.regex.find_iter(text) {
                let start = mat.start();
                let end = mat.end();
                let matched_text = mat.as_str();

                // Skip very short matches
                if matched_text.len() < 3 {
                    continue;
                }

                // For phone numbers, require at least 7 digits
                if pattern.category == PiiCategory::PhoneNumber {
                    let digit_count = matched_text.chars().filter(|c| c.is_ascii_digit()).count();
                    if digit_count < 7 {
                        continue;
                    }
                }

                // Check if this region is already matched by a higher-priority pattern
                let overlaps = matched_ranges.iter().any(|&(rs, re)| {
                    start < re && end > rs
                });
                if overlaps {
                    continue;
                }

                matched_ranges.push((start, end));

                entities.push(PiiEntity {
                    id: Uuid::new_v4().to_string(),
                    text: matched_text.to_string(),
                    start,
                    end,
                    page,
                    category: pattern.category.clone(),
                    layer: "regex".to_string(),
                    confidence: 0.95,
                    replacement: pattern.category.replacement_label().to_string(),
                });
            }
        }

        // Sort by position for display
        entities.sort_by_key(|e| e.start);
        entities
    }
}
