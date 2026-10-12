//! Which PDF pages need OCR.
//!
//! A page needs OCR when either
//! - its text layer is empty or near-empty: fewer than [`MIN_TEXT_CHARS`]
//!   visible (non-whitespace) characters. That covers scans, photos, and
//!   text drawn as vector outlines; a page number or a stamp added by a
//!   scanner ("Page 1 of 3") stays under the threshold; or
//! - it is mostly image: images cover at least [`MOSTLY_IMAGE`] of the page
//!   area. Scanners that add their own OCR text layer produce such pages, and
//!   that layer cannot be trusted to contain everything visible.
//!
//! Text counts come from both extractors (the backend's own and PDF.js in the
//! frontend); the smaller count wins, so a page one of them cannot read is
//! OCR'd rather than trusted.

use lopdf::content::Content;
use lopdf::{Dictionary, Document, Object, ObjectId};
use serde::Serialize;

use crate::document::pdf_redact::{as_rect, inherited};
use crate::document::pdf_text::extract_page_text;

/// Fewer visible characters than this = "near-empty" text layer.
pub const MIN_TEXT_CHARS: usize = 32;
/// Image coverage (fraction of the page area) at or above which a page is
/// treated as "mostly image" and OCR'd even if it has a text layer.
pub const MOSTLY_IMAGE: f64 = 0.5;

const MAX_FORM_DEPTH: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum OcrReason {
    NoText,
    LittleText,
    MostlyImage,
}

impl OcrReason {
    pub fn as_str(&self) -> &'static str {
        match self {
            OcrReason::NoText => "no_text",
            OcrReason::LittleText => "little_text",
            OcrReason::MostlyImage => "mostly_image",
        }
    }
}

/// The decision rule, kept separate so the thresholds are tested directly.
pub fn classify(text_chars: usize, image_coverage: f64) -> Option<OcrReason> {
    if text_chars == 0 {
        Some(OcrReason::NoText)
    } else if text_chars < MIN_TEXT_CHARS {
        Some(OcrReason::LittleText)
    } else if image_coverage >= MOSTLY_IMAGE {
        Some(OcrReason::MostlyImage)
    } else {
        None
    }
}

/// Visible characters: anything that is not whitespace or a control/format
/// character (zero-width spaces and soft hyphens do not count).
pub fn visible_chars(s: &str) -> usize {
    s.chars()
        .filter(|c| !c.is_whitespace() && !c.is_control() && !matches!(c, '\u{00AD}' | '\u{200B}'..='\u{200D}' | '\u{2060}' | '\u{FEFF}'))
        .count()
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PageClass {
    /// 0-based.
    pub page: u32,
    pub text_chars: usize,
    pub image_coverage: f64,
    pub needs_ocr: bool,
    pub reason: Option<OcrReason>,
}

/// Classify every page of a PDF. `frontend_text_chars[i]`, when given, is the
/// visible-character count PDF.js found on page i.
pub fn classify_pdf(doc: &Document, frontend_text_chars: &[usize]) -> Vec<PageClass> {
    doc.get_pages()
        .values()
        .enumerate()
        .map(|(i, &page_id)| {
            let backend = visible_chars(&extract_page_text(doc, page_id));
            let text_chars = frontend_text_chars.get(i).map_or(backend, |&f| f.min(backend));
            let image_coverage = image_coverage(doc, page_id);
            let reason = classify(text_chars, image_coverage);
            PageClass { page: i as u32, text_chars, image_coverage, needs_ocr: reason.is_some(), reason }
        })
        .collect()
}

// ─── Image coverage ──────────────────────────────────────────────────────────

type Matrix = [f64; 6];
const IDENTITY: Matrix = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];

/// m × n (PDF row-vector convention: a point is transformed by m, then n).
fn mul(m: &Matrix, n: &Matrix) -> Matrix {
    [
        m[0] * n[0] + m[1] * n[2],
        m[0] * n[1] + m[1] * n[3],
        m[2] * n[0] + m[3] * n[2],
        m[2] * n[1] + m[3] * n[3],
        m[4] * n[0] + m[5] * n[2] + n[4],
        m[4] * n[1] + m[5] * n[3] + n[5],
    ]
}

