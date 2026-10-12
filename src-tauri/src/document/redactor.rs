//! Export entry points. PDF export lives in `pdf_redact`, DOCX in `docx_redact`.
//!
//! There is deliberately no "vector" PDF export: drawing boxes over a PDF
//! leaves the text underneath extractable. PDFs are only exported through
//! `export_flattened_pdf`, which rasterizes every page that has a redaction.

use crate::storage::database::RedactionRow;

pub use super::docx_redact::export_redacted_docx;
pub use super::pdf_redact::{
    export_flattened_pdf, export_flattened_pdf_ocr, OcrExport, PageRaster, PdfExportSummary, PixelRect,
};

#[derive(Debug, Clone, Copy)]
pub enum RedactionMode {
    /// Replace with a fixed run of block characters.
    BlackBar,
    /// Replace with the redaction's replacement label (e.g. `[NAME]`).
    TextReplace,
}

/// The text that replaces a redacted span in text-based exports (DOCX).
pub fn replacement_for(row: &RedactionRow, mode: RedactionMode) -> String {
    match mode {
        RedactionMode::TextReplace if !row.replacement.trim().is_empty() => row.replacement.clone(),
        RedactionMode::TextReplace => format!("[{}]", category_label(&row.category)),
        // Fixed width: the length of the original would itself be a hint.
        RedactionMode::BlackBar => "\u{2588}".repeat(8),
    }
}

/// Map PII category to a short human-readable label
pub fn category_label(cat: &str) -> &str {
    match cat {
        "PersonName" => "NAME",
        "Email" => "EMAIL",
        "PhoneNumber" => "PHONE",
        "Address" => "ADDRESS",
        "DateOfBirth" => "DOB",
        "Date" => "DATE",
        "MonetaryAmount" => "AMOUNT",
        "SocialSecurityNumber" => "SSN",
        "NationalId" => "ID",
        "PassportNumber" => "PASSPORT",
        "DriversLicense" => "LICENSE",
        "BankAccount" => "BANK",
        "CreditCard" => "CARD",
        "IBAN" => "IBAN",
        "TaxId" => "TAX ID",
        "CaseNumber" => "CASE",
        "CourtFileNumber" => "COURT",
        "ClientId" => "CLIENT",
        "IpAddress" => "IP",
        "Url" => "URL",
        "LicensePlate" => "PLATE",
        "MedicalRecordNumber" => "MRN",
        "CompanyName" => "COMPANY",
        _ => "REDACTED",
    }
}
