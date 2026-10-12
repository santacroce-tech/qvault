pub mod pdf_parser;
pub mod docx_parser;
pub mod redactor;
pub mod text_match;
pub mod pdf_text;
pub(crate) mod pdf_redact;
/// Size in points of a PDF page as displayed (crop box, after /Rotate).
pub use pdf_redact::displayed_size as page_displayed_size;
mod docx_redact;

use pdf_parser::PageText;
use std::path::Path;

pub fn extract_text(path: &Path) -> Result<Vec<PageText>, String> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    match ext.as_str() {
        "pdf" => pdf_parser::extract_pdf_text(path),
        "docx" => docx_parser::extract_docx_text(path),
        _ => Err(format!("Unsupported file type: {}", ext)),
    }
}
