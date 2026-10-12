//! Regression gate for redacted exports: no approved string may survive.
//!
//! Replaces the throwaway `redaction_leak_audit.rs` from the Phase 0 audit
//! (docs/qvac-integration.md §2). Self-contained: no poppler, no PDF.js.
//! Run: cargo test --test redaction_export_test
extern crate app_lib;

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use app_lib::document::docx_parser::scan_docx_bytes;
use app_lib::document::pdf_text::{extract_page_text, extract_page_texts};
use app_lib::document::redactor::{
    export_flattened_pdf, export_redacted_docx, PageRaster, PixelRect, RedactionMode,
};
use app_lib::document::text_match::{Folded, Needle};
use app_lib::storage::database::RedactionRow;
use lopdf::{dictionary, Document, Object, ObjectId, Stream};

const JURISDICTIONS: [&str; 5] = ["global", "us", "eu", "br", "de"];

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")
}

fn out_dir(test: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("qvault-export-test-{}-{}", std::process::id(), test));
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn row(id: &str, page: i32, start: usize, end: usize, text: &str, category: &str, replacement: &str) -> RedactionRow {
    RedactionRow {
        id: id.into(),
        document_id: "doc".into(),
        page,
        start_offset: start as i32,
        end_offset: end as i32,
        original_text: text.into(),
        replacement: replacement.into(),
        category: category.into(),
        detection_layer: "regex".into(),
        confidence: 1.0,
        status: "approved".into(),
        reviewed_at: None,
    }
}

// ─── PDF helpers ─────────────────────────────────────────────────────────────

/// Layers 1–2 on every page, every detection approved.
fn detect_pdf(doc: &Document) -> Vec<RedactionRow> {
    let mut rows = Vec::new();
    for (p, text) in extract_page_texts(doc).iter().enumerate() {
        for e in app_lib::pipeline::run_pipeline_no_llm(text, p as u32, &JURISDICTIONS) {
            rows.push(row(&e.id, p as i32, e.start, e.end, &e.text, &e.category.to_string(), &e.replacement));
        }
    }
    rows
}

fn white_png(w: u32, h: u32) -> Vec<u8> {
    let img = image::RgbImage::from_pixel(w, h, image::Rgb([255, 255, 255]));
    let mut out = std::io::Cursor::new(Vec::new());
    img.write_to(&mut out, image::ImageFormat::Png).unwrap();
    out.into_inner()
}

/// Stand-in for the frontend: for every page carrying an approved redaction,
/// a blank page image and one rect per occurrence of every approved string on
/// that page (the leak check is about the text layer, not pixel placement).
fn build_rasters(doc: &Document, rows: &[RedactionRow], skip_ids: &[&str], occurrence_cap: Option<usize>) -> Vec<PageRaster> {
    let texts = extract_page_texts(doc);
    let mut by_key: BTreeMap<String, (Needle, &RedactionRow)> = BTreeMap::new();
    for r in rows.iter().filter(|r| r.status == "approved" && !skip_ids.contains(&r.id.as_str())) {
        if let Some(n) = Needle::new(&r.original_text) {
            by_key.entry(n.key()).or_insert((n, r));
        }
    }
    let pages: std::collections::BTreeSet<u32> = rows.iter().map(|r| r.page as u32).collect();
    let page_ids: Vec<ObjectId> = doc.get_pages().values().copied().collect();
    let mut out = Vec::new();
    for p in pages {
        let media = doc
            .get_dictionary(page_ids[p as usize])
            .unwrap()
            .get(b"MediaBox")
            .ok()
            .and_then(|m| m.as_array().ok().cloned());
        let (w, h) = media
            .map(|m| (m[2].as_float().unwrap_or(612.0), m[3].as_float().unwrap_or(792.0)))
            .unwrap_or((612.0, 792.0));
        let (px_w, px_h) = ((w * 2.0) as u32, (h * 2.0) as u32);
        let folded = Folded::new(&texts[p as usize]);
        let mut rects = Vec::new();
        for (n, r) in by_key.values() {
            let mut count = folded.count(n);
            if let Some(cap) = occurrence_cap {
                count = count.min(cap);
            }
            for occ in 0..count {
                rects.push(PixelRect {
                    redaction_id: r.id.clone(),
                    occurrence: occ as u32,
                    x: 20.0,
                    y: 20.0 + occ as f64 * 30.0,
                    width: 200.0,
                    height: 24.0,
                });
            }
        }
        out.push(PageRaster { page_index: p, image: white_png(px_w, px_h), rects });
    }
    out
}

