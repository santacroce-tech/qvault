use lopdf::{dictionary, Document, Object, Stream};
use lopdf::content::{Content, Operation};
use std::io::{Read, Write};
use std::path::Path;
use crate::commands::document::RedactionRect;
use crate::storage::database::RedactionRow;

#[derive(Debug, Clone, Copy)]
pub enum RedactionMode {
    BlackBar,
    TextReplace,
}

/// Export a redacted PDF. Supports two modes:
/// - BlackBar: draws padded black rectangles over redacted areas
/// - TextReplace: draws white rectangles + replacement text labels
pub fn export_redacted_pdf(
    pdf_bytes: &[u8],
    rects: &[RedactionRect],
    output_path: &Path,
    mode: RedactionMode,
) -> Result<(), String> {
    let mut doc = Document::load_mem(pdf_bytes)
        .map_err(|e| format!("Failed to load PDF: {}", e))?;

    // For TextReplace mode, ensure Helvetica font is available on each page that needs it
    let font_name = b"F_QV_Redact".to_vec();

    let pages_map = doc.get_pages();
    let mut page_numbers: Vec<u32> = pages_map.keys().copied().collect();
    page_numbers.sort();

    let page_ids: Vec<(u32, lopdf::ObjectId)> = page_numbers
        .iter()
        .map(|&num| (num, *pages_map.get(&num).unwrap()))
        .collect();

    for (idx, &(_page_num, page_obj_id)) in page_ids.iter().enumerate() {
        let page_rects: Vec<&RedactionRect> = rects
            .iter()
            .filter(|r| r.page == idx as u32)
            .collect();

        if page_rects.is_empty() {
            continue;
        }

        // For text replace mode, add Helvetica font resource to the page
        if matches!(mode, RedactionMode::TextReplace) {
            add_helvetica_font(&mut doc, page_obj_id, &font_name)?;
        }

        let operations = match mode {
            RedactionMode::BlackBar => build_black_bar_ops(&page_rects),
            RedactionMode::TextReplace => build_text_replace_ops(&page_rects, &font_name),
        };

        let content = Content { operations };
        let content_bytes = content.encode()
            .map_err(|e| format!("Failed to encode content: {}", e))?;

        // Wrap original content in q...Q to guarantee CTM reset before our overlay.
        // Many DOCX-to-PDF converters use a Y-flip CTM (cm 1 0 0 -1 0 h) that persists
        // across the content stream. By wrapping in q/Q, the CTM is restored to the
        // page's default user space before our overlay runs.
        let q_stream = Stream::new(lopdf::dictionary! {}, b"q\n".to_vec());
        let q_id = doc.add_object(q_stream);

        // Q restores state, then our overlay follows
        let mut suffix_bytes = b"Q\n".to_vec();
        suffix_bytes.extend_from_slice(&content_bytes);
        let overlay_stream = Stream::new(lopdf::dictionary! {}, suffix_bytes);
        let overlay_id = doc.add_object(overlay_stream);

        let page_obj = doc.get_object(page_obj_id)
            .map_err(|e| format!("Failed to get page object: {}", e))?;

        let existing_contents = if let Ok(dict) = page_obj.as_dict() {
            dict.get(b"Contents").ok().cloned()
        } else {
            None
        };

        // Build new Contents array: [q_stream, ...original..., Q+overlay_stream]
        let new_contents = match existing_contents {
            Some(Object::Reference(ref_id)) => {
                Object::Array(vec![
                    Object::Reference(q_id),
                    Object::Reference(ref_id),
                    Object::Reference(overlay_id),
                ])
            }
            Some(Object::Array(mut arr)) => {
                arr.insert(0, Object::Reference(q_id));
                arr.push(Object::Reference(overlay_id));
                Object::Array(arr)
            }
            _ => {
                Object::Array(vec![
                    Object::Reference(q_id),
                    Object::Reference(overlay_id),
                ])
            }
        };

        let page_obj_mut = doc.get_object_mut(page_obj_id)
            .map_err(|e| format!("Failed to get mutable page object: {}", e))?;

        if let Ok(dict) = page_obj_mut.as_dict_mut() {
            dict.set("Contents", new_contents);
        }
    }

    doc.save(output_path)
        .map_err(|e| format!("Failed to save redacted PDF: {}", e))?;

    Ok(())
}

