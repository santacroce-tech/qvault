// Build the "scanned" test fixture: landing/demo.pdf rasterized to an
// image-only PDF (no text layer, no fonts), like a clean 300 DPI greyscale
// office scan.
//
//   node tools/make-scanned-fixture.mjs
//
// Needs poppler's `pdftoppm` on PATH. Output:
//   src-tauri/tests/fixtures/demo-scanned.pdf
//
// Each page is one 8-bit DeviceGray image (Flate, lossless) at 300 DPI drawn
// to fill a US Letter page. The PDF is written by hand (a few objects) so
// nothing but the image ends up in it: no text, no fonts, no metadata.
// Re-run after changing landing/demo.pdf; the OCR tests use this file.
import { execFileSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import zlib from "node:zlib";
import { fileURLToPath } from "node:url";

const DPI = 300;

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const src = path.join(root, "landing", "demo.pdf");
const outDir = path.join(root, "src-tauri", "tests", "fixtures");
const out = path.join(outDir, "demo-scanned.pdf");

/** Parse a binary PGM (P5, maxval 255). */
function readPgm(file) {
  const buf = fs.readFileSync(file);
  const fields = [];
  let i = 0;
  while (fields.length < 4) {
    while (/\s/.test(String.fromCharCode(buf[i]))) i++;
    if (buf[i] === 0x23) { // comment
      while (buf[i] !== 0x0a) i++;
      continue;
    }
    let s = "";
    while (!/\s/.test(String.fromCharCode(buf[i]))) s += String.fromCharCode(buf[i++]);
    fields.push(s);
  }
  i++; // single whitespace after maxval
  const [magic, w, h, max] = fields;
  if (magic !== "P5" || max !== "255") throw new Error(`unexpected PGM header in ${file}`);
  return { width: Number(w), height: Number(h), data: buf.subarray(i, i + Number(w) * Number(h)) };
}

function writePdf(images, file) {
  const parts = [];
  const offsets = [];
  let size = 0;
  const push = (b) => { const buf = Buffer.isBuffer(b) ? b : Buffer.from(b, "latin1"); parts.push(buf); size += buf.length; };
  const obj = (n, body, stream) => {
    offsets[n] = size;
    push(`${n} 0 obj\n${body}\n`);
    if (stream) { push("stream\n"); push(stream); push("\nendstream\n"); }
    push("endobj\n");
  };
  push("%PDF-1.4\n%\xE2\xE3\xCF\xD3\n");
  const pageIds = images.map((_, k) => 3 + k * 3);
  obj(1, "<< /Type /Catalog /Pages 2 0 R >>");
  obj(2, `<< /Type /Pages /Kids [${pageIds.map((id) => `${id} 0 R`).join(" ")}] /Count ${images.length} >>`);
  images.forEach((img, k) => {
    const [pageId, contentId, imageId] = [3 + k * 3, 4 + k * 3, 5 + k * 3];
    const content = Buffer.from("q 612 0 0 792 0 0 cm /Im0 Do Q", "latin1");
    obj(pageId, `<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /XObject << /Im0 ${imageId} 0 R >> >> /Contents ${contentId} 0 R >>`);
    obj(contentId, `<< /Length ${content.length} >>`, content);
    const data = zlib.deflateSync(img.data, { level: 9 });
    obj(imageId, `<< /Type /XObject /Subtype /Image /Width ${img.width} /Height ${img.height} /ColorSpace /DeviceGray /BitsPerComponent 8 /Filter /FlateDecode /Length ${data.length} >>`, data);
  });
  const count = 3 + images.length * 3;
  const xref = size;
  push(`xref\n0 ${count}\n0000000000 65535 f \n`);
  for (let n = 1; n < count; n++) push(`${String(offsets[n]).padStart(10, "0")} 00000 n \n`);
  push(`trailer\n<< /Size ${count} /Root 1 0 R >>\nstartxref\n${xref}\n%%EOF\n`);
  fs.writeFileSync(file, Buffer.concat(parts));
}

const tmp = fs.mkdtempSync(path.join(os.tmpdir(), "qvault-scan-"));
try {
  execFileSync("pdftoppm", ["-r", String(DPI), "-gray", src, path.join(tmp, "page")]);
  const pages = fs.readdirSync(tmp).filter((f) => f.endsWith(".pgm")).sort();
  if (pages.length === 0) throw new Error("pdftoppm produced no pages");
  fs.mkdirSync(outDir, { recursive: true });
  writePdf(pages.map((p) => readPgm(path.join(tmp, p))), out);
  console.log(`wrote ${path.relative(root, out)} (${pages.length} pages, ${fs.statSync(out).size} bytes)`);
} finally {
  fs.rmSync(tmp, { recursive: true, force: true });
}
