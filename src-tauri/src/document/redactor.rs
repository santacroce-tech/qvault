use lopdf::{Document, Object, Stream};
use lopdf::content::{Content, Operation};
use std::path::Path;
use crate::commands::document::RedactionRect;

/// Export a redacted PDF by drawing black rectangles over redacted areas.
/// The rects use PDF coordinate space (origin bottom-left).
pub fn export_redacted_pdf(
    pdf_bytes: &[u8],
    rects: &[RedactionRect],
    output_path: &Path,
) -> Result<(), String> {
    let mut doc = Document::load_mem(pdf_bytes)
        .map_err(|e| format!("Failed to load PDF: {}", e))?;

    let pages_map = doc.get_pages();
    let mut page_numbers: Vec<u32> = pages_map.keys().copied().collect();
    page_numbers.sort();

    // Collect page ObjectIds
    let page_ids: Vec<(u32, lopdf::ObjectId)> = page_numbers
        .iter()
        .map(|&num| (num, *pages_map.get(&num).unwrap()))
        .collect();

    // Group rects by page index
    for (idx, &(_page_num, page_obj_id)) in page_ids.iter().enumerate() {
        let page_rects: Vec<&RedactionRect> = rects
            .iter()
            .filter(|r| r.page == idx as u32)
            .collect();

        if page_rects.is_empty() {
            continue;
        }

        // Build content stream operations for black rectangles
        let mut operations = Vec::new();

        // Save graphics state
        operations.push(Operation::new("q", vec![]));

        // Set fill color to black
        operations.push(Operation::new("rg", vec![
            Object::Real(0.0f32),
            Object::Real(0.0f32),
            Object::Real(0.0f32),
        ]));

        for rect in &page_rects {
            // Draw filled rectangle
            operations.push(Operation::new("re", vec![
                Object::Real(rect.x as f32),
                Object::Real(rect.y as f32),
                Object::Real(rect.width as f32),
                Object::Real(rect.height as f32),
            ]));
            operations.push(Operation::new("f", vec![]));
        }

        // Restore graphics state
        operations.push(Operation::new("Q", vec![]));

        let content = Content { operations };
        let content_bytes = content.encode()
            .map_err(|e| format!("Failed to encode content: {}", e))?;

        // Add the redaction overlay as a new stream object
        let stream = Stream::new(lopdf::dictionary! {}, content_bytes);
        let stream_id = doc.add_object(stream);

        // Get current Contents from the page and append our stream
        let page_obj = doc.get_object(page_obj_id)
            .map_err(|e| format!("Failed to get page object: {}", e))?;

        let existing_contents = if let Ok(dict) = page_obj.as_dict() {
            dict.get(b"Contents").ok().cloned()
        } else {
            None
        };

        let new_contents = match existing_contents {
            Some(Object::Reference(ref_id)) => {
                Object::Array(vec![Object::Reference(ref_id), Object::Reference(stream_id)])
            }
            Some(Object::Array(mut arr)) => {
                arr.push(Object::Reference(stream_id));
                Object::Array(arr)
            }
            _ => {
                Object::Array(vec![Object::Reference(stream_id)])
            }
        };

        // Update the page dictionary with new Contents
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