/// Build operations for black bar mode: white background + black rectangle with padding
fn build_black_bar_ops(page_rects: &[&RedactionRect]) -> Vec<Operation> {
    let mut ops = Vec::new();
    let padding = 2.0_f32;

    ops.push(Operation::new("q", vec![]));

    for rect in page_rects {
        let x = rect.x as f32 - padding;
        let y = rect.y as f32 - padding;
        let w = rect.width as f32 + padding * 2.0;
        let h = rect.height as f32 + padding * 2.0;

        // White background first (covers text layer for copy-paste protection)
        ops.push(Operation::new("rg", vec![
            Object::Real(1.0), Object::Real(1.0), Object::Real(1.0),
        ]));
        ops.push(Operation::new("re", vec![
            Object::Real(x), Object::Real(y), Object::Real(w), Object::Real(h),
        ]));
        ops.push(Operation::new("f", vec![]));

        // Black rectangle on top
        ops.push(Operation::new("rg", vec![
            Object::Real(0.0), Object::Real(0.0), Object::Real(0.0),
        ]));
        ops.push(Operation::new("re", vec![
            Object::Real(x), Object::Real(y), Object::Real(w), Object::Real(h),
        ]));
        ops.push(Operation::new("f", vec![]));
    }

    ops.push(Operation::new("Q", vec![]));
    ops
}

/// Group adjacent rects that belong to the same redaction (same category+replacement).
/// Returns groups where each group gets one label drawn over a merged bounding box.
fn group_redaction_rects<'a>(page_rects: &[&'a RedactionRect]) -> Vec<Vec<&'a RedactionRect>> {
    if page_rects.is_empty() {
        return vec![];
    }

    let mut groups: Vec<Vec<&'a RedactionRect>> = vec![];
    let mut current_group: Vec<&'a RedactionRect> = vec![page_rects[0]];

    for i in 1..page_rects.len() {
        let prev = page_rects[i - 1];
        let curr = page_rects[i];

        // Same redaction if same category + replacement and rects are close together
        let same_category = curr.category == prev.category;
        let same_replacement = curr.replacement == prev.replacement;
        let close_y = (curr.y - prev.y).abs() < (prev.height * 1.5).max(5.0);
        let close_x = (curr.x - (prev.x + prev.width)).abs() < 20.0
            || (prev.x - (curr.x + curr.width)).abs() < 20.0;

        if same_category && same_replacement && close_y && close_x {
            current_group.push(curr);
        } else {
            groups.push(current_group);
            current_group = vec![curr];
        }
    }
    groups.push(current_group);
    groups
}

/// Build operations for text replacement mode: white rectangles + one label per group
fn build_text_replace_ops(page_rects: &[&RedactionRect], font_name: &[u8]) -> Vec<Operation> {
    let mut ops = Vec::new();
    let padding = 1.0_f32;

    ops.push(Operation::new("q", vec![]));

    let groups = group_redaction_rects(page_rects);

    for group in &groups {
        // Draw white rectangles over ALL rects in the group
        ops.push(Operation::new("rg", vec![
            Object::Real(1.0), Object::Real(1.0), Object::Real(1.0),
        ]));
        for rect in group {
            let x = rect.x as f32 - padding;
            let y = rect.y as f32 - padding;
            let w = rect.width as f32 + padding * 2.0;
            let h = rect.height as f32 + padding * 2.0;
            ops.push(Operation::new("re", vec![
                Object::Real(x), Object::Real(y), Object::Real(w), Object::Real(h),
            ]));
            ops.push(Operation::new("f", vec![]));
        }

        // Compute bounding box of the group for label placement
        let min_x = group.iter().map(|r| r.x).fold(f64::MAX, f64::min);
        let min_y = group.iter().map(|r| r.y).fold(f64::MAX, f64::min);
        let max_x = group.iter().map(|r| r.x + r.width).fold(f64::MIN, f64::max);
        let max_h = group.iter().map(|r| r.height).fold(0.0_f64, f64::max);
        let group_width = max_x - min_x;

        // Build label and size it to fit the bounding box
        let label = build_label(group[0]);
        let font_size = pick_font_size(max_h as f32, &label, group_width as f32);

        // Draw ONE label using absolute positioning (Tm matrix)
        ops.push(Operation::new("BT", vec![]));
        ops.push(Operation::new("Tf", vec![
            Object::Name(font_name.to_vec()),
            Object::Real(font_size),
        ]));
        ops.push(Operation::new("rg", vec![
            Object::Real(0.4), Object::Real(0.4), Object::Real(0.4),
        ]));
        // Tm sets an absolute text matrix: [fontSize 0 0 fontSize x y]
        ops.push(Operation::new("Tm", vec![
            Object::Real(font_size),
            Object::Real(0.0),
            Object::Real(0.0),
            Object::Real(font_size),
            Object::Real(min_x as f32),
            Object::Real(min_y as f32 + 2.0),
        ]));
        ops.push(Operation::new("Tj", vec![
            Object::String(label.into_bytes(), lopdf::StringFormat::Literal),
        ]));
        ops.push(Operation::new("ET", vec![]));
    }

    ops.push(Operation::new("Q", vec![]));
    ops
}

