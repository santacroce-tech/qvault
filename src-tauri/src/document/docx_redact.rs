//! Redacted DOCX export.
//!
//! Every XML part of the package is parsed and rewritten with a real XML
//! reader/writer (never string replacement on raw XML):
//!
//! - tracked changes are accepted (deleted text and revision history removed,
//!   insertions unwrapped), see `docx_parser::clean_tracked_changes`;
//! - in the run-concatenated text of each part, every occurrence of every
//!   approved string (plus the exact spans the scanner reported) is replaced:
//!   the replacement goes into the first run of the span, keeping its
//!   formatting, and the rest of the span is deleted from the following runs;
//! - other text nodes (field codes, document properties, custom XML, chart
//!   caches, …) and text-bearing attributes (alt text, tooltips, field
//!   instructions, external link targets) get the same replacement;
//! - comment and revision authors/initials are blanked, personal fields in
//!   `docProps/core.xml` and `docProps/app.xml` are emptied, and the package
//!   thumbnail (a picture of page 1) is removed.
//!
//! The output is then re-read and verified: every part must be well-formed and
//! no approved string may survive anywhere. Otherwise the file is deleted.

use std::collections::{HashMap, HashSet};
use std::io::{Read, Write};
use std::path::Path;

use quick_xml::events::{BytesStart, BytesText, Event};
use quick_xml::Writer;

use super::docx_parser::{
    clean_tracked_changes, event_text, flow, local, parse_part, scan_docx_bytes, Flow, Ns, PieceKind,
    XmlPart,
};
use super::redactor::{replacement_for, RedactionMode};
use super::text_match::{Folded, Needle};
use crate::storage::database::RedactionRow;

/// Attributes whose value is blanked wherever they appear.
const BLANK_ATTRS: &[&[u8]] = &[b"author", b"initials", b"userId", b"providerId"];
/// Attributes that can hold document text (alt text, titles, tooltips, field codes).
const TEXT_ATTRS: &[&[u8]] = &[b"descr", b"title", b"tooltip", b"instr"];
const CORE_BLANK: &[&[u8]] = &[
    b"creator",
    b"lastModifiedBy",
    b"title",
    b"subject",
    b"keywords",
    b"description",
    b"category",
    b"contentStatus",
    b"identifier",
];
const APP_BLANK: &[&[u8]] = &[b"Company", b"Manager", b"HyperlinkBase"];
/// Elements whose text nodes are never rewritten outside the run flow, because
/// they hold layout numbers (e.g. `wp:posOffset`) rather than prose. Only
/// field codes are rewritten in WordprocessingML/DrawingML parts.
const WORD_TEXT_NODES: &[&[u8]] = &[b"instrText"];

struct Target {
    needle: Needle,
    replacement: String,
}

fn is_xml_part(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    n.ends_with(".xml") || n.ends_with(".rels") || n.ends_with(".vml")
}

fn is_thumbnail(name: &str) -> bool {
    name.to_ascii_lowercase().starts_with("docprops/thumbnail")
}

/// Parts made of WordprocessingML prose, where only run text and field codes
/// are rewritten.
fn is_word_part(name: &str) -> bool {
    name.starts_with("word/") && !name.starts_with("word/charts/") && !name.starts_with("word/diagrams/")
}

/// Export a redacted copy of `docx_bytes`. Only `approved` rows are applied.
pub fn export_redacted_docx(
    docx_bytes: &[u8],
    redactions: &[RedactionRow],
    output_path: &Path,
    mode: RedactionMode,
) -> Result<(), String> {
    let result = export_inner(docx_bytes, redactions, output_path, mode);
    if result.is_err() {
        let _ = std::fs::remove_file(output_path);
    }
    result
}

