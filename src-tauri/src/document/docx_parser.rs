//! DOCX text model shared by the scanner and the redactor.
//!
//! The scanner and the redactor must see exactly the same text, or the
//! offsets stored with each detection would not map back. Both therefore go
//! through [`parse_part`] → [`clean_tracked_changes`] → [`flow`]:
//!
//! - `parse_part` reads an XML part into owned events with their resolved
//!   namespace (so `w:` works whatever prefix the producer chose);
//! - `clean_tracked_changes` drops deleted/moved-away content and revision
//!   markers and unwraps insertions (i.e. accepts all insertions, rejects
//!   nothing that is still visible);
//! - `flow` concatenates the text of every `t` run element (WordprocessingML,
//!   DrawingML and OMML) in document order, with `\n` after each paragraph and
//!   `\t`/`\n` for tabs and breaks, remembering which event each byte came from.
//!
//! Scanned text = document.xml, then headers, footers, footnotes, endnotes,
//! comments, each followed by one extra `\n`.

use quick_xml::events::Event;
use quick_xml::name::ResolveResult;
use quick_xml::NsReader;
use std::io::Read;
use std::path::Path;

use super::pdf_parser::PageText;

const W_NS: &[&[u8]] = &[
    b"http://schemas.openxmlformats.org/wordprocessingml/2006/main",
    b"http://purl.oclc.org/ooxml/wordprocessingml/main",
];
const A_NS: &[&[u8]] = &[
    b"http://schemas.openxmlformats.org/drawingml/2006/main",
    b"http://purl.oclc.org/ooxml/drawingml/main",
];
const M_NS: &[&[u8]] = &[
    b"http://schemas.openxmlformats.org/officeDocument/2006/math",
    b"http://purl.oclc.org/ooxml/officeDocument/math",
];

/// Namespace class of an element.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Ns {
    /// WordprocessingML
    W,
    /// DrawingML
    A,
    /// Office Math
    M,
    Other,
}

/// An XML part as owned events. `ns[i]` is the namespace class of event `i`
/// when it is a Start/Empty element (`Ns::Other` otherwise).
pub(crate) struct XmlPart {
    pub events: Vec<Event<'static>>,
    pub ns: Vec<Ns>,
}

pub(crate) fn local(name: &[u8]) -> &[u8] {
    match name.iter().rposition(|&b| b == b':') {
        Some(i) => &name[i + 1..],
        None => name,
    }
}

/// Parse a whole part. Fails on malformed XML (including mismatched end tags).
pub(crate) fn parse_part(xml: &str) -> Result<XmlPart, String> {
    let xml = xml.strip_prefix('\u{FEFF}').unwrap_or(xml);
    let mut reader = NsReader::from_str(xml);
    reader.trim_text(false);
    reader.check_end_names(true);
    let mut events = Vec::new();
    let mut ns = Vec::new();
    loop {
        let (res, ev) = match reader.read_resolved_event() {
            Ok(x) => x,
            Err(e) => return Err(format!("XML parse error at byte {}: {}", reader.buffer_position(), e)),
        };
        if let Event::Eof = ev {
            break;
        }
        let class = match (&ev, res) {
            (Event::Start(_) | Event::Empty(_), ResolveResult::Bound(n)) => {
                let n = n.as_ref();
                if W_NS.contains(&n) {
                    Ns::W
                } else if A_NS.contains(&n) {
                    Ns::A
                } else if M_NS.contains(&n) {
                    Ns::M
                } else {
                    Ns::Other
                }
            }
            _ => Ns::Other,
        };
        events.push(ev.into_owned());
        ns.push(class);
    }
    Ok(XmlPart { events, ns })
}

/// WordprocessingML elements removed with everything inside them: deleted and
/// moved-away content, formatting-change history, revision range markers,
/// document variables and mail-merge settings.
const DROP_W: &[&[u8]] = &[
    b"del",
    b"moveFrom",
    b"delText",
    b"delInstrText",
    b"rPrChange",
    b"pPrChange",
    b"sectPrChange",
    b"tblPrChange",
    b"tblPrExChange",
    b"trPrChange",
    b"tcPrChange",
    b"tblGridChange",
    b"numberingChange",
    b"cellDel",
    b"cellIns",
    b"cellMerge",
    b"moveFromRangeStart",
    b"moveFromRangeEnd",
    b"moveToRangeStart",
    b"moveToRangeEnd",
    b"customXmlDelRangeStart",
    b"customXmlDelRangeEnd",
    b"customXmlInsRangeStart",
    b"customXmlInsRangeEnd",
    b"customXmlMoveFromRangeStart",
    b"customXmlMoveFromRangeEnd",
    b"customXmlMoveToRangeStart",
    b"customXmlMoveToRangeEnd",
    b"docVars",
    b"mailMerge",
];
/// Elements removed in any namespace (w15:presenceInfo holds account e-mails).
const DROP_ANY: &[&[u8]] = &[b"presenceInfo"];
/// WordprocessingML wrappers whose content is kept (insertions accepted).
/// As empty elements they are revision markers and are dropped.
const UNWRAP_W: &[&[u8]] = &[b"ins", b"moveTo"];

