//! Standalone images (PNG, JPEG, TIFF incl. multi-page) → image-only PDF.
//!
//! Images are converted once, at upload, so the rest of Qvault (viewer,
//! classification, OCR, export with burn-in, verification) handles them as
//! the scanned PDFs they effectively are. The PDF holds exactly one image per
//! page and nothing else: no text, no fonts, no metadata.
//!
//! Page size comes from the image's resolution metadata when it is plausible
//! (100–1200 DPI); otherwise the longer side is taken to be 11 in (a Letter /
//! A4 page), which is right for scans and phone photos of documents.
//!
//! JPEGs with 1 or 3 components and no EXIF rotation are embedded unchanged
//! (no re-compression); everything else is decoded and stored losslessly
//! (Flate), as 1-bit when the image is pure black and white.

use std::io::Cursor;

use image::{DynamicImage, GrayImage, ImageDecoder, RgbImage};
use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Document, Object, Stream};

/// Refuse images larger than this many pixels per page (≈ A3 at 600 DPI).
pub const MAX_PIXELS: u64 = 80_000_000;
pub const MAX_PAGES: usize = 500;
const FALLBACK_LONG_SIDE_IN: f64 = 11.0;
/// PDF implementation limit for page dimensions.
const MAX_PAGE_PT: f64 = 14_400.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageKind {
    Png,
    Jpeg,
    Tiff,
}

impl ImageKind {
    pub fn from_extension(ext: &str) -> Option<Self> {
        match ext.to_ascii_lowercase().as_str() {
            "png" => Some(ImageKind::Png),
            "jpg" | "jpeg" => Some(ImageKind::Jpeg),
            "tif" | "tiff" => Some(ImageKind::Tiff),
            _ => None,
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            ImageKind::Png => "PNG",
            ImageKind::Jpeg => "JPEG",
            ImageKind::Tiff => "TIFF",
        }
    }
}

enum Pixels {
    /// JPEG bytes to embed as-is, with its number of components.
    Jpeg(Vec<u8>, u8),
    Gray(GrayImage),
    Rgb(RgbImage),
}

struct PageImage {
    width: u32,
    height: u32,
    /// Horizontal / vertical resolution from metadata, if any.
    dpi: Option<(f64, f64)>,
    pixels: Pixels,
}

#[derive(Debug, Clone)]
pub struct ImportedPdf {
    pub pdf: Vec<u8>,
    pub pages: usize,
}

/// Convert image bytes to an image-only PDF.
pub fn image_to_pdf(bytes: &[u8], kind: ImageKind) -> Result<ImportedPdf, String> {
    let pages = match kind {
        ImageKind::Png => vec![decode_png(bytes)?],
        ImageKind::Jpeg => vec![decode_jpeg(bytes)?],
        ImageKind::Tiff => decode_tiff(bytes)?,
    };
    if pages.is_empty() {
        return Err("The image contains no pages.".into());
    }
    let n = pages.len();
    Ok(ImportedPdf { pdf: build_pdf(pages)?, pages: n })
}

fn check_size(w: u32, h: u32) -> Result<(), String> {
    if w == 0 || h == 0 {
        return Err("The image is empty.".into());
    }
    if w as u64 * h as u64 > MAX_PIXELS {
        return Err(format!("The image is too large ({}×{} pixels).", w, h));
    }
    Ok(())
}

fn plausible(dpi: f64) -> bool {
    (100.0..=1200.0).contains(&dpi)
}

/// Flatten any image onto white and keep it grey if it has no colour.
fn to_pixels(img: DynamicImage) -> Pixels {
    let has_alpha = img.color().has_alpha();
    let rgba = img.to_rgba8();
    let blend = |c: u8, a: u8| ((c as u32 * a as u32 + 255 * (255 - a as u32)) / 255) as u8;
    let (w, h) = rgba.dimensions();
    let gray = rgba.pixels().all(|p| p[0] == p[1] && p[1] == p[2]);
    if gray {
        let mut g = GrayImage::new(w, h);
        for (x, y, p) in rgba.enumerate_pixels() {
            let v = if has_alpha { blend(p[0], p[3]) } else { p[0] };
            g.put_pixel(x, y, image::Luma([v]));
        }
        Pixels::Gray(g)
    } else {
        let mut rgb = RgbImage::new(w, h);
        for (x, y, p) in rgba.enumerate_pixels() {
            rgb.put_pixel(x, y, image::Rgb([blend(p[0], p[3]), blend(p[1], p[3]), blend(p[2], p[3])]));
        }
        Pixels::Rgb(rgb)
    }
}