fn export_inner(
    docx_bytes: &[u8],
    redactions: &[RedactionRow],
    output_path: &Path,
    mode: RedactionMode,
) -> Result<(), String> {
    let approved: Vec<&RedactionRow> = redactions.iter().filter(|r| r.status == "approved").collect();

    // Distinct approved strings, longest first so overlapping matches prefer
    // the longer one.
    let mut targets: Vec<Target> = Vec::new();
    let mut seen = HashSet::new();
    for r in &approved {
        if let Some(needle) = Needle::new(&r.original_text) {
            if seen.insert(needle.key()) {
                targets.push(Target { needle, replacement: replacement_for(r, mode) });
            }
        }
    }
    targets.sort_by_key(|t| std::cmp::Reverse(t.needle.key().chars().count()));

    // Exact spans reported by the scanner, mapped back to their part.
    let mut part_spans: HashMap<String, Vec<(usize, usize, String)>> = HashMap::new();
    let scan = scan_docx_bytes(docx_bytes)?;
    for r in approved.iter().filter(|r| r.page == 0) {
        let (s, e) = (r.start_offset.max(0) as usize, r.end_offset.max(0) as usize);
        if e <= s || scan.text.get(s..e) != Some(r.original_text.as_str()) {
            continue; // stale offsets; the string match below still applies
        }
        for seg in &scan.segments {
            let (cs, ce) = (s.max(seg.start), e.min(seg.end));
            if cs < ce {
                part_spans.entry(seg.part.clone()).or_default().push((
                    cs - seg.start,
                    ce - seg.start,
                    replacement_for(r, mode),
                ));
            }
        }
    }

    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(docx_bytes))
        .map_err(|e| format!("Failed to read DOCX zip: {}", e))?;
    let file = std::fs::File::create(output_path).map_err(|e| format!("Failed to create output DOCX: {}", e))?;
    let mut writer = zip::ZipWriter::new(file);
    let options = zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Deflated);

    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| format!("Failed to read zip entry: {}", e))?;
        let name = entry.name().to_string();
        if entry.is_dir() || is_thumbnail(&name) {
            continue;
        }
        let mut buf = Vec::new();
        entry.read_to_end(&mut buf).map_err(|e| format!("Failed to read {}: {}", name, e))?;
        drop(entry);
        let out = if is_xml_part(&name) {
            let xml = String::from_utf8(buf).map_err(|_| format!("Export failed: {} is not UTF-8.", name))?;
            let spans = part_spans.get(&name).map(|v| v.as_slice()).unwrap_or(&[]);
            redact_part(&name, &xml, &targets, spans)
                .map_err(|e| format!("Export failed while processing {}: {}", name, e))?
        } else {
            buf
        };
        writer.start_file(&name, options).map_err(|e| format!("Failed to write {}: {}", name, e))?;
        writer.write_all(&out).map_err(|e| format!("Failed to write {}: {}", name, e))?;
    }
    writer.finish().map_err(|e| format!("Failed to finalize DOCX: {}", e))?;
    drop(writer);

    verify_docx(output_path, &targets)
}

// ─── Per-part rewriting ──────────────────────────────────────────────────────

/// Merge overlapping spans; the first (earliest, then longest) wins the label.
fn merge_spans(mut spans: Vec<(usize, usize, String)>) -> Vec<(usize, usize, String)> {
    spans.sort_by(|a, b| a.0.cmp(&b.0).then(b.1.cmp(&a.1)));
    let mut out: Vec<(usize, usize, String)> = Vec::new();
    for s in spans {
        if let Some(last) = out.last_mut() {
            if s.0 < last.1 {
                last.1 = last.1.max(s.1);
                continue;
            }
        }
        out.push(s);
    }
    out
}