fn num(o: &Object) -> Option<f64> {
    match o {
        Object::Integer(i) => Some(*i as f64),
        Object::Real(r) => Some(*r as f64),
        _ => None,
    }
}

fn matrix_of(ops: &[Object]) -> Option<Matrix> {
    if ops.len() != 6 {
        return None;
    }
    let mut m = [0.0; 6];
    for (i, o) in ops.iter().enumerate() {
        m[i] = num(o)?;
    }
    Some(m)
}

/// Bounding box of the unit square under `m`.
fn unit_square_bbox(m: &Matrix) -> [f64; 4] {
    let pts = [(0.0, 0.0), (1.0, 0.0), (0.0, 1.0), (1.0, 1.0)];
    let mut b = [f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY];
    for (x, y) in pts {
        let px = m[0] * x + m[2] * y + m[4];
        let py = m[1] * x + m[3] * y + m[5];
        b[0] = b[0].min(px);
        b[1] = b[1].min(py);
        b[2] = b[2].max(px);
        b[3] = b[3].max(py);
    }
    b
}

fn deref<'a>(doc: &'a Document, o: &'a Object) -> &'a Object {
    match o {
        Object::Reference(id) => doc.get_object(*id).unwrap_or(o),
        _ => o,
    }
}

/// Replace inline images (`BI … ID <binary> EI`) with a marker operation, so
/// the content parser does not trip over the binary data and the walker can
/// still count the image.
pub(crate) fn strip_inline_images(content: &[u8]) -> Vec<u8> {
    let is_ws = |b: u8| matches!(b, b' ' | b'\n' | b'\r' | b'\t' | b'\x0c' | b'\0');
    let token_at = |i: usize, t: &[u8; 2]| {
        content.len() >= i + 2
            && &content[i..i + 2] == t
            && (i == 0 || is_ws(content[i - 1]))
            && (i + 2 == content.len() || is_ws(content[i + 2]))
    };
    let mut out = Vec::with_capacity(content.len());
    let mut i = 0;
    while i < content.len() {
        if token_at(i, b"BI") {
            // Find ID (end of the image dictionary), then EI after the data.
            let mut j = i + 2;
            while j < content.len() && !token_at(j, b"ID") {
                j += 1;
            }
            let mut k = j + 3;
            while k < content.len() && !token_at(k, b"EI") {
                k += 1;
            }
            if j >= content.len() || k >= content.len() {
                out.extend_from_slice(&content[i..]);
                break;
            }
            out.extend_from_slice(b" /QvInlineImage Do ");
            i = k + 2;
            continue;
        }
        out.push(content[i]);
        i += 1;
    }
    out
}

struct Walker<'a> {
    doc: &'a Document,
    clip: [f64; 4],
    covered: f64,
}

impl<'a> Walker<'a> {
    fn add(&mut self, ctm: &Matrix) {
        let b = unit_square_bbox(ctm);
        let w = (b[2].min(self.clip[2]) - b[0].max(self.clip[0])).max(0.0);
        let h = (b[3].min(self.clip[3]) - b[1].max(self.clip[1])).max(0.0);
        self.covered += w * h;
    }