// ─── PNG ─────────────────────────────────────────────────────────────────────

/// Resolution from the pHYs chunk (unit = metre).
fn png_dpi(bytes: &[u8]) -> Option<(f64, f64)> {
    let mut i = 8;
    while i + 8 <= bytes.len() {
        let len = u32::from_be_bytes(bytes[i..i + 4].try_into().ok()?) as usize;
        let ty = &bytes[i + 4..i + 8];
        if ty == b"IDAT" || ty == b"IEND" {
            return None;
        }
        if ty == b"pHYs" && len == 9 && i + 8 + 9 <= bytes.len() {
            let d = &bytes[i + 8..i + 17];
            let x = u32::from_be_bytes(d[0..4].try_into().ok()?) as f64;
            let y = u32::from_be_bytes(d[4..8].try_into().ok()?) as f64;
            return (d[8] == 1).then_some((x * 0.0254, y * 0.0254));
        }
        i += 12 + len;
    }
    None
}

fn decode_png(bytes: &[u8]) -> Result<PageImage, String> {
    let img = image::load_from_memory_with_format(bytes, image::ImageFormat::Png)
        .map_err(|e| format!("Could not read the PNG image: {}", e))?;
    check_size(img.width(), img.height())?;
    Ok(PageImage { width: img.width(), height: img.height(), dpi: png_dpi(bytes), pixels: to_pixels(img) })
}

// ─── JPEG ────────────────────────────────────────────────────────────────────

/// (JFIF density in DPI, number of components from the SOF header).
fn jpeg_info(bytes: &[u8]) -> (Option<(f64, f64)>, Option<u8>) {
    let mut dpi = None;
    let mut comps = None;
    let mut i = 2;
    while i + 4 <= bytes.len() && bytes[i] == 0xFF {
        let marker = bytes[i + 1];
        if marker == 0xD8 || (0xD0..=0xD7).contains(&marker) {
            i += 2;
            continue;
        }
        let len = u16::from_be_bytes([bytes[i + 2], bytes[i + 3]]) as usize;
        let seg = &bytes[(i + 4).min(bytes.len())..(i + 2 + len).min(bytes.len())];
        if marker == 0xE0 && seg.len() >= 12 && &seg[0..5] == b"JFIF\0" {
            let unit = seg[7];
            let x = u16::from_be_bytes([seg[8], seg[9]]) as f64;
            let y = u16::from_be_bytes([seg[10], seg[11]]) as f64;
            dpi = match unit {
                1 => Some((x, y)),
                2 => Some((x * 2.54, y * 2.54)),
                _ => None,
            };
        }
        if matches!(marker, 0xC0..=0xC3 | 0xC5..=0xC7 | 0xC9..=0xCB | 0xCD..=0xCF) && seg.len() >= 6 {
            comps = Some(seg[5]);
            break;
        }
        if marker == 0xDA {
            break;
        }
        i += 2 + len;
    }
    (dpi, comps)
}

fn decode_jpeg(bytes: &[u8]) -> Result<PageImage, String> {
    let err = |e: image::ImageError| format!("Could not read the JPEG image: {}", e);
    let mut decoder = image::codecs::jpeg::JpegDecoder::new(Cursor::new(bytes)).map_err(err)?;
    let orientation = decoder.orientation().unwrap_or(image::metadata::Orientation::NoTransforms);
    let (w, h) = decoder.dimensions();
    check_size(w, h)?;
    let (dpi, comps) = jpeg_info(bytes);
    if orientation == image::metadata::Orientation::NoTransforms && matches!(comps, Some(1) | Some(3)) {
        // Validate that it decodes before embedding it untouched.
        let img = DynamicImage::from_decoder(decoder).map_err(err)?;
        drop(img);
        return Ok(PageImage { width: w, height: h, dpi, pixels: Pixels::Jpeg(bytes.to_vec(), comps.unwrap()) });
    }
    let mut img = DynamicImage::from_decoder(decoder).map_err(err)?;
    img.apply_orientation(orientation);
    let (w, h) = (img.width(), img.height());
    let dpi = match (orientation, dpi) {
        (o, Some((x, y))) if matches!(
            o,
            image::metadata::Orientation::Rotate90
                | image::metadata::Orientation::Rotate270
                | image::metadata::Orientation::Rotate90FlipH
                | image::metadata::Orientation::Rotate270FlipH
        ) => Some((y, x)),
        (_, d) => d,
    };
    Ok(PageImage { width: w, height: h, dpi, pixels: to_pixels(img) })
}

