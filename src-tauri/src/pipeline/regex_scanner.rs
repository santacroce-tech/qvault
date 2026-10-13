use regex::Regex;
use uuid::Uuid;
use crate::pipeline::patterns::global::{global_patterns, PatternDef};
use crate::pipeline::patterns::{us, eu, br, de};
use crate::pipeline::types::{PiiEntity, PiiCategory};

pub struct RegexScanner {
    patterns: Vec<PatternDef>,
    dob_context: Regex,
}

/// A birth-date label right before a date ("born on", "fecha de nacimiento",
/// "nascido em", …).
const DOB_CONTEXT: &str = r"(?i)(?:date\s+of\s+birth|birth\s*date|born(?:\s+on)?|d\.?o\.?b\.?|nacid[oa](?:\s+el)?|fecha\s+de\s+nacimiento|nascid[oa](?:\s+em)?|data\s+de\s+nascimento|geboren(?:\s+am)?|geburtsdatum)\W{0,4}$";

/// Whether a date starting at `start` follows a birth-date label (the rule
/// Layer 1 uses to type DateOfBirth; also used for Layer 3 dates).
pub fn after_dob_label(text: &str, start: usize) -> bool {
    static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    let re = RE.get_or_init(|| Regex::new(DOB_CONTEXT).unwrap());
    let mut from = start.saturating_sub(40);
    while !text.is_char_boundary(from) {
        from += 1;
    }
    re.is_match(&text[from..start])
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
        // (stable: equal priorities keep their declaration order).
        patterns.sort_by(|a, b| b.priority.cmp(&a.priority));

        // A date right after one of these is a date of birth.
        let dob_context = Regex::new(DOB_CONTEXT).unwrap();

        RegexScanner { patterns, dob_context }
    }

    pub fn scan(&self, text: &str, page: u32) -> Vec<PiiEntity> {
        let mut entities: Vec<PiiEntity> = Vec::new();
        // Track matched character ranges to prevent lower-priority overlaps
        let mut matched_ranges: Vec<(usize, usize)> = Vec::new();

        for pattern in &self.patterns {
            for caps in pattern.regex.captures_iter(text) {
                let Some(m) = caps.get(pattern.group) else { continue };
                let start = m.start();
                let mut end = m.end();
                let mut confidence = pattern.confidence;
                if let Some(check) = pattern.check {
                    match check(m.as_str()) {
                        Some((len, c)) => {
                            end = start + len;
                            confidence = c;
                        }
                        None => continue,
                    }
                }
                let matched_text = &text[start..end];

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
                let overlaps = matched_ranges.iter().any(|&(rs, re)| start < re && end > rs);
                if overlaps {
                    continue;
                }

                matched_ranges.push((start, end));

                let category = if pattern.category == PiiCategory::Date && self.after_dob_label(text, start) {
                    PiiCategory::DateOfBirth
                } else {
                    pattern.category.clone()
                };

                entities.push(PiiEntity {
                    id: Uuid::new_v4().to_string(),
                    text: matched_text.to_string(),
                    start,
                    end,
                    page,
                    replacement: category.replacement_label().to_string(),
                    category,
                    layer: "regex".to_string(),
                    confidence,
                });
            }
        }

        // Sort by position for display
        entities.sort_by_key(|e| e.start);
        entities
    }

    fn after_dob_label(&self, text: &str, start: usize) -> bool {
        let mut from = start.saturating_sub(40);
        while !text.is_char_boundary(from) {
            from += 1;
        }
        self.dob_context.is_match(&text[from..start])
    }
}