/// Accept tracked changes: drop deletions and revision metadata, unwrap
/// insertions. XML comments are dropped too.
pub(crate) fn clean_tracked_changes(part: XmlPart) -> XmlPart {
    let mut events = Vec::with_capacity(part.events.len());
    let mut ns = Vec::with_capacity(part.ns.len());
    let mut skip_depth = 0usize;
    // For each open element: true if its End must be dropped.
    let mut open: Vec<bool> = Vec::new();
    for (ev, class) in part.events.into_iter().zip(part.ns) {
        match &ev {
            Event::Start(e) => {
                if skip_depth > 0 {
                    skip_depth += 1;
                    continue;
                }
                let name = e.name();
                let l = local(name.as_ref());
                if (class == Ns::W && DROP_W.contains(&l)) || DROP_ANY.contains(&l) {
                    skip_depth = 1;
                    continue;
                }
                let unwrap = class == Ns::W && UNWRAP_W.contains(&l);
                open.push(unwrap);
                if unwrap {
                    continue;
                }
            }
            Event::Empty(e) => {
                if skip_depth > 0 {
                    continue;
                }
                let name = e.name();
                let l = local(name.as_ref());
                if (class == Ns::W && (DROP_W.contains(&l) || UNWRAP_W.contains(&l))) || DROP_ANY.contains(&l) {
                    continue;
                }
            }
            Event::End(_) => {
                if skip_depth > 0 {
                    skip_depth -= 1;
                    continue;
                }
                if open.pop().unwrap_or(false) {
                    continue;
                }
            }
            Event::Comment(_) => continue,
            _ => {
                if skip_depth > 0 {
                    continue;
                }
            }
        }
        events.push(ev);
        ns.push(class);
    }
    XmlPart { events, ns }
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum PieceKind {
    /// Text of event `event`, inside the `t` element that starts at `t_start`.
    Text { event: usize, t_start: usize },
    /// Paragraph end, tab or break: present in the text, not editable.
    Virtual,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct Piece {
    pub start: usize,
    pub end: usize,
    pub kind: PieceKind,
}

pub(crate) struct Flow {
    pub text: String,
    pub pieces: Vec<Piece>,
    /// `in_flow[i]` is true when event `i` is a text node that is part of `text`.
    pub in_flow: Vec<bool>,
}

pub(crate) fn event_text(ev: &Event) -> Option<String> {
    match ev {
        Event::Text(t) => Some(
            t.unescape()
                .map(|c| c.into_owned())
                .unwrap_or_else(|_| String::from_utf8_lossy(t.as_ref()).into_owned()),
        ),
        Event::CData(c) => Some(String::from_utf8_lossy(c.as_ref()).into_owned()),
        _ => None,
    }
}

/// Build the run-concatenated text of a part. See the module docs.
pub(crate) fn flow(part: &XmlPart) -> Flow {
    let mut text = String::new();
    let mut pieces = Vec::new();
    let mut in_flow = vec![false; part.events.len()];
    // (local name, namespace class) of open elements
    let mut stack: Vec<(Vec<u8>, Ns)> = Vec::new();
    let mut in_t: Option<usize> = None;

    let push_virtual =|text: &mut String, pieces: &mut Vec<Piece>, s: &str| {
        let start = text.len();
        text.push_str(s);
        pieces.push(Piece { start, end: text.len(), kind: PieceKind::Virtual });
    };

    for (i, ev) in part.events.iter().enumerate() {
        let class = part.ns[i];
        let parent_is_run = |stack: &[(Vec<u8>, Ns)]| {
            stack.last().map_or(false, |(n, c)| n.as_slice() == b"r" && *c != Ns::Other)
        };
        match ev {
            Event::Start(e) => {
                let name = e.name();
                let l = local(name.as_ref()).to_vec();
                if l == b"t" && class != Ns::Other && parent_is_run(&stack) {
                    in_t = Some(i);
                }
                stack.push((l, class));
            }
            Event::End(_) => {
                if let Some((l, c)) = stack.pop() {
                    if l == b"t" {
                        in_t = None;
                    }
                    if l == b"p" && matches!(c, Ns::W | Ns::A) {
                        push_virtual(&mut text, &mut pieces, "\n");
                    }
                }
            }
            Event::Empty(e) => {
                let name = e.name();
                let l = local(name.as_ref());
                let run_child = class == Ns::W && parent_is_run(&stack);
                if run_child && (l == b"tab" || l == b"ptab") {
                    push_virtual(&mut text, &mut pieces, "\t");
                } else if (run_child && (l == b"br" || l == b"cr")) || (class == Ns::A && l == b"br") {
                    push_virtual(&mut text, &mut pieces, "\n");
                }
            }
            Event::Text(_) | Event::CData(_) => {
                if let Some(t_start) = in_t {
                    let s = event_text(ev).unwrap_or_default();
                    let start = text.len();
                    text.push_str(&s);
                    pieces.push(Piece { start, end: text.len(), kind: PieceKind::Text { event: i, t_start } });
                    in_flow[i] = true;
                }
            }
            _ => {}
        }
    }
    Flow { text, pieces, in_flow }
}

// ─── Part selection and scanning ─────────────────────────────────────────────

fn numbered(name: &str, prefix: &str) -> Option<u32> {
    let rest = name.strip_prefix(prefix)?.strip_suffix(".xml")?;
    if rest.is_empty() {
        return Some(0);
    }
    rest.parse().ok()
}

/// The parts whose text is scanned, in scan order.
pub fn scanned_parts(names: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    let has = |n: &str| names.iter().any(|x| x == n);
    if has("word/document.xml") {
        out.push("word/document.xml".to_string());
    }
    for prefix in ["word/header", "word/footer"] {
        let mut v: Vec<(u32, &String)> =
            names.iter().filter_map(|n| numbered(n, prefix).map(|k| (k, n))).collect();
        v.sort();
        out.extend(v.into_iter().map(|(_, n)| n.clone()));
    }
    for n in ["word/footnotes.xml", "word/endnotes.xml", "word/comments.xml"] {
        if has(n) {
            out.push(n.to_string());
        }
    }
    out
}

pub(crate) fn read_zip_text<R: std::io::Read + std::io::Seek>(
    archive: &mut zip::ZipArchive<R>,
    name: &str,
) -> Result<String, String> {
    let mut entry = archive.by_name(name).map_err(|e| format!("Failed to open {}: {}", name, e))?;
    let mut buf = Vec::new();
    entry.read_to_end(&mut buf).map_err(|e| format!("Failed to read {}: {}", name, e))?;
    String::from_utf8(buf).map_err(|_| format!("{} is not UTF-8", name))
}

/// Where a scanned part's text sits inside [`DocxScan::text`].
#[derive(Debug, Clone)]
pub struct DocxSegment {
    pub part: String,
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone)]
pub struct DocxScan {
    pub text: String,
    pub segments: Vec<DocxSegment>,
}

/// Extract the scan text of a DOCX given its bytes.
pub fn scan_docx_bytes(bytes: &[u8]) -> Result<DocxScan, String> {
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes))
        .map_err(|e| format!("Failed to read DOCX zip: {}", e))?;
    let names: Vec<String> = archive.file_names().map(|s| s.to_string()).collect();
    if !names.iter().any(|n| n == "word/document.xml") {
        return Err("Failed to find word/document.xml".to_string());
    }
    let mut text = String::new();
    let mut segments = Vec::new();
    for part in scanned_parts(&names) {
        let xml = read_zip_text(&mut archive, &part)?;
        let parsed = clean_tracked_changes(parse_part(&xml).map_err(|e| format!("{}: {}", part, e))?);
        let f = flow(&parsed);
        let start = text.len();
        text.push_str(&f.text);
        segments.push(DocxSegment { part, start, end: text.len() });
        text.push('\n');
    }
    Ok(DocxScan { text, segments })
}