/// Build the label text from the redaction's replacement or category
fn build_label(rect: &RedactionRect) -> String {
    if let Some(ref repl) = rect.replacement {
        if !repl.is_empty() {
            return repl.clone();
        }
    }
    if let Some(ref cat) = rect.category {
        return format!("[{}]", category_label(cat));
    }
    "[REDACTED]".to_string()
}

/// Map PII category to a short human-readable label
fn category_label(cat: &str) -> &str {
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

/// Pick a font size that fits within the rect height, and roughly within the width
fn pick_font_size(height: f32, label: &str, width: f32) -> f32 {
    // Start from rect height minus some margin
    let max_by_height = (height - 2.0).max(4.0).min(14.0);
    // Rough approximation: each char is ~0.5 * font_size wide in Helvetica
    let chars = label.len() as f32;
    let max_by_width = if chars > 0.0 { (width / (chars * 0.5)).min(14.0) } else { 14.0 };
    max_by_height.min(max_by_width).max(4.0)
}

/// Add a Helvetica font resource to a page's Resources dictionary.
/// Handles both inline and indirect-reference Resources/Font dicts.
fn add_helvetica_font(
    doc: &mut Document,
    page_obj_id: lopdf::ObjectId,
    font_name: &[u8],
) -> Result<(), String> {
    let font_dict = dictionary! {
        "Type" => Object::Name(b"Font".to_vec()),
        "Subtype" => Object::Name(b"Type1".to_vec()),
        "BaseFont" => Object::Name(b"Helvetica".to_vec()),
    };
    let font_id = doc.add_object(font_dict);

    // Resolve the Resources object ID (may be inline or indirect reference)
    let resources_id = {
        let page_obj = doc.get_object(page_obj_id)
            .map_err(|e| format!("Failed to get page: {}", e))?;
        let page_dict = page_obj.as_dict()
            .map_err(|e| format!("Page is not a dict: {}", e))?;

        match page_dict.get(b"Resources") {
            Ok(Object::Reference(id)) => Some(*id),
            Ok(_) => None, // inline dict — we'll handle via page_obj_id
            Err(_) => None, // missing — we'll create it
        }
    };

    // If Resources is an indirect reference, modify the referenced object directly
    if let Some(res_id) = resources_id {
        let res_obj = doc.get_object_mut(res_id)
            .map_err(|e| format!("Failed to get Resources object: {}", e))?;
        let res_dict = res_obj.as_dict_mut()
            .map_err(|e| format!("Resources ref is not a dict: {}", e))?;

        // Resolve Font — may also be indirect
        let font_ref_id = match res_dict.get(b"Font") {
            Ok(Object::Reference(id)) => Some(*id),
            _ => None,
        };

        if let Some(fid) = font_ref_id {
            // Font is an indirect reference
            let font_obj = doc.get_object_mut(fid)
                .map_err(|e| format!("Failed to get Font object: {}", e))?;
            let fd = font_obj.as_dict_mut()
                .map_err(|e| format!("Font ref is not a dict: {}", e))?;
            fd.set(font_name, Object::Reference(font_id));
        } else {
            // Font is inline or missing
            let res_obj = doc.get_object_mut(res_id)
                .map_err(|e| format!("Failed to get Resources: {}", e))?;
            let res_dict = res_obj.as_dict_mut()
                .map_err(|e| format!("Resources is not a dict: {}", e))?;
            if res_dict.get(b"Font").is_err() {
                res_dict.set("Font", dictionary! {});
            }
            let fonts = res_dict.get_mut(b"Font")
                .map_err(|e| format!("Failed to get Font: {}", e))?;
            let fd = fonts.as_dict_mut()
                .map_err(|e| format!("Font is not a dict: {}", e))?;
            fd.set(font_name, Object::Reference(font_id));
        }
    } else {
        // Resources is inline or missing — modify via the page dict
        let page_obj = doc.get_object_mut(page_obj_id)
            .map_err(|e| format!("Failed to get page: {}", e))?;
        let page_dict = page_obj.as_dict_mut()
            .map_err(|e| format!("Page is not a dict: {}", e))?;

        if page_dict.get(b"Resources").is_err() {
            page_dict.set("Resources", dictionary! {});
        }
        let resources = page_dict.get_mut(b"Resources")
            .map_err(|e| format!("Failed to get Resources: {}", e))?;
        let res_dict = resources.as_dict_mut()
            .map_err(|e| format!("Resources is not a dict: {}", e))?;

        if res_dict.get(b"Font").is_err() {
            res_dict.set("Font", dictionary! {});
        }
        let fonts = res_dict.get_mut(b"Font")
            .map_err(|e| format!("Failed to get Font: {}", e))?;
        let fd = fonts.as_dict_mut()
            .map_err(|e| format!("Font is not a dict: {}", e))?;
        fd.set(font_name, Object::Reference(font_id));
    }

    Ok(())
}

// ─── DOCX Clean Export ───────────────────────────────────────────────────────

/// Export a clean DOCX by replacing PII text directly in the XML.
/// Preserves all formatting, styles, and layout from the original.
pub fn export_redacted_docx(
    docx_bytes: &[u8],
    redactions: &[RedactionRow],
    output_path: &Path,
    mode: RedactionMode,
) -> Result<(), String> {
    let cursor = std::io::Cursor::new(docx_bytes);
    let mut archive = zip::ZipArchive::new(cursor)
        .map_err(|e| format!("Failed to read DOCX zip: {}", e))?;

    // Collect approved redactions sorted by longest-first (to avoid partial matches)
    let mut approved: Vec<&RedactionRow> = redactions
        .iter()
        .filter(|r| r.status == "approved")
        .collect();
    approved.sort_by(|a, b| b.original_text.len().cmp(&a.original_text.len()));

    // Read document.xml and apply replacements
    let mut doc_xml = String::new();
    {
        let mut entry = archive.by_name("word/document.xml")
            .map_err(|e| format!("Failed to find document.xml: {}", e))?;
        entry.read_to_string(&mut doc_xml)
            .map_err(|e| format!("Failed to read document.xml: {}", e))?;
    }

    // Replace PII text in the XML content
    let redacted_xml = replace_pii_in_xml(&doc_xml, &approved, mode);

    // Write the modified DOCX (copy all entries, replace document.xml)
    let output_file = std::fs::File::create(output_path)
        .map_err(|e| format!("Failed to create output DOCX: {}", e))?;
    let mut writer = zip::ZipWriter::new(output_file);

    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)
            .map_err(|e| format!("Failed to read zip entry: {}", e))?;
        let name = entry.name().to_string();

        let options = zip::write::FileOptions::default()
            .compression_method(entry.compression());

        writer.start_file(&name, options)
            .map_err(|e| format!("Failed to start zip entry: {}", e))?;

        if name == "word/document.xml" {
            writer.write_all(redacted_xml.as_bytes())
                .map_err(|e| format!("Failed to write document.xml: {}", e))?;
        } else {
            let mut buf = Vec::new();
            entry.read_to_end(&mut buf)
                .map_err(|e| format!("Failed to read entry: {}", e))?;
            writer.write_all(&buf)
                .map_err(|e| format!("Failed to write entry: {}", e))?;
        }
    }

    writer.finish()
        .map_err(|e| format!("Failed to finalize DOCX: {}", e))?;

    Ok(())
}