fn assert_no_survivors(out: &Path, rows: &[RedactionRow]) {
    let doc = Document::load(out).unwrap();
    let ours = extract_page_texts(&doc).join("\n");
    let pages: Vec<u32> = doc.get_pages().keys().copied().collect();
    let theirs = doc.extract_text(&pages).unwrap_or_default();
    let raw = std::fs::read(out).unwrap();
    for r in rows.iter().filter(|r| r.status == "approved") {
        let n = Needle::new(&r.original_text).unwrap();
        assert_eq!(Folded::new(&ours).count(&n), 0, "still extractable: {:?}", r.original_text);
        assert_eq!(Folded::new(&theirs).count(&n), 0, "lopdf still extracts {:?}", r.original_text);
        let needle = r.original_text.as_bytes();
        assert!(!raw.windows(needle.len()).any(|w| w == needle), "raw bytes contain {:?}", r.original_text);
    }
}

fn helvetica_page(doc: &mut Document, text: &str) -> ObjectId {
    let pages_id = doc.catalog().unwrap().get(b"Pages").unwrap().as_reference().unwrap();
    let font = doc.add_object(dictionary! { "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica" });
    let content = format!("BT /F1 12 Tf 72 720 Td ({}) Tj ET", text);
    let content_id = doc.add_object(Stream::new(dictionary! {}, content.into_bytes()));
    let page = doc.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
        "Resources" => dictionary! { "Font" => dictionary! { "F1" => font } },
        "Contents" => content_id,
    });
    let pages = doc.get_object_mut(pages_id).unwrap().as_dict_mut().unwrap();
    pages.get_mut(b"Kids").unwrap().as_array_mut().unwrap().push(Object::Reference(page));
    let count = pages.get(b"Count").unwrap().as_i64().unwrap();
    pages.set("Count", count + 1);
    page
}

/// Hide `secret` in every document-level place the exporter must strip.
fn plant_document_level_pii(doc: &mut Document, secret: &str, untouched_page: ObjectId) {
    let s = || Object::string_literal(secret);
    let info = doc.add_object(dictionary! { "Author" => s(), "Title" => s(), "Subject" => s() });
    doc.trailer.set("Info", info);

    let first_page = *doc.get_pages().values().next().unwrap();
    let outlines = doc.new_object_id();
    let item = doc.add_object(dictionary! {
        "Title" => s(), "Parent" => outlines,
        "Dest" => vec![first_page.into(), "Fit".into()],
    });
    doc.objects.insert(outlines, Object::Dictionary(dictionary! {
        "Type" => "Outlines", "First" => item, "Last" => item, "Count" => 1,
    }));
    let xmp = doc.add_object(Stream::new(
        dictionary! { "Type" => "Metadata", "Subtype" => "XML" },
        format!("<x:xmpmeta><dc:creator>{}</dc:creator></x:xmpmeta>", secret).into_bytes(),
    ));
    let field = doc.add_object(dictionary! { "FT" => "Tx", "T" => Object::string_literal("client"), "V" => s() });
    let embedded = doc.add_object(Stream::new(dictionary! { "Type" => "EmbeddedFile" }, secret.as_bytes().to_vec()));
    let filespec = doc.add_object(dictionary! {
        "Type" => "Filespec", "F" => Object::string_literal("notes.txt"),
        "EF" => dictionary! { "F" => embedded },
    });
    let js = doc.add_object(dictionary! { "S" => "JavaScript", "JS" => Object::string_literal(format!("app.alert('{}')", secret)) });
    let catalog = doc.catalog_mut().unwrap();
    catalog.set("Outlines", outlines);
    catalog.set("Metadata", xmp);
    catalog.set("AcroForm", dictionary! { "Fields" => vec![field.into()] });
    catalog.set("Names", dictionary! {
        "EmbeddedFiles" => dictionary! { "Names" => vec![Object::string_literal("notes.txt"), filespec.into()] },
    });
    catalog.set("OpenAction", js);

    let note = doc.add_object(dictionary! {
        "Type" => "Annot", "Subtype" => "Text",
        "Rect" => vec![10.into(), 10.into(), 30.into(), 30.into()], "Contents" => s(), "T" => s(),
    });
    let link = doc.add_object(dictionary! {
        "Type" => "Annot", "Subtype" => "Link", "Contents" => s(),
        "Rect" => vec![72.into(), 700.into(), 300.into(), 730.into()],
        "A" => dictionary! { "S" => "URI", "URI" => Object::string_literal("https://example.org/") },
    });
    doc.get_object_mut(untouched_page)
        .unwrap()
        .as_dict_mut()
        .unwrap()
        .set("Annots", vec![note.into(), link.into()]);
}

