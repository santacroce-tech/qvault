use quick_xml::events::Event;
use quick_xml::Reader;
use std::io::Read;
use std::path::Path;

use super::pdf_parser::PageText;

pub fn extract_docx_text(path: &Path) -> Result<Vec<PageText>, String> {
    let file = std::fs::File::open(path).map_err(|e| format!("Failed to open DOCX: {}", e))?;
    let mut archive =
        zip::ZipArchive::new(file).map_err(|e| format!("Failed to read DOCX zip: {}", e))?;

    // Read word/document.xml
    let mut doc_xml = String::new();
    {
        let mut entry = archive
            .by_name("word/document.xml")
            .map_err(|e| format!("Failed to find document.xml: {}", e))?;
        entry
            .read_to_string(&mut doc_xml)
            .map_err(|e| format!("Failed to read document.xml: {}", e))?;
    }

    // Parse XML to extract text from <w:t> elements
    let mut reader = Reader::from_str(&doc_xml);
    reader.trim_text(true);

    let mut text = String::new();
    let mut in_text_element = false;
    let mut paragraphs: Vec<String> = Vec::new();
    let mut current_paragraph = String::new();

    loop {
        match reader.read_event() {
            Ok(Event::Start(ref e)) | Ok(Event::Empty(ref e)) => {
                let local_name = e.local_name();
                match local_name.as_ref() {
                    b"t" => in_text_element = true,
                    b"p" => {
                        if !current_paragraph.is_empty() {
                            paragraphs.push(current_paragraph.clone());
                            current_paragraph.clear();
                        }
                    }
                    _ => {}
                }
            }
            Ok(Event::Text(ref e)) => {
                if in_text_element {
                    let t = e.unescape().unwrap_or_default().to_string();
                    current_paragraph.push_str(&t);
                    text.push_str(&t);
                }
            }
            Ok(Event::End(ref e)) => {
                if e.local_name().as_ref() == b"t" {
                    in_text_element = false;
                }
                if e.local_name().as_ref() == b"p" {
                    if !current_paragraph.is_empty() {
                        paragraphs.push(current_paragraph.clone());
                        current_paragraph.clear();
                    }
                    text.push('\n');
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(format!("XML parse error: {}", e)),
            _ => {}
        }
    }

    if !current_paragraph.is_empty() {
        paragraphs.push(current_paragraph);
    }

    // DOCX doesn't have pages per se — treat as single page for now
    Ok(vec![PageText {
        page_number: 0,
        text,
    }])
}