pub fn extract_docx_text(path: &Path) -> Result<Vec<PageText>, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("Failed to open DOCX: {}", e))?;
    let scan = scan_docx_bytes(&bytes)?;
    // DOCX has no pages; everything is page 0.
    Ok(vec![PageText { page_number: 0, text: scan.text }])
}

#[cfg(test)]
mod tests {
    use super::*;

    const W: &str = r#"xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main""#;

    fn text_of(xml: &str) -> String {
        flow(&clean_tracked_changes(parse_part(xml).unwrap())).text
    }

    #[test]
    fn joins_runs_and_drops_deletions() {
        let xml = format!(
            r#"<w:document {W}><w:body><w:p><w:r><w:t>Jo</w:t></w:r><w:proofErr w:type="spellStart"/><w:ins w:author="A"><w:r><w:t>hn</w:t></w:r></w:ins><w:del w:author="B"><w:r><w:delText>X</w:delText></w:r></w:del><w:r><w:tab/><w:t xml:space="preserve"> Smith</w:t></w:r></w:p></w:body></w:document>"#
        );
        assert_eq!(text_of(&xml), "John\t Smith\n");
    }

    #[test]
    fn any_prefix_is_resolved() {
        let xml = r#"<x:document xmlns:x="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><x:p><x:r><x:t>A&amp;B</x:t></x:r></x:p></x:document>"#;
        assert_eq!(text_of(xml), "A&B\n");
    }

    #[test]
    fn tab_stops_are_not_tabs() {
        let xml = format!(r#"<w:p {W}><w:pPr><w:tabs><w:tab w:val="left" w:pos="720"/></w:tabs></w:pPr><w:r><w:t>a</w:t></w:r></w:p>"#);
        assert_eq!(text_of(&xml), "a\n");
    }

    #[test]
    fn part_order() {
        let names: Vec<String> = ["word/comments.xml", "word/header10.xml", "word/footer1.xml", "word/header2.xml", "word/document.xml", "word/footnotes.xml"]
            .iter().map(|s| s.to_string()).collect();
        assert_eq!(
            scanned_parts(&names),
            vec!["word/document.xml", "word/header2.xml", "word/header10.xml", "word/footer1.xml", "word/footnotes.xml", "word/comments.xml"]
        );
    }
}