fn save_mem(doc: &mut Document) -> Vec<u8> {
    let mut v = Vec::new();
    doc.save_to(&mut v).unwrap();
    v
}

// ─── PDF tests ───────────────────────────────────────────────────────────────

#[test]
fn pdf_demo_export_leaves_no_approved_text() {
    let bytes = std::fs::read(root().join("landing/demo.pdf")).unwrap();
    let doc = Document::load_mem(&bytes).unwrap();
    let rows = detect_pdf(&doc);
    assert!(rows.len() >= 20, "pipeline found only {} detections on demo.pdf", rows.len());
    let rasters = build_rasters(&doc, &rows, &[], None);
    let out = out_dir("demo").join("demo_redacted.pdf");

    let summary = export_flattened_pdf(&bytes, &rows, &rasters, &out).expect("export");
    assert_eq!(summary.page_count, 3);
    assert_no_survivors(&out, &rows);

    let out_doc = Document::load(&out).unwrap();
    assert!(!out_doc.trailer.has(b"Info"), "Info dict must be stripped");
}

#[test]
fn pdf_export_strips_metadata_and_keeps_untouched_pages() {
    let bytes = std::fs::read(root().join("landing/demo.pdf")).unwrap();
    let mut doc = Document::load_mem(&bytes).unwrap();
    let rows = detect_pdf(&doc);
    let clean_text = "Schedule A intentionally left blank";
    let extra_page = helvetica_page(&mut doc, clean_text);
    let secret = rows.iter().find(|r| r.category == "Email").expect("an email on demo.pdf").original_text.clone();
    plant_document_level_pii(&mut doc, &secret, extra_page);
    let input = save_mem(&mut doc);
    let doc = Document::load_mem(&input).unwrap();

    let rasters = build_rasters(&doc, &rows, &[], None);
    let out = out_dir("strip").join("planted_redacted.pdf");
    let summary = export_flattened_pdf(&input, &rows, &rasters, &out).expect("export");
    assert_eq!(summary.page_count, 4);
    assert_eq!(summary.rasterized_pages, vec![0, 1, 2]);
    assert_no_survivors(&out, &rows);

    let out_doc = Document::load(&out).unwrap();
    // Untouched page keeps its original text layer.
    let last = *out_doc.get_pages().values().last().unwrap();
    let text = extract_page_text(&out_doc, last);
    assert!(text.contains(clean_text), "untouched page lost its text: {:?}", text);
    // Only the (sanitized) link annotation survives on it.
    let annots = out_doc.get_dictionary(last).unwrap().get(b"Annots").unwrap().as_array().unwrap().clone();
    assert_eq!(annots.len(), 1);
    let link = out_doc.get_dictionary(annots[0].as_reference().unwrap()).unwrap();
    assert_eq!(link.get(b"Subtype").unwrap().as_name().unwrap(), b"Link");
    assert!(!link.has(b"Contents"));
    // Document-level carriers are gone.
    let catalog = out_doc.catalog().unwrap();
    for key in [&b"Outlines"[..], b"Metadata", b"AcroForm", b"Names", b"OpenAction"] {
        assert!(!catalog.has(key), "/{} survived", String::from_utf8_lossy(key));
    }
    assert!(!out_doc.trailer.has(b"Info"));
    for obj in out_doc.objects.values() {
        if let Ok(d) = obj.as_dict().or_else(|_| obj.as_stream().map(|s| &s.dict)) {
            let ty = d.get(b"Type").and_then(Object::as_name).unwrap_or(b"");
            assert!(!matches!(ty, b"EmbeddedFile" | b"Filespec" | b"Metadata" | b"Outlines"), "object of type {:?} survived", String::from_utf8_lossy(ty));
        }
    }
    // Rasterized pages carry exactly one image and nothing else.
    for (i, (_, &id)) in out_doc.get_pages().iter().enumerate().take(3) {
        let page = out_doc.get_dictionary(id).unwrap();
        assert!(!page.has(b"Annots"), "page {} kept annotations", i + 1);
        let res = page.get(b"Resources").unwrap().as_dict().unwrap();
        assert!(!res.has(b"Font"), "page {} kept fonts", i + 1);
        assert_eq!(res.get(b"XObject").unwrap().as_dict().unwrap().len(), 1);
    }
}