/// Replace PII text in DOCX XML content.
fn replace_pii_in_xml(xml: &str, approved: &[&RedactionRow], mode: RedactionMode) -> String {
    let mut result = xml.to_string();

    for redaction in approved {
        let original = &redaction.original_text;
        if original.is_empty() {
            continue;
        }

        let replacement = match mode {
            RedactionMode::TextReplace => {
                if !redaction.replacement.is_empty() {
                    redaction.replacement.clone()
                } else {
                    format!("[{}]", category_label(&redaction.category))
                }
            }
            RedactionMode::BlackBar => {
                "\u{2588}".repeat(original.len().min(20))
            }
        };

        // Escape XML special chars in the replacement
        let xml_safe_replacement = replacement
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('"', "&quot;");

        // Escape the original text for XML matching
        let xml_safe_original = original
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('"', "&quot;");

        result = result.replace(&xml_safe_original, &xml_safe_replacement);

        // Also try the unescaped version
        if xml_safe_original != *original {
            result = result.replace(original, &xml_safe_replacement);
        }
    }

    result
}

// ─── Flattened PDF from page images ──────────────────────────────────────────

/// Page image data for creating a flattened PDF
pub struct PageImage {
    pub jpeg_bytes: Vec<u8>,
    pub width: u32,
    pub height: u32,
    /// Original PDF page width in points (for correct sizing)
    pub page_width: f64,
    /// Original PDF page height in points
    pub page_height: f64,
}