fn replace_in_string(s: &str, targets: &[Target]) -> Option<String> {
    let folded = Folded::new(s);
    let mut spans = Vec::new();
    for t in targets {
        for (a, b) in folded.find_all(&t.needle) {
            spans.push((a, b, t.replacement.clone()));
        }
    }
    if spans.is_empty() {
        return None;
    }
    let mut out = String::new();
    let mut pos = 0;
    for (a, b, r) in merge_spans(spans) {
        out.push_str(&s[pos..a]);
        out.push_str(&r);
        pos = b;
    }
    out.push_str(&s[pos..]);
    Some(out)
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or("");
            if let Ok(v) = u8::from_str_radix(hex, 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Apply spans to the flow's text pieces. Returns new text per event index and
/// the `t` elements that need `xml:space="preserve"`.
fn apply_flow_spans(flow: &Flow, spans: &[(usize, usize, String)]) -> (HashMap<usize, String>, HashSet<usize>) {
    let mut new_text: HashMap<usize, String> = HashMap::new();
    let mut preserve = HashSet::new();
    let mut placed = vec![false; spans.len()];
    for piece in &flow.pieces {
        let PieceKind::Text { event, t_start } = piece.kind else { continue };
        let (ps, pe) = (piece.start, piece.end);
        let mut touched = false;
        let mut out = String::new();
        let mut pos = ps;
        for (k, (s, e, r)) in spans.iter().enumerate() {
            // Spans that end before this piece or start after it don't touch it,
            // except that an unplaced span whose start lay in virtual text gets
            // its label in the first editable piece after it.
            if *e <= ps || *s >= pe {
                continue;
            }
            touched = true;
            let s_in = (*s).max(ps);
            if s_in > pos {
                out.push_str(&flow.text[pos..s_in]);
            }
            if !placed[k] {
                out.push_str(r);
                placed[k] = true;
            }
            pos = (*e).min(pe).max(pos);
        }
        if touched {
            if pos < pe {
                out.push_str(&flow.text[pos..pe]);
            }
            new_text.insert(event, out);
            preserve.insert(t_start);
        }
    }
    (new_text, preserve)
}

fn rebuild_start(
    e: &BytesStart,
    mut f: impl FnMut(&[u8], &str) -> Option<String>,
    extra: Option<(&str, &str)>,
) -> Result<BytesStart<'static>, String> {
    let name = String::from_utf8_lossy(e.name().as_ref()).into_owned();
    let mut ne = BytesStart::new(name);
    let mut has_extra = false;
    for attr in e.attributes().with_checks(false) {
        let a = attr.map_err(|e| e.to_string())?;
        let key = String::from_utf8_lossy(a.key.as_ref()).into_owned();
        let val = a.unescape_value().map_err(|e| e.to_string())?.into_owned();
        if let Some((k, _)) = extra {
            if key == k {
                has_extra = true;
            }
        }
        if let Some(v) = f(a.key.as_ref(), &val) {
            ne.push_attribute((key.as_str(), v.as_str()));
        }
    }
    if let Some((k, v)) = extra {
        if !has_extra {
            ne.push_attribute((k, v));
        }
    }
    Ok(ne)
}

fn attr_rewrite(
    key: &[u8],
    val: &str,
    targets: &[Target],
    external_target: bool,
) -> Option<String> {
    let l = local(key);
    if BLANK_ATTRS.contains(&l) {
        return Some(String::new());
    }
    if TEXT_ATTRS.contains(&l) {
        return Some(replace_in_string(val, targets).unwrap_or_else(|| val.to_string()));
    }
    if key == b"Target" && external_target {
        if let Some(v) = replace_in_string(val, targets) {
            return Some(v);
        }
        let decoded = percent_decode(val);
        if replace_in_string(&decoded, targets).is_some() {
            return Some("#".to_string());
        }
    }
    Some(val.to_string())
}

fn is_external(e: &BytesStart) -> bool {
    e.attributes().with_checks(false).flatten().any(|a| {
        a.key.as_ref() == b"TargetMode" && a.value.as_ref() == b"External"
    })
}

fn is_thumbnail_rel(e: &BytesStart) -> bool {
    e.attributes().with_checks(false).flatten().any(|a| {
        a.key.as_ref() == b"Type" && a.value.as_ref().ends_with(b"/thumbnail")
    })
}

fn redact_part(
    name: &str,
    xml: &str,
    targets: &[Target],
    offset_spans: &[(usize, usize, String)],
) -> Result<Vec<u8>, String> {
    let part: XmlPart = clean_tracked_changes(parse_part(xml)?);
    let fl = flow(&part);

    // Run-flow replacement.
    let mut spans: Vec<(usize, usize, String)> = offset_spans.to_vec();
    let folded = Folded::new(&fl.text);
    for t in targets {
        for (a, b) in folded.find_all(&t.needle) {
            spans.push((a, b, t.replacement.clone()));
        }
    }
    let spans = merge_spans(spans);
    let (mut new_text, preserve) = apply_flow_spans(&fl, &spans);

    let blank_elems: &[&[u8]] = match name {
        "docProps/core.xml" => CORE_BLANK,
        "docProps/app.xml" => APP_BLANK,
        _ => &[],
    };
    let word_part = is_word_part(name);
    let is_root_rels = name == "_rels/.rels";

    let XmlPart { events, ns } = part;
    let mut out_events: Vec<Event<'static>> = Vec::with_capacity(events.len());
    let mut stack: Vec<(Vec<u8>, Ns)> = Vec::new();
    let mut skip_end = 0usize; // depth inside a dropped element
    for (i, ev) in events.into_iter().enumerate() {
        if skip_end > 0 {
            match &ev {
                Event::Start(_) => skip_end += 1,
                Event::End(_) => skip_end -= 1,
                _ => {}
            }
            continue;
        }
        let ev = match ev {
            Event::Start(e) => {
                let ext = is_external(&e);
                let extra = preserve.contains(&i).then_some(("xml:space", "preserve"));
                let ne = rebuild_start(&e, |k, v| attr_rewrite(k, v, targets, ext), extra)?;
                let nm = e.name();
                stack.push((local(nm.as_ref()).to_vec(), ns[i]));
                Event::Start(ne)
            }
            Event::Empty(e) => {
                if is_root_rels && is_thumbnail_rel(&e) {
                    continue;
                }
                let ext = is_external(&e);
                Event::Empty(rebuild_start(&e, |k, v| attr_rewrite(k, v, targets, ext), None)?)
            }
            Event::End(e) => {
                stack.pop();
                Event::End(e)
            }
            ev @ (Event::Text(_) | Event::CData(_)) => {
                let parent = stack.last();
                if parent.map_or(false, |(l, _)| blank_elems.contains(&l.as_slice())) {
                    continue; // empty the element
                }
                if let Some(t) = new_text.remove(&i) {
                    Event::Text(BytesText::new(&t).into_owned())
                } else if fl.in_flow[i] {
                    ev
                } else {
                    let node_ok = match parent {
                        Some((l, c)) if word_part || *c != Ns::Other => {
                            WORD_TEXT_NODES.contains(&l.as_slice())
                        }
                        _ => true,
                    };
                    let s = event_text(&ev).unwrap_or_default();
                    match (node_ok, replace_in_string(&s, targets)) {
                        (true, Some(r)) => Event::Text(BytesText::new(&r).into_owned()),
                        _ => ev,
                    }
                }
            }
            other => other,
        };
        out_events.push(ev);
    }

    let mut w = Writer::new(Vec::new());
    for ev in out_events {
        w.write_event(ev).map_err(|e| e.to_string())?;
    }
    Ok(w.into_inner())
}

// ─── Verification ────────────────────────────────────────────────────────────

/// Everything in a part that a reader of the file could see or extract.
fn visible_text(part: &XmlPart) -> Vec<String> {
    let fl = flow(part);
    let mut chunks = vec![fl.text.clone()];
    let mut nodes = String::new();
    let mut attrs = String::new();
    for (i, ev) in part.events.iter().enumerate() {
        match ev {
            Event::Text(_) | Event::CData(_) if !fl.in_flow[i] => {
                nodes.push_str(&event_text(ev).unwrap_or_default());
                nodes.push('\n');
            }
            Event::Start(e) | Event::Empty(e) => {
                let ext = is_external(e);
                for a in e.attributes().with_checks(false).flatten() {
                    let key = a.key.as_ref();
                    let l = local(key);
                    if BLANK_ATTRS.contains(&l) || TEXT_ATTRS.contains(&l) || (key == b"Target" && ext) {
                        let v = a.unescape_value().map(|c| c.into_owned()).unwrap_or_default();
                        attrs.push_str(&v);
                        attrs.push('\n');
                        if key == b"Target" {
                            attrs.push_str(&percent_decode(&v));
                            attrs.push('\n');
                        }
                    }
                }
            }
            _ => {}
        }
    }
    chunks.push(nodes);
    chunks.push(attrs);
    chunks
}

fn verify_docx(path: &Path, targets: &[Target]) -> Result<(), String> {
    let file = std::fs::File::open(path).map_err(|e| format!("Export failed: cannot reopen output: {}", e))?;
    let mut archive =
        zip::ZipArchive::new(file).map_err(|e| format!("Export failed: output is not a valid DOCX: {}", e))?;
    let mut leaks: Vec<String> = Vec::new();
    let mut leftovers: Vec<String> = Vec::new();
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| format!("Export failed: {}", e))?;
        let name = entry.name().to_string();
        let mut buf = Vec::new();
        entry.read_to_end(&mut buf).map_err(|e| format!("Export failed: {}", e))?;
        if is_thumbnail(&name) {
            leftovers.push(name.clone());
        }
        let chunks = if is_xml_part(&name) {
            let xml = String::from_utf8(buf).map_err(|_| format!("Export failed: {} is not UTF-8.", name))?;
            let part = parse_part(&xml).map_err(|e| format!("Export failed: {} is not well-formed: {}", name, e))?;
            for (ev, class) in part.events.iter().zip(&part.ns) {
                if let Event::Start(e) | Event::Empty(e) = ev {
                    let nm = e.name();
                    let l = local(nm.as_ref());
                    if *class == Ns::W && matches!(l, b"del" | b"delText" | b"delInstrText" | b"ins" | b"moveFrom") {
                        leftovers.push(name.clone());
                    }
                }
            }
            visible_text(&part)
        } else {
            vec![String::from_utf8_lossy(&buf).into_owned()]
        };
        let hit = chunks.iter().any(|c| {
            let f = Folded::new(c);
            targets.iter().any(|t| f.count(&t.needle) > 0)
        });
        if hit {
            leaks.push(name);
        }
    }
    if !leftovers.is_empty() {
        leftovers.dedup();
        return Err(format!(
            "Export failed: internal check found tracked changes or a thumbnail left in {}. Nothing was exported.",
            leftovers.join(", ")
        ));
    }
    if !leaks.is_empty() {
        log::error!("DOCX export verification: approved text survives in {}", leaks.join(", "));
        return Err(format!(
            "Export refused: approved text is still present in {}. Nothing was exported.",
            leaks.join(", ")
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn targets(list: &[(&str, &str)]) -> Vec<Target> {
        list.iter()
            .map(|(s, r)| Target { needle: Needle::new(s).unwrap(), replacement: r.to_string() })
            .collect()
    }

    const W: &str = r#"xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main""#;

    #[test]
    fn rewrites_split_runs_keeping_first_run_formatting() {
        let xml = format!(
            r#"<w:document {W}><w:p><w:r><w:t xml:space="preserve">Client </w:t></w:r><w:r><w:rPr><w:i/></w:rPr><w:t>Jo</w:t></w:r><w:r><w:rPr><w:b/></w:rPr><w:t xml:space="preserve">hn Smith, hi</w:t></w:r></w:p></w:document>"#
        );
        let out = String::from_utf8(redact_part("word/document.xml", &xml, &targets(&[("John Smith", "[NAME]")]), &[]).unwrap()).unwrap();
        assert!(out.contains(r#"<w:rPr><w:i/></w:rPr><w:t xml:space="preserve">[NAME]</w:t>"#), "{}", out);
        assert!(out.contains(r#"<w:rPr><w:b/></w:rPr><w:t xml:space="preserve">, hi</w:t>"#), "{}", out);
        assert!(!out.contains("Jo<") && !out.contains("Smith"));
    }

    #[test]
    fn attribute_text_is_escaped_not_corrupted() {
        let xml = format!(r#"<w:p {W}><w:r><w:t>A &amp; B &lt;x&gt;</w:t></w:r></w:p>"#);
        let out = String::from_utf8(redact_part("word/document.xml", &xml, &targets(&[("B", "<&>")]), &[]).unwrap()).unwrap();
        assert!(out.contains("A &amp; &lt;&amp;&gt; &lt;x&gt;"), "{}", out);
        parse_part(&out).unwrap();
    }

    #[test]
    fn percent_decoding() {
        assert_eq!(percent_decode("mailto:a%40b.com"), "mailto:a@b.com");
        assert_eq!(percent_decode("100%"), "100%");
    }
}
