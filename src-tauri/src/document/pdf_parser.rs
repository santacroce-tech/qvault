use lopdf::Document;
use std::path::Path;

pub struct PageText {
    pub page_number: u32,
    pub text: String,
}

pub fn extract_pdf_text(path: &Path) -> Result<Vec<PageText>, String> {
    let doc = Document::load(path).map_err(|e| format!("Failed to load PDF: {}", e))?;

    let pages = doc.get_pages();
    let mut result = Vec::new();

    let mut page_numbers: Vec<u32> = pages.keys().copied().collect();
    page_numbers.sort();

    for (idx, &page_num) in page_numbers.iter().enumerate() {
        let text = doc
            .extract_text(&[page_num])
            .unwrap_or_default();

        result.push(PageText {
            page_number: idx as u32,
            text,
        });
    }

    Ok(result)
}