/// Create a new PDF where each page is a rasterized JPEG image.
/// This completely destroys the text layer — nothing is selectable/copyable.
pub fn create_pdf_from_images(
    pages: &[PageImage],
    output_path: &Path,
) -> Result<(), String> {
    let mut doc = Document::with_version("1.5");

    let pages_id = doc.new_object_id();
    let mut page_ids = Vec::new();

    for page in pages {
        // Create image XObject (JPEG)
        let img_stream = Stream::new(
            lopdf::dictionary! {
                "Type" => Object::Name(b"XObject".to_vec()),
                "Subtype" => Object::Name(b"Image".to_vec()),
                "Width" => Object::Integer(page.width as i64),
                "Height" => Object::Integer(page.height as i64),
                "ColorSpace" => Object::Name(b"DeviceRGB".to_vec()),
                "BitsPerComponent" => Object::Integer(8),
                "Filter" => Object::Name(b"DCTDecode".to_vec()),
            },
            page.jpeg_bytes.clone(),
        );
        let img_id = doc.add_object(img_stream);

        // Content stream: draw the image scaled to fill the page
        let content_ops = Content {
            operations: vec![
                Operation::new("q", vec![]),
                Operation::new("cm", vec![
                    Object::Real(page.page_width as f32),
                    Object::Real(0.0),
                    Object::Real(0.0),
                    Object::Real(page.page_height as f32),
                    Object::Real(0.0),
                    Object::Real(0.0),
                ]),
                Operation::new("Do", vec![Object::Name(b"Img0".to_vec())]),
                Operation::new("Q", vec![]),
            ],
        };
        let content_bytes = content_ops.encode()
            .map_err(|e| format!("Failed to encode content: {}", e))?;
        let content_id = doc.add_object(Stream::new(lopdf::dictionary! {}, content_bytes));

        // Resources with the image XObject
        let resources = lopdf::dictionary! {
            "XObject" => lopdf::dictionary! {
                "Img0" => Object::Reference(img_id),
            },
        };
        let resources_id = doc.add_object(resources);

        // Page dictionary
        let page_dict = lopdf::dictionary! {
            "Type" => Object::Name(b"Page".to_vec()),
            "Parent" => Object::Reference(pages_id),
            "MediaBox" => Object::Array(vec![
                Object::Real(0.0),
                Object::Real(0.0),
                Object::Real(page.page_width as f32),
                Object::Real(page.page_height as f32),
            ]),
            "Contents" => Object::Reference(content_id),
            "Resources" => Object::Reference(resources_id),
        };
        let page_id = doc.add_object(page_dict);
        page_ids.push(page_id);
    }

    // Pages dictionary
    let kids: Vec<Object> = page_ids.iter().map(|id| Object::Reference(*id)).collect();
    let pages_dict = lopdf::dictionary! {
        "Type" => Object::Name(b"Pages".to_vec()),
        "Count" => Object::Integer(page_ids.len() as i64),
        "Kids" => Object::Array(kids),
    };
    doc.objects.insert(pages_id, Object::Dictionary(pages_dict));

    // Catalog
    let catalog = lopdf::dictionary! {
        "Type" => Object::Name(b"Catalog".to_vec()),
        "Pages" => Object::Reference(pages_id),
    };
    let catalog_id = doc.add_object(catalog);
    doc.trailer.set("Root", Object::Reference(catalog_id));

    doc.save(output_path)
        .map_err(|e| format!("Failed to save flattened PDF: {}", e))?;

    Ok(())
}
