use regex::Regex;
use uuid::Uuid;
use std::collections::HashSet;
use crate::pipeline::types::{PiiEntity, PiiCategory};

/// Heuristic-based scanner for detecting person names and entities in legal documents.
/// Uses contextual patterns and capitalization analysis rather than ML.
pub struct HeuristicScanner {
    // Patterns that capture "Name, Title" or "Name – percentage"
    name_title_re: Regex,
    name_percentage_re: Regex,
    // Capitalized word sequences on a single line (2+ words)
    cap_sequence_re: Regex,
    // Common terms that look like names but aren't
    stop_phrases: HashSet<&'static str>,
    stop_words: HashSet<&'static str>,
}

impl HeuristicScanner {
    pub fn new() -> Self {
        // "Firstname Lastname, Title" where Title is a known role
        // Supports names with multiple connector words: "Gustavo Lima Caldeira de Andrada"
        let name_title_re = Regex::new(
            r"([A-Z][a-zà-ÿ]+(?:[ ]+(?:de|da|do|dos|das|van|von|del|di|el|la|le))*(?:[ ]+[A-Z][a-zà-ÿ]+(?:[ ]+(?:de|da|do|dos|das|van|von|del|di|el|la|le))*)+),\s*(?:Manager|Shareholder|Member|Director|Officer|President|Secretary|Treasurer|Partner|Attorney|Counsel|Agent|Trustee|Executor|Administrator|Signatory|Authorized\s+Representative)"
        ).unwrap();

        // "Firstname Lastname – XX.XXXX%" (distribution tables)
        let name_percentage_re = Regex::new(
            r"([A-Z][a-zà-ÿ]+(?:[ ]+(?:de|da|do|dos|das|van|von|del|di|el|la|le))*(?:[ ]+[A-Z][a-zà-ÿ]+(?:[ ]+(?:de|da|do|dos|das|van|von|del|di|el|la|le))*)+)\s*[–—\-]\s*\d+(?:\.\d+)?%"
        ).unwrap();

        // General: 2+ consecutive capitalized words on the SAME line
        let cap_sequence_re = Regex::new(
            r"(?m)^.*?([A-Z][a-zà-ÿ]+(?:[ ]+(?:de|da|do|dos|das|van|von|del|di|el|la|le))*(?:[ ]+[A-Z][a-zà-ÿ]+(?:[ ]+(?:de|da|do|dos|das|van|von|del|di|el|la|le))*)+)"
        ).unwrap();

        let stop_phrases: HashSet<&'static str> = [
            // Two-word legal/document terms
            "Amendment No", "Amended And", "Annual Income",
            "Bank Account", "Banker Fee",
            "Capital Contribution", "Case Reference", "Closing Consideration",
            "Company Agreement", "Court File", "Credit Card",
            "Digital Ventures",
            "Earn Out", "Emergency Contact", "Employment Information",
            "Financial Information", "Full Satisfaction",
            "General Release",
            "Limited Liability",
            "Management Group", "Medical Record", "Miscellaneous Except",
            "Net Proceeds",
            "Oak Street", "Olenox Industries",
            "Restated Limited",
            "Side Agreements", "Social Security",
            "Tax Treatment", "Transaction Costs",
            "Witness Whereof",
            // Three-word
            "Client Intake Form", "Court File Number",
            "Distribution Of Transaction", "Social Security Number",
            "Medical Record Number",
        ].iter().copied().collect();