// ─── TIFF ────────────────────────────────────────────────────────────────────

fn decode_tiff(bytes: &[u8]) -> Result<Vec<PageImage>, String> {
    use tiff::decoder::{Decoder, DecodingResult};
    use tiff::tags::Tag;
    use tiff::ColorType;

    let err = |e: tiff::TiffError| format!("Could not read the TIFF image: {}", e);
    let mut dec = Decoder::new(Cursor::new(bytes)).map_err(err)?;
    let mut pages = Vec::new();
    loop {
        if pages.len() >= MAX_PAGES {
            return Err(format!("The TIFF has more than {} pages.", MAX_PAGES));
        }
        let (w, h) = dec.dimensions().map_err(err)?;
        check_size(w, h)?;
        let color = dec.colortype().map_err(err)?;
        let res = |dec: &mut Decoder<Cursor<&[u8]>>, tag: Tag| -> Option<f64> {
            match dec.find_tag(tag).ok().flatten()? {
                tiff::decoder::ifd::Value::Rational(n, d) if d != 0 => Some(n as f64 / d as f64),
                v => v.into_f64().ok(),
            }
        };
        let unit = dec.find_tag_unsigned::<u16>(Tag::ResolutionUnit).ok().flatten().unwrap_or(2);
        let (rx, ry) = (res(&mut dec, Tag::XResolution), res(&mut dec, Tag::YResolution));
        let scale = match unit {
            2 => Some(1.0),  // inch
            3 => Some(2.54), // centimetre
            _ => None,
        };
        let dpi = match (rx, ry, scale) {
            (Some(x), Some(y), Some(s)) => Some((x * s, y * s)),
            _ => None,
        };
        let unsupported = || format!("This TIFF uses a colour format Qvault cannot read ({:?}).", color);
        let n = (w as usize) * (h as usize);
        let data = dec.read_image().map_err(err)?;
        let img: DynamicImage = match (color, data) {
            (ColorType::Gray(1), DecodingResult::U8(buf)) => {
                // Packed, MSB first, rows padded to bytes; 1 = white after
                // the decoder normalises WhiteIsZero.
                let stride = (w as usize).div_ceil(8);
                let mut g = GrayImage::new(w, h);
                for y in 0..h as usize {
                    for x in 0..w as usize {
                        let bit = buf.get(y * stride + x / 8).map_or(1, |b| (b >> (7 - (x % 8))) & 1);
                        g.put_pixel(x as u32, y as u32, image::Luma([if bit == 1 { 255 } else { 0 }]));
                    }
                }
                DynamicImage::ImageLuma8(g)
            }
            (ColorType::Gray(8), DecodingResult::U8(buf)) if buf.len() >= n => {
                DynamicImage::ImageLuma8(GrayImage::from_raw(w, h, buf[..n].to_vec()).ok_or_else(unsupported)?)
            }
            (ColorType::Gray(16), DecodingResult::U16(buf)) if buf.len() >= n => DynamicImage::ImageLuma8(
                GrayImage::from_raw(w, h, buf[..n].iter().map(|v| (v >> 8) as u8).collect()).ok_or_else(unsupported)?,
            ),
            (ColorType::GrayA(8), DecodingResult::U8(buf)) if buf.len() >= 2 * n => DynamicImage::ImageLumaA8(
                image::GrayAlphaImage::from_raw(w, h, buf[..2 * n].to_vec()).ok_or_else(unsupported)?,
            ),
            (ColorType::RGB(8), DecodingResult::U8(buf)) if buf.len() >= 3 * n => {
                DynamicImage::ImageRgb8(RgbImage::from_raw(w, h, buf[..3 * n].to_vec()).ok_or_else(unsupported)?)
            }
            (ColorType::RGB(16), DecodingResult::U16(buf)) if buf.len() >= 3 * n => DynamicImage::ImageRgb8(
                RgbImage::from_raw(w, h, buf[..3 * n].iter().map(|v| (v >> 8) as u8).collect()).ok_or_else(unsupported)?,
            ),
            (ColorType::RGBA(8), DecodingResult::U8(buf)) if buf.len() >= 4 * n => DynamicImage::ImageRgba8(
                image::RgbaImage::from_raw(w, h, buf[..4 * n].to_vec()).ok_or_else(unsupported)?,
            ),
            (ColorType::CMYK(8), DecodingResult::U8(buf)) if buf.len() >= 4 * n => {
                let rgb: Vec<u8> = buf[..4 * n]
                    .chunks_exact(4)
                    .flat_map(|p| {
                        let k = 255 - p[3] as u32;
                        [p[0], p[1], p[2]].map(|c| ((255 - c as u32) * k / 255) as u8)
                    })
                    .collect();
                DynamicImage::ImageRgb8(RgbImage::from_raw(w, h, rgb).ok_or_else(unsupported)?)
            }
            _ => return Err(unsupported()),
        };
        pages.push(PageImage { width: w, height: h, dpi, pixels: to_pixels(img) });
        if !dec.more_images() {
            break;
        }
        dec.next_image().map_err(err)?;
    }
    Ok(pages)
}

