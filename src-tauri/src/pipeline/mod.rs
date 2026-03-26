pub mod types;
pub mod regex_scanner;
pub mod heuristic_scanner;
pub mod patterns;

use types::PiiEntity;
use regex_scanner::RegexScanner;
use heuristic_scanner::HeuristicScanner;

pub fn run_pipeline(text: &str, page: u32, jurisdictions: &[&str]) -> Vec<PiiEntity> {
    // Layer 1: Regex-based detection (structured data: emails, phones, SSNs, dates, money)
    let scanner = RegexScanner::new(jurisdictions);
    let mut entities = scanner.scan(text, page);

    // Collect matched ranges from Layer 1 so Layer 2 doesn't overlap
    let existing_ranges: Vec<(usize, usize)> = entities.iter()
        .map(|e| (e.start, e.end))
        .collect();

    // Layer 2: Heuristic name detection (person names in legal documents)
    let heuristic = HeuristicScanner::new();
    let mut name_entities = heuristic.scan(text, page, &existing_ranges);

    entities.append(&mut name_entities);

    // Sort by position for display
    entities.sort_by_key(|e| e.start);
    entities
}