        let stop_words: HashSet<&'static str> = [
            // Common English words that get capitalized at starts
            "The", "This", "That", "These", "Those", "Which", "What",
            "When", "Where", "While", "With", "From", "Into", "Upon",
            "After", "Before", "Between", "Under", "Over", "About",
            "Each", "All", "Any", "Such", "Every", "Both", "Some",
            "Most", "Many", "Other", "Another", "Several", "Few",
            "Following", "Including", "Except", "Without",
            // Legal/document terms
            "Amendment", "Amended", "Restated", "Limited", "Liability",
            "Company", "Agreement", "Recitals", "Whereas", "Transaction",
            "Closing", "Consideration", "Proceeds", "Members",
            "Section", "Article", "Pursuant", "Herein", "Thereof",
            "General", "Release", "Representation", "Satisfaction",
            "Capital", "Contribution", "Tax", "Treatment", "Miscellaneous",
            "Witness", "Whereof", "State", "Federal", "United", "States",
            "Distribution", "Costs", "Investment", "Payment", "Prior",
            "Management", "Group", "Digital", "Ventures", "Industries",
            "Financial", "Information", "Employment", "Emergency",
            "Contact", "Credit", "Card", "Account", "Bank", "Record",
            "Medical", "Social", "Security", "Number", "Court", "File",
            "Case", "Reference", "Annual", "Income",
            "Street", "Avenue", "Road", "Boulevard", "Drive", "Lane",
            "Not", "For", "Avoidance", "Doubt", "Exhibit",
            "Schedule", "Appendix", "Annex", "Name", "Date",
        ].iter().copied().collect();

        HeuristicScanner {
            name_title_re,
            name_percentage_re,
            cap_sequence_re,
            stop_phrases,
            stop_words,
        }
    }

    pub fn scan(&self, text: &str, page: u32, existing_ranges: &[(usize, usize)]) -> Vec<PiiEntity> {
        let mut entities: Vec<PiiEntity> = Vec::new();
        let mut matched_ranges: Vec<(usize, usize)> = existing_ranges.to_vec();

        // Pass 1: High-confidence — "Name, Title" patterns
        for cap in self.name_title_re.captures_iter(text) {
            if let Some(name_match) = cap.get(1) {
                self.maybe_add_name(
                    name_match.as_str(),
                    name_match.start(),
                    name_match.end(),
                    page,
                    0.92,
                    &mut entities,
                    &mut matched_ranges,
                );
            }
        }

        // Pass 2: High-confidence — "Name – XX%" distribution patterns
        for cap in self.name_percentage_re.captures_iter(text) {
            if let Some(name_match) = cap.get(1) {
                self.maybe_add_name(
                    name_match.as_str(),
                    name_match.start(),
                    name_match.end(),
                    page,
                    0.90,
                    &mut entities,
                    &mut matched_ranges,
                );
            }
        }

        // Pass 3: Medium-confidence — general capitalized sequences on same line
        for cap in self.cap_sequence_re.captures_iter(text) {
            if let Some(m) = cap.get(1) {
                let candidate = m.as_str().trim();

                // Must not span newlines
                if candidate.contains('\n') || candidate.contains('\r') {
                    continue;
                }

                // Skip if already matched
                let overlaps = matched_ranges.iter().any(|&(rs, re)| {
                    m.start() < re && m.end() > rs
                });
                if overlaps {
                    continue;
                }

                if self.looks_like_name(candidate, text, m.start()) {
                    self.maybe_add_name(
                        candidate,
                        m.start(),
                        m.start() + candidate.len(),
                        page,
                        0.75,
                        &mut entities,
                        &mut matched_ranges,
                    );
                }
            }
        }

        entities
    }

    fn maybe_add_name(
        &self,
        text: &str,
        start: usize,
        end: usize,
        page: u32,
        confidence: f64,
        entities: &mut Vec<PiiEntity>,
        matched_ranges: &mut Vec<(usize, usize)>,
    ) {
        // No newlines in a name
        if text.contains('\n') || text.contains('\r') {
            return;
        }

        // Check overlap with existing
        let overlaps = matched_ranges.iter().any(|&(rs, re)| {
            start < re && end > rs
        });
        if overlaps {
            return;
        }

        // Filter out stop phrases
        if self.is_stop_phrase(text) {
            return;
        }

        // Must have at least 2 "real" name parts (ignoring connectors)
        let name_parts: Vec<&str> = text.split_whitespace()
            .filter(|w| {
                let lower = w.to_lowercase();
                !matches!(lower.as_str(), "de" | "da" | "do" | "dos" | "das" | "van" | "von" | "del" | "di" | "el" | "la" | "le")
            })
            .collect();

        if name_parts.len() < 2 {
            return;
        }

        // Each name part must look like a proper name word
        for part in &name_parts {
            if part.len() < 2 {
                return;
            }
            let mut chars = part.chars();
            if let Some(first) = chars.next() {
                if !first.is_uppercase() {
                    return;
                }
            }
        }

        matched_ranges.push((start, end));

        entities.push(PiiEntity {
            id: Uuid::new_v4().to_string(),
            text: text.to_string(),
            start,
            end,
            page,
            category: PiiCategory::PersonName,
            layer: "heuristic".to_string(),
            confidence,
            replacement: PiiCategory::PersonName.replacement_label().to_string(),
        });
    }

    fn is_stop_phrase(&self, text: &str) -> bool {
        // Direct match
        if self.stop_phrases.contains(text) {
            return true;
        }

        // Check if ALL words are stop words
        let words: Vec<&str> = text.split_whitespace().collect();
        if words.iter().all(|w| self.stop_words.contains(w)) {
            return true;
        }

        // Common multi-word legal phrases (case-insensitive)
        let lower = text.to_lowercase();
        let legal_fragments = [
            "limited liability", "company agreement", "amended and",
            "closing consideration", "earn out", "net proceeds",
            "transaction cost", "general release", "tax treatment",
            "capital contribution", "side agreement", "witness whereof",
            "management group", "digital ventures", "authorized representative",
            "amendment no", "full satisfaction", "file number",
            "record number", "security number", "intake form",
        ];
        for phrase in &legal_fragments {
            if lower.contains(phrase) {
                return true;
            }
        }

        false
    }

    fn looks_like_name(&self, candidate: &str, full_text: &str, pos: usize) -> bool {
        let words: Vec<&str> = candidate.split_whitespace().collect();

        // Filter to "real" name parts (skip connectors like de/da/von)
        let name_parts: Vec<&str> = words.iter()
            .filter(|w| {
                let lower = w.to_lowercase();
                !matches!(lower.as_str(), "de" | "da" | "do" | "dos" | "das" | "van" | "von" | "del" | "di" | "el" | "la" | "le")
            })
            .copied()
            .collect();

        if name_parts.len() < 2 {
            return false;
        }

        // Each name part must be Title Case and 2+ chars
        for part in &name_parts {
            if part.len() < 2 {
                return false;
            }
            let mut chars = part.chars();
            if let Some(first) = chars.next() {
                if !first.is_uppercase() {
                    return false;
                }
                // Rest should be lowercase (allow accented, hyphens, apostrophes)
                if !chars.all(|c| c.is_lowercase() || c == '-' || c == '\'') {
                    return false;
                }
            }
        }

        // If any word is a stop word, require strong context
        let has_stop_word = name_parts.iter().any(|w| self.stop_words.contains(w));

        if has_stop_word {
            // Require positive context
            return self.has_name_context(full_text, pos, candidate.len());
        }

        // No stop words — likely a real name. Still check it's not a stop phrase.
        !self.is_stop_phrase(candidate)
    }

    fn has_name_context(&self, full_text: &str, pos: usize, len: usize) -> bool {
        let before_start = pos.saturating_sub(80);
        let before = &full_text[before_start..pos];
        let after_end = (pos + len + 80).min(full_text.len());
        let after = &full_text[pos + len..after_end];

        let positive_signals = [
            "Member", "Shareholder", "Manager", "Director", "Officer",
            "by:", "By:", "Signed by", "signed by", "DocuSigned",
            "Name:", "name:", "between", "among", "party", "Party",
            "executed", "Executed", "undersigned", "behalf",
            "Signature", "signature", "witness", "Witness",
        ];

        positive_signals.iter().any(|kw| {
            before.contains(kw) || after.contains(kw)
        })
    }
}