// ─── PDF ─────────────────────────────────────────────────────────────────────

fn page_size_pt(p: &PageImage) -> (f64, f64) {
    let (dx, dy) = match p.dpi {
        Some((x, y)) if plausible(x) && plausible(y) => (x, y),
        _ => {
            let d = p.width.max(p.height) as f64 / FALLBACK_LONG_SIDE_IN;
            (d, d)
        }
    };
    let (w, h) = (p.width as f64 / dx * 72.0, p.height as f64 / dy * 72.0);
    let shrink = (MAX_PAGE_PT / w.max(h)).min(1.0);
    (w * shrink, h * shrink)
}

fn image_stream(p: PageImage) -> Result<Stream, String> {
    let (w, h) = (p.width as i64, p.height as i64);
    Ok(match p.pixels {
        Pixels::Jpeg(bytes, comps) => Stream::new(
            dictionary! {
                "Type" => "XObject", "Subtype" => "Image", "Width" => w, "Height" => h,
                "ColorSpace" => if comps == 1 { "DeviceGray" } else { "DeviceRGB" },
                "BitsPerComponent" => 8, "Filter" => "DCTDecode",
            },
            bytes,
        ),
        Pixels::Gray(g) => {
            let bilevel = g.pixels().all(|p| p[0] == 0 || p[0] == 255);
            let (data, bpc) = if bilevel {
                let stride = (p.width as usize).div_ceil(8);
                let mut packed = vec![0u8; stride * p.height as usize];
                for (x, y, px) in g.enumerate_pixels() {
                    if px[0] == 255 {
                        packed[y as usize * stride + x as usize / 8] |= 0x80 >> (x % 8);
                    }
                }
                (packed, 1)
            } else {
                (g.into_raw(), 8)
            };
            let mut s = Stream::new(
                dictionary! {
                    "Type" => "XObject", "Subtype" => "Image", "Width" => w, "Height" => h,
                    "ColorSpace" => "DeviceGray", "BitsPerComponent" => bpc,
                },
                data,
            );
            s.compress().map_err(|e| format!("Could not compress the image: {}", e))?;
            s
        }
        Pixels::Rgb(rgb) => {
            let mut s = Stream::new(
                dictionary! {
                    "Type" => "XObject", "Subtype" => "Image", "Width" => w, "Height" => h,
                    "ColorSpace" => "DeviceRGB", "BitsPerComponent" => 8,
                },
                rgb.into_raw(),
            );
            s.compress().map_err(|e| format!("Could not compress the image: {}", e))?;
            s
        }
    })
}

