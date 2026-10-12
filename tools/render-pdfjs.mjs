// Render PDF pages to PNG with the same PDF.js build and canvas path the app
// uses (DocumentViewer.tsx renderPagePng), in headless Chrome via puppeteer.
// Used to check that the OCR integration test's pdftoppm stand-in matches
// what the app actually sends to OCR.
//
//   node tools/render-pdfjs.mjs <file.pdf> <dpi> <out dir>
//
// Writes <out dir>/p-<n>.png (1-based). Local only: a throwaway HTTP server
// on 127.0.0.1 serves pdfjs-dist and the PDF to the headless browser.
import fs from "node:fs";
import http from "node:http";
import path from "node:path";
import { fileURLToPath } from "node:url";
import puppeteer from "puppeteer";

const [pdfPath, dpiArg, outDir] = process.argv.slice(2);
if (!pdfPath || !dpiArg || !outDir) {
  console.error("usage: node tools/render-pdfjs.mjs <file.pdf> <dpi> <out dir>");
  process.exit(2);
}
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const pdfjs = path.join(root, "node_modules", "pdfjs-dist", "build");
const pdfBytes = fs.readFileSync(pdfPath);

const page = `<!doctype html><meta charset="utf-8"><script type="module">
import * as pdfjsLib from "/pdfjs/pdf.mjs";
pdfjsLib.GlobalWorkerOptions.workerSrc = "/pdfjs/pdf.worker.mjs";
window.renderAll = async (scale) => {
  const data = new Uint8Array(await (await fetch("/doc.pdf")).arrayBuffer());
  const pdf = await pdfjsLib.getDocument({ data }).promise;
  const out = [];
  for (let i = 1; i <= pdf.numPages; i++) {
    const p = await pdf.getPage(i);
    const viewport = p.getViewport({ scale });
    const canvas = document.createElement("canvas");
    canvas.width = Math.round(viewport.width);
    canvas.height = Math.round(viewport.height);
    const ctx = canvas.getContext("2d");
    ctx.fillStyle = "#ffffff";
    ctx.fillRect(0, 0, canvas.width, canvas.height);
    await p.render({ canvasContext: ctx, viewport }).promise;
    out.push(canvas.toDataURL("image/png"));
  }
  return out;
};
window.ready = true;
</script>`;

const server = http.createServer((req, res) => {
  if (req.url === "/") {
    res.writeHead(200, { "content-type": "text/html" });
    return res.end(page);
  }
  if (req.url === "/doc.pdf") {
    res.writeHead(200, { "content-type": "application/pdf" });
    return res.end(pdfBytes);
  }
  const m = /^\/pdfjs\/([\w.-]+)$/.exec(req.url);
  if (m && fs.existsSync(path.join(pdfjs, m[1]))) {
    res.writeHead(200, { "content-type": "text/javascript" });
    return res.end(fs.readFileSync(path.join(pdfjs, m[1])));
  }
  res.writeHead(404);
  res.end();
});
await new Promise((r) => server.listen(0, "127.0.0.1", r));
const url = `http://127.0.0.1:${server.address().port}/`;

const browser = await puppeteer.launch({ headless: true });
try {
  const tab = await browser.newPage();
  await tab.goto(url);
  await tab.waitForFunction("window.ready === true");
  const pages = await tab.evaluate((s) => window.renderAll(s), Number(dpiArg) / 72);
  fs.mkdirSync(outDir, { recursive: true });
  pages.forEach((dataUrl, i) => {
    fs.writeFileSync(path.join(outDir, `p-${i + 1}.png`), Buffer.from(dataUrl.split(",")[1], "base64"));
  });
  console.log(`rendered ${pages.length} page(s) at ${dpiArg} DPI to ${outDir}`);
} finally {
  await browser.close();
  server.close();
}