    fn lookup_xobject(&self, resources: &[&'a Dictionary], name: &[u8]) -> Option<&'a Object> {
        for res in resources {
            let Ok(xo) = res.get(b"XObject") else { continue };
            let Ok(xo) = deref(self.doc, xo).as_dict() else { continue };
            if let Ok(entry) = xo.get(name) {
                return Some(deref(self.doc, entry));
            }
        }
        None
    }

    fn walk(&mut self, content: &[u8], resources: &[&'a Dictionary], base: Matrix, depth: usize) {
        let cleaned = strip_inline_images(content);
        let Ok(ops) = Content::decode(&cleaned) else { return };
        let mut ctm = base;
        let mut stack: Vec<Matrix> = Vec::new();
        for op in &ops.operations {
            match op.operator.as_str() {
                "q" => stack.push(ctm),
                "Q" => ctm = stack.pop().unwrap_or(base),
                "cm" => {
                    if let Some(m) = matrix_of(&op.operands) {
                        ctm = mul(&m, &ctm);
                    }
                }
                "Do" => {
                    let Some(name) = op.operands.first().and_then(|o| o.as_name().ok()) else { continue };
                    if name == b"QvInlineImage" {
                        self.add(&ctm);
                        continue;
                    }
                    let Some(Object::Stream(s)) = self.lookup_xobject(resources, name) else { continue };
                    match s.dict.get(b"Subtype").and_then(Object::as_name).ok() {
                        Some(b"Image") => self.add(&ctm),
                        Some(b"Form") if depth < MAX_FORM_DEPTH => {
                            let m = s.dict.get(b"Matrix").ok().and_then(|o| o.as_array().ok()).and_then(|a| matrix_of(a)).unwrap_or(IDENTITY);
                            let own = s.dict.get(b"Resources").ok().and_then(|o| deref(self.doc, o).as_dict().ok());
                            let mut chain: Vec<&'a Dictionary> = Vec::new();
                            if let Some(d) = own {
                                chain.push(d);
                            }
                            chain.extend_from_slice(resources);
                            let data = s.decompressed_content().unwrap_or_else(|_| s.content.clone());
                            self.walk(&data, &chain, mul(&m, &ctm), depth + 1);
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        }
    }
}

/// Fraction (0..=1) of the page's visible area covered by images, counting
/// image XObjects (also inside forms) and inline images. Overlapping images
/// are summed, so the value is an upper bound; it is capped at 1.
pub fn image_coverage(doc: &Document, page_id: ObjectId) -> f64 {
    let media = inherited(doc, page_id, b"MediaBox").and_then(|o| as_rect(doc, o)).unwrap_or([0.0, 0.0, 612.0, 792.0]);
    let clip = match inherited(doc, page_id, b"CropBox").and_then(|o| as_rect(doc, o)) {
        Some(c) => {
            let i = [c[0].max(media[0]), c[1].max(media[1]), c[2].min(media[2]), c[3].min(media[3])];
            if i[2] > i[0] && i[3] > i[1] { i } else { media }
        }
        None => media,
    };
    let area = (clip[2] - clip[0]) * (clip[3] - clip[1]);
    if area <= 0.0 {
        return 0.0;
    }
    let Ok(content) = doc.get_page_content(page_id) else { return 0.0 };
    let (inline, ids) = doc.get_page_resources(page_id);
    let mut chain: Vec<&Dictionary> = Vec::new();
    if let Some(d) = inline {
        chain.push(d);
    }
    for id in ids {
        if let Ok(d) = doc.get_dictionary(id) {
            chain.push(d);
        }
    }
    let mut w = Walker { doc, clip, covered: 0.0 };
    w.walk(&content, &chain, IDENTITY, 0);
    (w.covered / area).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::{dictionary, Stream};

    #[test]
    fn thresholds() {
        assert_eq!(classify(0, 0.0), Some(OcrReason::NoText));
        assert_eq!(classify(0, 1.0), Some(OcrReason::NoText));
        assert_eq!(classify(MIN_TEXT_CHARS - 1, 0.0), Some(OcrReason::LittleText));
        assert_eq!(classify(MIN_TEXT_CHARS, 0.0), None);
        assert_eq!(classify(2000, MOSTLY_IMAGE - 0.01), None);
        assert_eq!(classify(2000, MOSTLY_IMAGE), Some(OcrReason::MostlyImage));
        assert_eq!(visible_chars(" Page 1 of 3 \n\u{200B}\u{00AD}"), 8);
    }

    /// A one-page PDF whose content is `ops`, with an image XObject /Im0
    /// and a form /Fm0 that draws /Im0 into its unit square.
    fn pdf_with(ops: &str, text: bool) -> Document {
        let mut doc = Document::with_version("1.5");
        let pages_id = doc.new_object_id();
        let font_id = doc.add_object(dictionary! {"Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica"});
        let img = doc.add_object(Stream::new(
            dictionary! {"Type" => "XObject", "Subtype" => "Image", "Width" => 1, "Height" => 1,
                "ColorSpace" => "DeviceGray", "BitsPerComponent" => 8},
            vec![0],
        ));
        let form = doc.add_object(Stream::new(
            dictionary! {"Type" => "XObject", "Subtype" => "Form", "BBox" => vec![0.into(), 0.into(), 1.into(), 1.into()],
                "Resources" => dictionary! {"XObject" => dictionary! {"Im0" => img}}},
            b"/Im0 Do".to_vec(),
        ));
        let mut body = ops.to_string();
        if text {
            body.push_str(" BT /F1 12 Tf 72 700 Td (This page has a perfectly ordinary text layer on it.) Tj ET");
        }
        let content = doc.add_object(Stream::new(dictionary! {}, body.into_bytes()));
        let page = doc.add_object(dictionary! {
            "Type" => "Page", "Parent" => pages_id, "MediaBox" => vec![0.into(), 0.into(), 600.into(), 800.into()],
            "Contents" => content,
            "Resources" => dictionary! {"Font" => dictionary! {"F1" => font_id}, "XObject" => dictionary! {"Im0" => img, "Fm0" => form}},
        });
        doc.objects.insert(pages_id, Object::Dictionary(dictionary! {"Type" => "Pages", "Kids" => vec![page.into()], "Count" => 1}));
        let catalog = doc.add_object(dictionary! {"Type" => "Catalog", "Pages" => pages_id});
        doc.trailer.set("Root", catalog);
        doc
    }

    fn first_page(doc: &Document) -> ObjectId {
        *doc.get_pages().values().next().unwrap()
    }

    #[test]
    fn coverage_of_full_page_scan() {
        let doc = pdf_with("q 600 0 0 800 0 0 cm /Im0 Do Q", false);
        let c = image_coverage(&doc, first_page(&doc));
        assert!((c - 1.0).abs() < 1e-9, "{}", c);
        let classes = classify_pdf(&doc, &[]);
        assert_eq!(classes[0].reason, Some(OcrReason::NoText));
        assert!(classes[0].needs_ocr);
    }

    #[test]
    fn coverage_tracks_q_cm_forms_and_inline_images() {
        // Quarter page via nested cm, restored by Q; then a form scaled to a
        // quarter page; then an inline image over another quarter.
        let ops = "q 2 0 0 2 0 0 cm q 150 0 0 200 0 0 cm /Im0 Do Q Q \
                   q 300 0 0 400 300 0 cm /Fm0 Do Q \
                   q 300 0 0 400 0 400 cm BI /W 1 /H 1 /CS /G /BPC 8 ID \x00\x01 EI Q \
                   /Im0 Do";
        let doc = pdf_with(ops, true);
        let c = image_coverage(&doc, first_page(&doc));
        // 3 quarters + a 1x1 pt image at the origin.
        assert!((c - (0.75 + 1.0 / (600.0 * 800.0))).abs() < 1e-6, "{}", c);
        let classes = classify_pdf(&doc, &[]);
        assert_eq!(classes[0].reason, Some(OcrReason::MostlyImage));
    }

    #[test]
    fn text_pages_with_small_images_are_left_alone() {
        let doc = pdf_with("q 100 0 0 50 20 720 cm /Im0 Do Q", true);
        let classes = classify_pdf(&doc, &[]);
        assert!(!classes[0].needs_ocr, "{:?}", classes[0]);
        assert!(classes[0].text_chars >= MIN_TEXT_CHARS);
        // PDF.js seeing no text (e.g. a font it cannot decode) wins.
        let classes = classify_pdf(&doc, &[3]);
        assert_eq!(classes[0].reason, Some(OcrReason::LittleText));
    }

    #[test]
    fn images_off_the_page_do_not_count() {
        let doc = pdf_with("q 600 0 0 800 1000 1000 cm /Im0 Do Q", true);
        assert_eq!(image_coverage(&doc, first_page(&doc)), 0.0);
    }

    #[test]
    fn inline_image_stripping_keeps_other_operators() {
        let s = strip_inline_images(b"q 1 0 0 1 0 0 cm BI /W 2 ID xxEIyy EI Q");
        assert_eq!(String::from_utf8_lossy(&s), "q 1 0 0 1 0 0 cm  /QvInlineImage Do  Q");
    }
}
