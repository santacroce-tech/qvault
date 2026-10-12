pub mod types;
pub mod regex_scanner;
pub mod heuristic_scanner;
pub mod patterns;

use types::PiiEntity;
use regex_scanner::RegexScanner;
use heuristic_scanner::HeuristicScanner;

/// Outcome of Layer 3 for one page.
#[derive(Debug, Clone, PartialEq)]
pub enum Layer3Status {
    /// Layer 3 was not requested (disabled, or no engine passed).
    NotRun,
    Ran { entities: usize, chunks: usize, failed_chunks: usize },
    /// The engine failed fatally; Layers 1-2 results stand on their own.
    Aborted(String),
}

/// Run the PII detection pipeline. Layers 1 (regex) and 2 (heuristic) always
/// run. Layer 3 (LLM) runs when an engine is provided; whatever happens to
/// it, the Layer 1-2 results are returned.
pub fn run_pipeline(
    text: &str,
    page: u32,
    jurisdictions: &[&str],
    layer3: Option<&dyn crate::llm::llm_scanner::Layer3Engine>,
    on_llm_progress: Option<&crate::llm::llm_scanner::ProgressFn>,
) -> (Vec<PiiEntity>, Layer3Status) {
    let mut entities = run_layers_1_2(text, page, jurisdictions);
    let mut status = Layer3Status::NotRun;

    if let Some(engine) = layer3 {
        let existing_ranges: Vec<(usize, usize)> = entities.iter().map(|e| (e.start, e.end)).collect();
        let report = crate::llm::llm_scanner::scan(engine, text, page, &existing_ranges, on_llm_progress);
        status = match report.aborted {
            Some(reason) => Layer3Status::Aborted(reason),
            None => Layer3Status::Ran {
                entities: report.entities.len(),
                chunks: report.chunks,
                failed_chunks: report.failed_chunks,
            },
        };
        entities.extend(report.entities);
    }

    entities.sort_by_key(|e| e.start);
    (entities, status)
}

/// Convenience: run Layers 1+2 only (no LLM). Useful for tests and non-LLM callers.
pub fn run_pipeline_no_llm(text: &str, page: u32, jurisdictions: &[&str]) -> Vec<PiiEntity> {
    let mut entities = run_layers_1_2(text, page, jurisdictions);
    entities.sort_by_key(|e| e.start);
    entities
}

/// Layers 1 and 2: regex + heuristic detection.
fn run_layers_1_2(text: &str, page: u32, jurisdictions: &[&str]) -> Vec<PiiEntity> {
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
    entities
}