#[test]
fn pdf_export_refuses_unmapped_redaction() {
    let bytes = std::fs::read(root().join("landing/demo.pdf")).unwrap();
    let doc = Document::load_mem(&bytes).unwrap();
    let rows = detect_pdf(&doc);
    // Pick a string that occurs once, so no other row shares its rects.
    let victim = rows
        .iter()
        .find(|r| rows.iter().filter(|o| o.original_text == r.original_text).count() == 1)
        .unwrap();
    let rasters = build_rasters(&doc, &rows, &[victim.id.as_str()], None);
    let out = out_dir("unmapped").join("unmapped.pdf");
    let err = export_flattened_pdf(&bytes, &rows, &rasters, &out).expect_err("must refuse");
    assert!(err.contains("could not be located"), "{}", err);
    assert!(err.contains(&victim.id), "{}", err);
    assert!(err.contains(&format!("page {}", victim.page + 1)), "{}", err);
    assert!(!err.contains(&victim.original_text), "error must not echo the PII");
    assert!(!out.exists(), "no file may be left behind");
}

#[test]
fn pdf_export_refuses_uncovered_occurrence() {
    let bytes = std::fs::read(root().join("landing/demo.pdf")).unwrap();
    let doc = Document::load_mem(&bytes).unwrap();
    let rows = detect_pdf(&doc);
    let texts = extract_page_texts(&doc);
    // Needs a string that occurs at least twice on one page.
    let repeated = rows.iter().any(|r| {
        Folded::new(&texts[r.page as usize]).count(&Needle::new(&r.original_text).unwrap()) > 1
    });
    assert!(repeated, "demo.pdf should repeat some approved string on a page");
    let rasters = build_rasters(&doc, &rows, &[], Some(1));
    let out = out_dir("shortfall").join("shortfall.pdf");
    let err = export_flattened_pdf(&bytes, &rows, &rasters, &out).expect_err("must refuse");
    assert!(err.contains("more occurrences"), "{}", err);
    assert!(!out.exists());
}

#[test]
fn pdf_export_refuses_text_on_page_without_redaction() {
    let mut doc = Document::with_version("1.5");
    let pages_id = doc.new_object_id();
    doc.objects.insert(pages_id, Object::Dictionary(dictionary! { "Type" => "Pages", "Kids" => Vec::<Object>::new(), "Count" => 0 }));
    let catalog = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    doc.trailer.set("Root", catalog);
    helvetica_page(&mut doc, "Witness: Jane Roe");
    helvetica_page(&mut doc, "Signed by Jane Roe");
    let bytes = save_mem(&mut doc);
    let doc = Document::load_mem(&bytes).unwrap();

    let rows = vec![row("r1", 0, 0, 0, "Jane Roe", "PersonName", "[NAME]")];
    let rasters = build_rasters(&doc, &rows, &[], None);
    let out = out_dir("missed").join("missed.pdf");
    let err = export_flattened_pdf(&bytes, &rows, &rasters, &out).expect_err("must refuse");
    assert!(err.contains("page(s) 2"), "{}", err);
    assert!(err.contains("missed detection"), "{}", err);
    assert!(!err.contains("Jane"));
    assert!(!out.exists());

    // Approving the occurrence on page 2 as well makes the export go through.
    let rows = vec![
        row("r1", 0, 0, 0, "Jane Roe", "PersonName", "[NAME]"),
        row("r2", 1, 0, 0, "Jane Roe", "PersonName", "[NAME]"),
    ];
    let rasters = build_rasters(&doc, &rows, &[], None);
    export_flattened_pdf(&bytes, &rows, &rasters, &out).expect("export");
    assert_no_survivors(&out, &rows);
}