fn build_pdf(pages: Vec<PageImage>) -> Result<Vec<u8>, String> {
    let mut doc = Document::with_version("1.5");
    let pages_id = doc.new_object_id();
    let mut kids = Vec::new();
    for p in pages {
        let (w_pt, h_pt) = page_size_pt(&p);
        let image_id = doc.add_object(image_stream(p)?);
        let content = Content {
            operations: vec![
                Operation::new("q", vec![]),
                Operation::new("cm", vec![Object::Real(w_pt as f32), 0.into(), 0.into(), Object::Real(h_pt as f32), 0.into(), 0.into()]),
                Operation::new("Do", vec![Object::Name(b"Im0".to_vec())]),
                Operation::new("Q", vec![]),
            ],
        };
        let content_id = doc.add_object(Stream::new(
            dictionary! {},
            content.encode().map_err(|e| format!("Could not build the page: {}", e))?,
        ));
        let page_id = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), Object::Real(w_pt as f32), Object::Real(h_pt as f32)],
            "Resources" => dictionary! { "XObject" => dictionary! { "Im0" => image_id } },
            "Contents" => content_id,
        });
        kids.push(Object::Reference(page_id));
    }
    let count = kids.len() as i64;
    doc.objects.insert(pages_id, Object::Dictionary(dictionary! { "Type" => "Pages", "Kids" => kids, "Count" => count }));
    let catalog = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    doc.trailer.set("Root", catalog);
    let mut out = Vec::new();
    doc.save_to(&mut out).map_err(|e| format!("Could not write the PDF: {}", e))?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::pdf_text::extract_page_texts;
    use crate::ocr::classify::{classify_pdf, OcrReason};

    fn png_bytes(img: &DynamicImage, dpi: Option<u32>) -> Vec<u8> {
        let mut out = Vec::new();
        img.write_to(&mut Cursor::new(&mut out), image::ImageFormat::Png).unwrap();
        if let Some(d) = dpi {
            // Insert a pHYs chunk after IHDR (8 + 25 bytes).
            let ppm = (d as f64 / 0.0254).round() as u32;
            let mut data = Vec::new();
            data.extend_from_slice(&ppm.to_be_bytes());
            data.extend_from_slice(&ppm.to_be_bytes());
            data.push(1);
            let mut chunk = Vec::new();
            chunk.extend_from_slice(&(9u32).to_be_bytes());
            chunk.extend_from_slice(b"pHYs");
            chunk.extend_from_slice(&data);
            let crc = crc32(&chunk[4..]);
            chunk.extend_from_slice(&crc.to_be_bytes());
            out.splice(33..33, chunk);
        }
        out
    }

    fn crc32(data: &[u8]) -> u32 {
        let mut c = 0xFFFF_FFFFu32;
        for &b in data {
            c ^= b as u32;
            for _ in 0..8 {
                c = if c & 1 != 0 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
            }
        }
        !c
    }

    fn sample(w: u32, h: u32) -> DynamicImage {
        let mut g = GrayImage::from_pixel(w, h, image::Luma([255]));
        for x in 10..w - 10 {
            g.put_pixel(x, h / 2, image::Luma([0]));
        }
        DynamicImage::ImageLuma8(g)
    }

    fn page_sizes(pdf: &[u8]) -> Vec<(f64, f64)> {
        let doc = Document::load_mem(pdf).unwrap();
        doc.get_pages()
            .values()
            .map(|&id| crate::document::pdf_redact::displayed_size(&doc, id))
            .collect()
    }

    fn assert_image_only(pdf: &[u8], pages: usize) {
        let doc = Document::load_mem(pdf).unwrap();
        assert_eq!(doc.get_pages().len(), pages);
        assert!(extract_page_texts(&doc).iter().all(|t| t.trim().is_empty()));
        assert!(doc.trailer.get(b"Info").is_err());
        for c in classify_pdf(&doc, &[]) {
            assert_eq!(c.reason, Some(OcrReason::NoText));
            assert!(c.image_coverage > 0.99);
        }
    }

    #[test]
    fn png_with_dpi_becomes_one_page_of_matching_size() {
        let bytes = png_bytes(&sample(850, 1100), Some(100));
        let out = image_to_pdf(&bytes, ImageKind::Png).unwrap();
        assert_eq!(out.pages, 1);
        assert_image_only(&out.pdf, 1);
        let (w, h) = page_sizes(&out.pdf)[0];
        assert!((w - 612.0).abs() < 0.5 && (h - 792.0).abs() < 0.5, "{}x{}", w, h);
        // Pure black-and-white is stored as 1-bit.
        let doc = Document::load_mem(&out.pdf).unwrap();
        let bpc = doc.objects.values().find_map(|o| match o {
            Object::Stream(s) if s.dict.get(b"Subtype").and_then(Object::as_name).ok() == Some(b"Image") => {
                s.dict.get(b"BitsPerComponent").and_then(Object::as_i64).ok()
            }
            _ => None,
        });
        assert_eq!(bpc, Some(1));
    }

    #[test]
    fn png_without_dpi_is_sized_as_a_page() {
        let bytes = png_bytes(&sample(800, 1600), None);
        let (w, h) = page_sizes(&image_to_pdf(&bytes, ImageKind::Png).unwrap().pdf)[0];
        assert!((h - 792.0).abs() < 0.5 && (w - 396.0).abs() < 0.5, "{}x{}", w, h);
    }

    #[test]
    fn rgba_png_is_flattened_onto_white() {
        let mut img = image::RgbaImage::from_pixel(200, 300, image::Rgba([255, 0, 0, 0]));
        img.put_pixel(5, 5, image::Rgba([200, 10, 10, 255]));
        let bytes = png_bytes(&DynamicImage::ImageRgba8(img), Some(150));
        let out = image_to_pdf(&bytes, ImageKind::Png).unwrap();
        assert_image_only(&out.pdf, 1);
    }

    #[test]
    fn jpeg_is_embedded_unchanged() {
        let mut jpg = Vec::new();
        DynamicImage::ImageRgb8(sample(400, 500).to_rgb8())
            .write_to(&mut Cursor::new(&mut jpg), image::ImageFormat::Jpeg)
            .unwrap();
        // The image crate writes JFIF without density units: fallback sizing.
        let out = image_to_pdf(&jpg, ImageKind::Jpeg).unwrap();
        assert_image_only(&out.pdf, 1);
        let doc = Document::load_mem(&out.pdf).unwrap();
        let embedded = doc.objects.values().find_map(|o| match o {
            Object::Stream(s) if s.dict.get(b"Filter").and_then(Object::as_name).ok() == Some(b"DCTDecode") => {
                Some(s.content.clone())
            }
            _ => None,
        });
        assert_eq!(embedded.as_deref(), Some(&jpg[..]));
    }

    #[test]
    fn multi_page_tiff_becomes_multi_page_pdf() {
        use tiff::encoder::{colortype, Rational, TiffEncoder};
        let mut buf = Cursor::new(Vec::new());
        {
            let mut enc = TiffEncoder::new(&mut buf).unwrap();
            for (w, h) in [(600u32, 800u32), (800, 600), (300, 300)] {
                let img = sample(w, h).to_luma8();
                let mut page = enc.new_image::<colortype::Gray8>(w, h).unwrap();
                page.resolution(tiff::tags::ResolutionUnit::Inch, Rational { n: 200, d: 1 });
                page.write_data(img.as_raw()).unwrap();
            }
            // An RGB page too.
            let rgb = RgbImage::from_pixel(100, 100, image::Rgb([10, 200, 30]));
            enc.write_image::<colortype::RGB8>(100, 100, rgb.as_raw()).unwrap();
        }
        let bytes = buf.into_inner();
        let out = image_to_pdf(&bytes, ImageKind::Tiff).unwrap();
        assert_eq!(out.pages, 4);
        assert_image_only(&out.pdf, 4);
        let sizes = page_sizes(&out.pdf);
        assert!((sizes[0].0 - 216.0).abs() < 0.5 && (sizes[0].1 - 288.0).abs() < 0.5, "{:?}", sizes[0]);
        assert!((sizes[1].0 - 288.0).abs() < 0.5 && (sizes[1].1 - 216.0).abs() < 0.5, "{:?}", sizes[1]);
    }

    #[test]
    fn garbage_and_oversized_inputs_are_refused() {
        assert!(image_to_pdf(b"not an image", ImageKind::Png).is_err());
        assert!(image_to_pdf(b"not an image", ImageKind::Jpeg).is_err());
        assert!(image_to_pdf(b"not an image", ImageKind::Tiff).is_err());
        assert!(check_size(100_000, 100_000).is_err());
        assert_eq!(ImageKind::from_extension("TIF"), Some(ImageKind::Tiff));
        assert_eq!(ImageKind::from_extension("docx"), None);
    }
}