// ─── DOCX ────────────────────────────────────────────────────────────────────

const W: &str = r#"xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main""#;

fn make_docx(path: &Path) {
    let f = std::fs::File::create(path).unwrap();
    let mut z = zip::ZipWriter::new(f);
    let o = zip::write::FileOptions::default();
    let document = format!(
        concat!(
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>"#,
            r#"<w:document {W} xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing"><w:body>"#,
            r#"<w:p><w:r><w:t>Contact: john.smith@example.com</w:t></w:r></w:p>"#,
            // name split across runs with different formatting
            r#"<w:p><w:r><w:t xml:space="preserve">Client </w:t></w:r><w:r><w:t>John</w:t></w:r><w:r><w:rPr><w:b/></w:rPr><w:t xml:space="preserve"> Smith</w:t></w:r><w:r><w:t xml:space="preserve"> signed.</w:t></w:r></w:p>"#,
            // SSN split by proofErr
            r#"<w:p><w:r><w:t>SSN: 123-45-</w:t></w:r><w:proofErr w:type="spellStart"/><w:r><w:t>6789</w:t></w:r></w:p>"#,
            // tracked deletion, tracked insertion with author, formatting change history
            r#"<w:p><w:del w:id="1" w:author="John Smith"><w:r><w:delText>Old phone +1-555-867-5309</w:delText></w:r></w:del><w:ins w:id="2" w:author="John Smith"><w:r><w:rPr><w:rPrChange w:id="3" w:author="John Smith"><w:rPr/></w:rPrChange></w:rPr><w:t>New text</w:t></w:r></w:ins></w:p>"#,
            // hyperlink field with the e-mail in its instruction, and alt text
            r#"<w:p><w:r><w:fldChar w:fldCharType="begin"/></w:r><w:r><w:instrText xml:space="preserve"> HYPERLINK "mailto:john.smith@example.com" </w:instrText></w:r><w:r><w:fldChar w:fldCharType="separate"/></w:r><w:r><w:t>email us</w:t></w:r><w:r><w:fldChar w:fldCharType="end"/></w:r></w:p>"#,
            r#"<w:p><w:hyperlink r:id="rIdLink"><w:r><w:t>site</w:t></w:r></w:hyperlink><w:r><w:drawing><wp:inline><wp:docPr id="1" name="Picture 1" descr="Photo of John Smith"/></wp:inline></w:drawing></w:r></w:p>"#,
            r#"<w:p><w:commentRangeStart w:id="0"/><w:r><w:t>Reviewed.</w:t></w:r><w:commentRangeEnd w:id="0"/></w:p>"#,
            r#"</w:body></w:document>"#
        ),
        W = W
    );
    let files: Vec<(&str, String)> = vec![
        ("[Content_Types].xml", r#"<?xml version="1.0" encoding="UTF-8"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Default Extension="jpeg" ContentType="image/jpeg"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>"#.into()),
        ("_rels/.rels", r#"<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/package/2006/relationships/metadata/thumbnail" Target="docProps/thumbnail.jpeg"/><Relationship Id="rId3" Type="http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties" Target="docProps/core.xml"/><Relationship Id="rId4" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/extended-properties" Target="docProps/app.xml"/></Relationships>"#.into()),
        ("word/_rels/document.xml.rels", r#"<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rIdLink" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink" Target="mailto:john.smith%40example.com" TargetMode="External"/></Relationships>"#.into()),
        ("word/document.xml", document),
        ("word/header1.xml", format!(r#"<w:hdr {W}><w:p><w:r><w:t>Matter of John Smith - john.smith@example.com</w:t></w:r></w:p></w:hdr>"#)),
        ("word/footer1.xml", format!(r#"<w:ftr {W}><w:p><w:r><w:t>SSN 123-45-</w:t></w:r><w:r><w:t>6789</w:t></w:r></w:p></w:ftr>"#)),
        ("word/footnotes.xml", format!(r#"<w:footnotes {W}><w:footnote w:id="1"><w:p><w:r><w:t>Per John Smith, phone +1-555-867-5309</w:t></w:r></w:p></w:footnote></w:footnotes>"#)),
        ("word/endnotes.xml", format!(r#"<w:endnotes {W}><w:endnote w:id="1"><w:p><w:r><w:t>Call +1-555-</w:t></w:r><w:r><w:t>867-5309</w:t></w:r></w:p></w:endnote></w:endnotes>"#)),
        ("word/comments.xml", format!(r#"<w:comments {W}><w:comment w:id="0" w:author="John Smith" w:initials="JS" w:date="2024-01-01T00:00:00Z"><w:p><w:r><w:t>Verify SSN 123-45-6789</w:t></w:r></w:p></w:comment></w:comments>"#)),
        ("word/people.xml", r#"<w15:people xmlns:w15="http://schemas.microsoft.com/office/word/2012/wordml"><w15:person w15:author="John Smith"><w15:presenceInfo w15:providerId="AD" w15:userId="john.smith@example.com"/></w15:person></w15:people>"#.into()),
        ("docProps/core.xml", r#"<?xml version="1.0" encoding="UTF-8"?><cp:coreProperties xmlns:cp="http://schemas.openxmlformats.org/package/2006/metadata/core-properties" xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:dcterms="http://purl.org/dc/terms/"><dc:title>Intake - John Smith</dc:title><dc:subject>Smith matter</dc:subject><dc:creator>John Smith</dc:creator><cp:keywords>john.smith@example.com</cp:keywords><dc:description>Notes on Smith</dc:description><cp:lastModifiedBy>john.smith@example.com</cp:lastModifiedBy><dcterms:created>2024-01-01T00:00:00Z</dcterms:created></cp:coreProperties>"#.into()),
        ("docProps/app.xml", r#"<?xml version="1.0" encoding="UTF-8"?><Properties xmlns="http://schemas.openxmlformats.org/officeDocument/2006/extended-properties"><Application>Microsoft Office Word</Application><Company>Smith Holdings</Company><Manager>John Smith</Manager></Properties>"#.into()),
        ("docProps/thumbnail.jpeg", "not really a jpeg: John Smith".into()),
    ];
    for (n, c) in files {
        z.start_file(n, o).unwrap();
        z.write_all(c.as_bytes()).unwrap();
    }
    z.finish().unwrap();
}

#[test]
fn docx_export_leaves_no_approved_text_anywhere() {
    let dir = out_dir("docx");
    let src = dir.join("synthetic.docx");
    make_docx(&src);
    let bytes = std::fs::read(&src).unwrap();

    // Scan exactly as the app does: parser text → Layers 1–2 → approve all.
    let scan = scan_docx_bytes(&bytes).unwrap();
    for part in ["header1", "footer1", "footnotes", "endnotes", "comments"] {
        assert!(scan.segments.iter().any(|s| s.part.contains(part)), "{} not scanned", part);
    }
    assert!(scan.text.contains("John Smith signed."), "split runs not joined: {:?}", scan.text);
    assert!(scan.text.contains("123-45-6789"), "proofErr split not joined");
    assert!(!scan.text.contains("Old phone"), "deleted text must not be scanned");
    let mut rows: Vec<RedactionRow> = app_lib::pipeline::run_pipeline_no_llm(&scan.text, 0, &JURISDICTIONS)
        .iter()
        .map(|e| row(&e.id, 0, e.start, e.end, &e.text, &e.category.to_string(), &e.replacement))
        .collect();
    // Make sure the canonical strings are approved even if the scanner missed them.
    for (t, c, r) in [
        ("John Smith", "PersonName", "[NAME]"),
        ("john.smith@example.com", "Email", "[EMAIL]"),
        ("123-45-6789", "SocialSecurityNumber", "[SSN]"),
        ("+1-555-867-5309", "PhoneNumber", "[PHONE]"),
    ] {
        if !rows.iter().any(|x| x.original_text == t) {
            let s = scan.text.find(t).unwrap_or(0);
            rows.push(row(&format!("manual-{}", c), 0, s, s + t.len(), t, c, r));
        }
    }
    let out = dir.join("synthetic_redacted.docx");
    export_redacted_docx(&bytes, &rows, &out, RedactionMode::TextReplace).expect("export");

    let mut z = zip::ZipArchive::new(std::fs::File::open(&out).unwrap()).expect("valid zip");
    let names: Vec<String> = z.file_names().map(String::from).collect();
    assert!(!names.iter().any(|n| n.contains("thumbnail")), "thumbnail must be removed");
    let probes = ["John Smith", "Smith", "john.smith", "example.com", "123-45-6789", "6789", "867-5309", "555-", "JS"];
    let mut parts: BTreeMap<String, String> = BTreeMap::new();
    for n in &names {
        let mut s = String::new();
        z.by_name(n).unwrap().read_to_string(&mut s).unwrap();
        // Well-formed XML, end tags checked.
        let mut reader = quick_xml::Reader::from_str(&s);
        reader.check_end_names(true);
        let mut plain = String::new();
        loop {
            match reader.read_event() {
                Ok(quick_xml::events::Event::Eof) => break,
                Ok(quick_xml::events::Event::Text(t)) => plain.push_str(&t.unescape().unwrap()),
                Ok(_) => {}
                Err(e) => panic!("{} is not well-formed: {}", n, e),
            }
        }
        for p in probes {
            assert!(!s.contains(p) && !plain.contains(p), "{:?} survives in {}:\n{}", p, n, s);
        }
        for bad in ["<w:del ", "<w:delText", "<w:ins ", "rPrChange", "presenceInfo"] {
            assert!(!s.contains(bad), "{} survives in {}", bad, n);
        }
        parts.insert(n.clone(), s);
    }

    let doc = &parts["word/document.xml"];
    // The label lands in the first run of the span; the bold tail run is
    // emptied but kept; text after the span is untouched. (Whether "Client"
    // is part of the detected name depends on the heuristics.)
    assert!(doc.contains(r#"<w:rPr><w:b/></w:rPr><w:t xml:space="preserve"></w:t></w:r><w:r><w:t xml:space="preserve"> signed.</w:t>"#), "{}", doc);
    assert!(doc.contains(r#"<w:t xml:space="preserve">SSN: [REDACTED SSN]</w:t></w:r><w:proofErr w:type="spellStart"/><w:r><w:t xml:space="preserve"></w:t>"#), "{}", doc);
    assert!(doc.contains("Contact: ["), "{}", doc);
    assert!(doc.contains("New text"), "accepted insertion must stay: {}", doc);
    assert!(doc.contains("Reviewed."));
    let comments = &parts["word/comments.xml"];
    assert!(comments.contains(r#"w:author="""#) && comments.contains(r#"w:initials="""#), "{}", comments);
    let core = &parts["docProps/core.xml"];
    for tag in ["dc:title", "dc:creator", "cp:lastModifiedBy", "cp:keywords", "dc:subject", "dc:description"] {
        assert!(core.contains(&format!("<{}></{}>", tag, tag)), "{} not blanked: {}", tag, core);
    }
    assert!(core.contains("2024-01-01T00:00:00Z"), "dates are kept");
    let app = &parts["docProps/app.xml"];
    assert!(app.contains("<Company></Company>") && app.contains("<Manager></Manager>"), "{}", app);
    assert!(!parts["_rels/.rels"].contains("thumbnail"));
    assert!(!parts["word/_rels/document.xml.rels"].contains("40example"));

    // Scanning the output finds none of the approved strings.
    let rescan = scan_docx_bytes(&std::fs::read(&out).unwrap()).unwrap();
    for r in &rows {
        let n = Needle::new(&r.original_text).unwrap();
        assert_eq!(Folded::new(&rescan.text).count(&n), 0, "{:?} survives a rescan", r.original_text);
    }
}

#[test]
fn docx_export_refuses_malformed_part() {
    let dir = out_dir("docx-bad");
    let src = dir.join("bad.docx");
    {
        let f = std::fs::File::create(&src).unwrap();
        let mut z = zip::ZipWriter::new(f);
        z.start_file("word/document.xml", zip::write::FileOptions::default()).unwrap();
        z.write_all(format!(r#"<w:document {W}><w:p><w:r><w:t>John Smith</w:t></w:r></w:document>"#).as_bytes()).unwrap();
        z.finish().unwrap();
    }
    let bytes = std::fs::read(&src).unwrap();
    let rows = vec![row("r", 0, 0, 0, "John Smith", "PersonName", "[NAME]")];
    let out = dir.join("bad_redacted.docx");
    assert!(export_redacted_docx(&bytes, &rows, &out, RedactionMode::TextReplace).is_err());
    assert!(!out.exists());
}
