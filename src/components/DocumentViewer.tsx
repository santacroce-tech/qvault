import { useEffect, useRef, useState, useCallback } from "react";
import { useTranslation } from "react-i18next";
import { listen } from "@tauri-apps/api/event";
import * as pdfjsLib from "pdfjs-dist";
import type { PDFDocumentProxy, TextItem } from "pdfjs-dist/types/src/display/api";
import { useDocument } from "../hooks/useDocument";
import type { UseRedactionsReturn } from "../hooks/useRedactions";
import {
  getDocumentBytes,
  extractDocxText,
  scanDocumentText,
  addManualRedaction,
  classifyPdfPages,
  getOcrStatus,
  getOcrPages,
  ocrPage,
  cancelOcr,
  finishOcr,
} from "../lib/tauri-commands";
import type {
  PixelRect,
  RedactedPageInput,
  PageClass,
  OcrStatus,
  OcrPageInfo,
  OcrPageLayout,
  OcrPageStatus,
  UnscannedPage,
} from "../lib/tauri-commands";
import { PII_COLORS } from "../lib/types";
import type { Redaction } from "../lib/types";
import type { RenderRedactedPages } from "../App";
import { FoldedText, makeNeedle } from "../lib/redaction-match";
import type { Needle } from "../lib/redaction-match";

// Set up PDF.js worker
pdfjsLib.GlobalWorkerOptions.workerSrc = "/pdf.worker.min.mjs";

interface Props {
  documentId: string;
  onRedactedPagesReady: (renderer: RenderRedactedPages) => void;
  advancedMode: boolean;
  redactionsHook: UseRedactionsReturn;
}

interface TextSpan {
  text: string;
  x: number;
  y: number;
  width: number;
  height: number;
  pdfX: number;
  pdfY: number;
  pdfWidth: number;
  pdfHeight: number;
  /** From OCR of the page image (boxes are placed by the backend at export). */
  ocr?: boolean;
}

/** OCR state of one page that needs OCR. */
interface PageOcrState {
  status: OcrPageStatus | "pending" | "running";
  dpi?: number | null;
  reason?: string | null;
}

/** Same rule as the backend's `visible_chars`. */
function visibleChars(s: string): number {
  let n = 0;
  for (const c of s) if (!/[\s\u00AD\u200B-\u200D\u2060\uFEFF]/u.test(c) && !/\p{Cc}/u.test(c)) n++;
  return n;
}

/**
 * Spans for an OCR'd page: one per layout segment, so that the spans' joined
 * text equals the OCR page text (detection offsets and highlights line up).
 * Separators have no box (zero size).
 */
function ocrSpans(layout: OcrPageLayout, viewportWidth: number): TextSpan[] {
  const k = viewportWidth / layout.widthPt;
  return layout.segments.map((seg) => {
    const b = seg.boxIndex !== null ? layout.boxes[seg.boxIndex] : null;
    return {
      text: seg.text,
      x: b ? b.x0 * k : 0,
      y: b ? b.y0 * k : 0,
      width: b ? (b.x1 - b.x0) * k : 0,
      height: b ? (b.y1 - b.y0) * k : 0,
      pdfX: 0,
      pdfY: 0,
      pdfWidth: 0,
      pdfHeight: 0,
      ocr: true,
    };
  });
}

/** Render one PDF page to PNG (base64) at `scale` (1 = 72 DPI). */
async function renderPagePng(pdf: PDFDocumentProxy, pageNum: number, scale: number): Promise<string> {
  const page = await pdf.getPage(pageNum);
  const viewport = page.getViewport({ scale });
  const canvas = document.createElement("canvas");
  canvas.width = Math.round(viewport.width);
  canvas.height = Math.round(viewport.height);
  const ctx = canvas.getContext("2d")!;
  ctx.fillStyle = "#ffffff";
  ctx.fillRect(0, 0, canvas.width, canvas.height);
  await page.render({ canvasContext: ctx, viewport }).promise;
  // PNG: lossless, so the backend's burn-in / OCR sees exactly the render.
  const blob = await new Promise<Blob>((resolve, reject) =>
    canvas.toBlob((b) => (b ? resolve(b) : reject(new Error("Failed to render page"))), "image/png")
  );
  const base64 = toBase64(new Uint8Array(await blob.arrayBuffer()));
  canvas.width = 0; // release the bitmap early
  return base64;
}

function mapRedactionsToRects(
  textSpans: TextSpan[],
  redactions: Redaction[]
): { redaction: Redaction; rects: TextSpan[] }[] {
  const results: { redaction: Redaction; rects: TextSpan[] }[] = [];

  const charToSpan: number[] = [];
  for (let i = 0; i < textSpans.length; i++) {
    for (let j = 0; j < textSpans[i].text.length; j++) {
      charToSpan.push(i);
    }
  }
  const fullSpanText = textSpans.map((s) => s.text).join("");

  for (const r of redactions) {
    if (r.status === "rejected") continue;

    const matchRects: TextSpan[] = [];
    const targetText = r.original_text;

    // Prefer the detection's own offset (exact for PDF.js and OCR page text,
    // which is what was scanned; offsets are UTF-8 bytes, so this only holds
    // for ASCII before the match), else the first occurrence.
    const atOffset =
      r.start_offset >= 0 &&
      fullSpanText.slice(r.start_offset, r.start_offset + targetText.length) === targetText;
    const matchIdx = atOffset ? r.start_offset : fullSpanText.indexOf(targetText);
    if (matchIdx >= 0) {
      const matchEnd = matchIdx + targetText.length;
      const spanIndices = new Set<number>();
      for (let i = matchIdx; i < matchEnd && i < charToSpan.length; i++) {
        spanIndices.add(charToSpan[i]);
      }
      for (const idx of spanIndices) {
        matchRects.push(textSpans[idx]);
      }
    }

    results.push({ redaction: r, rects: matchRects });
  }

  return results;
}

interface PageData {
  pageNum: number;
  textSpans: TextSpan[];
  width: number;
  height: number;
  scale: number;
}

/**
 * Boxes for every occurrence of every approved string on one page, in the
 * page's viewport coordinates (scale `pd.scale`). A box covers the whole
 * PDF.js text item a match touches, which over-redacts rather than risk a
 * partially visible word.
 *
 * Items are joined with a space so item edges count as word boundaries, as in
 * the backend's text extraction; whitespace is ignored when matching.
 */
interface OccurrenceBox {
  redactionId: string;
  occurrence: number;
  span: TextSpan;
}

function occurrenceBoxes(
  textSpans: TextSpan[],
  needles: { needle: Needle; redactionId: string }[]
): OccurrenceBox[] {
  let joined = "";
  const spanAt: number[] = [];
  textSpans.forEach((s, idx) => {
    if (idx > 0) {
      joined += " ";
      spanAt.push(-1);
    }
    joined += s.text;
    for (let k = 0; k < s.text.length; k++) spanAt.push(idx);
  });
  const folded = new FoldedText(joined);
  const out: OccurrenceBox[] = [];
  for (const { needle, redactionId } of needles) {
    folded.findAll(needle).forEach(([start, end], occurrence) => {
      const spans = new Set<number>();
      for (let i = start; i < end; i++) if (spanAt[i] >= 0) spans.add(spanAt[i]);
      for (const idx of spans) out.push({ redactionId, occurrence, span: textSpans[idx] });
    });
  }
  return out;
}

function toBase64(bytes: Uint8Array): string {
  let binary = "";
  const chunk = 0x8000;
  for (let i = 0; i < bytes.length; i += chunk) {
    binary += String.fromCharCode(...bytes.subarray(i, i + chunk));
  }
  return btoa(binary);
}

export default function DocumentViewer({ documentId, onRedactedPagesReady, advancedMode, redactionsHook }: Props) {
  const { t } = useTranslation();
  const { document: doc, loading: docLoading, error, loadDocument } = useDocument();
  const { redactions, selectedId, setSelectedId, reload: reloadRedactions } = redactionsHook;
  const [pdfDoc, setPdfDoc] = useState<PDFDocumentProxy | null>(null);
  const [pageData, setPageData] = useState<PageData[]>([]);
  const [loading, setLoading] = useState(true);
  const [scanning, setScanning] = useState(false);
  const [scanProgress, setScanProgress] = useState<{ page: number; totalPages: number; llmActive: boolean; phase?: string } | null>(null);
  const [llmProgress, setLlmProgress] = useState<{ phase: string; tokensGenerated: number } | null>(null);
  // Layer 3 outcome for the last scan (skipped / partial) — shown as a notice.
  const [layer3Notice, setLayer3Notice] = useState<{ status: string; reason: string | null } | null>(null);
  const [pdfError, setPdfError] = useState<string | null>(null);
  // OCR of scanned / image pages (0-based page -> state) and progress.
  const [ocrPages, setOcrPages] = useState<Record<number, PageOcrState>>({});
  const [ocrProgress, setOcrProgress] = useState<{ index: number; total: number; page: number } | null>(null);
  const [ocrInfo, setOcrInfo] = useState<OcrStatus | null>(null);
  const [ocrCancelling, setOcrCancelling] = useState(false);
  const ocrCancelRef = useRef(false);

  const handleCancelOcr = useCallback(async () => {
    ocrCancelRef.current = true;
    setOcrCancelling(true);
    try {
      await cancelOcr(documentId);
    } catch (e) {
      console.warn("[Qvault] OCR cancel failed:", e);
    }
  }, [documentId]);

  // Listen for scan progress events from the backend
  useEffect(() => {
    const unlisten = listen<{ documentId: string; page: number; totalPages: number; llmActive: boolean; phase?: string }>(
      "scan-progress",
      (event) => {
        if (event.payload.documentId === documentId) {
          setScanProgress({
            page: event.payload.page,
            totalPages: event.payload.totalPages,
            llmActive: event.payload.llmActive,
            phase: event.payload.phase,
          });
          // Reset LLM phase when a new page starts
          setLlmProgress(null);
        }
      }
    );
    return () => { unlisten.then((fn) => fn()); };
  }, [documentId]);

  // Listen for the Layer 3 (AI) outcome of a scan
  useEffect(() => {
    setLayer3Notice(null);
    const unlisten = listen<{ documentId: string; status: string; reason: string | null }>(
      "layer3-status",
      (event) => {
        if (event.payload.documentId === documentId) {
          setLayer3Notice(
            event.payload.status === "ran" ? null : { status: event.payload.status, reason: event.payload.reason }
          );
        }
      }
    );
    return () => { unlisten.then((fn) => fn()); };
  }, [documentId]);

  // Detections are stored page by page (Layers 1-2 first, then Layer 3);
  // refresh the review list as they arrive instead of only at the end.
  useEffect(() => {
    let pending: ReturnType<typeof setTimeout> | null = null;
    const unlisten = listen<{ documentId: string; page: number; layer: string; count: number }>(
      "scan-page-results",
      (event) => {
        if (event.payload.documentId !== documentId || event.payload.count === 0 || pending) return;
        pending = setTimeout(() => {
          pending = null;
          reloadRedactions().catch((e) => console.warn("[Qvault] reload during scan failed:", e));
        }, 400);
      }
    );
    return () => {
      if (pending) clearTimeout(pending);
      unlisten.then((fn) => fn());
    };
  }, [documentId, reloadRedactions]);

  // Listen for granular LLM inference progress
  useEffect(() => {
    const unlisten = listen<{ documentId: string; page: number; tokensGenerated: number; phase: string }>(
      "llm-progress",
      (event) => {
        if (event.payload.documentId === documentId) {
          setLlmProgress({
            phase: event.payload.phase,
            tokensGenerated: event.payload.tokensGenerated,
          });
        }
      }
    );
    return () => { unlisten.then((fn) => fn()); };
  }, [documentId]);

  // Load document metadata
  useEffect(() => {
    loadDocument(documentId);
  }, [documentId, loadDocument]);

  // Load document content — PDF via PDF.js, DOCX via stored page text
  useEffect(() => {
    let cancelled = false;

    async function loadContent() {
      setLoading(true);
      setPdfError(null);
      try {
        // Wait for doc metadata to know the file type
        if (!doc) return;

        if (doc.file_type === "docx") {
          await loadDocxContent(cancelled);
        } else {
          await loadPdfContent(cancelled);
        }
      } catch (e) {
        console.error("[Qvault] Failed to load document:", e);
        if (!cancelled) setPdfError(e instanceof Error ? e.message : String(e));
      } finally {
        if (!cancelled) {
          setLoading(false);
          setScanning(false); setScanProgress(null); setLlmProgress(null);
        }
      }
    }

    async function loadDocxContent(isCancelled: boolean) {
      // DOCX: extract text via Rust backend, then scan for PII
      const pageTexts = await extractDocxText(documentId);
      if (isCancelled) return;

      const allPageData: PageData[] = [];
      const pageWidth = 816; // ~8.5 inches at 96dpi
      const lineHeight = 18;
      const marginX = 40;
      const fontSize = 13;

      for (let pi = 0; pi < pageTexts.length; pi++) {
        const text = pageTexts[pi] || "";
        const lines = text.split("\n");
        const spans: TextSpan[] = [];
        let y = 30;

        for (const line of lines) {
          if (line.trim()) {
            spans.push({
              text: line,
              x: marginX,
              y,
              width: Math.min(line.length * (fontSize * 0.6), pageWidth - marginX * 2),
              height: lineHeight,
              pdfX: 0, pdfY: 0, pdfWidth: 0, pdfHeight: 0,
            });
          }
          y += lineHeight;
        }

        allPageData.push({
          pageNum: pi + 1,
          textSpans: spans,
          width: pageWidth,
          height: Math.max(y + 30, 400),
          scale: 1,
        });
      }

      if (isCancelled) return;
      setPageData(allPageData);
      setLoading(false);

      // Always scan — text is extracted fresh each time
      if (pageTexts.some((t) => t.length > 0)) {
        setScanning(true);
        const piiCount = await scanDocumentText(documentId, pageTexts);
        console.log("[Qvault] DOCX PII scan complete:", piiCount, "entities found");
        if (isCancelled) return;
        setScanning(false); setScanProgress(null);
      }
      await reloadRedactions();
      await loadDocument(documentId);
    }

    async function loadPdfContent(isCancelled: boolean) {
      const base64 = await getDocumentBytes(documentId);

      const binaryStr = atob(base64);
      const bytes = new Uint8Array(binaryStr.length);
      for (let i = 0; i < binaryStr.length; i++) {
        bytes[i] = binaryStr.charCodeAt(i);
      }

      const pdf = await pdfjsLib.getDocument({ data: bytes }).promise;
      if (isCancelled) return;
      setPdfDoc(pdf);

      const pageTexts: string[] = [];
      const allPageData: PageData[] = [];
      const scale = 1.5;

      for (let i = 1; i <= pdf.numPages; i++) {
        const page = await pdf.getPage(i);
        const viewport = page.getViewport({ scale });
        const textContent = await page.getTextContent();
        const spans: TextSpan[] = [];
        const pageTextParts: string[] = [];

        for (const item of textContent.items) {
          const ti = item as TextItem;
          if (!ti.str) continue;

          const tx = ti.transform;
          const rawPdfX = tx[4];
          const rawPdfY = tx[5];
          const rawPdfHeight = ti.height;
          const rawPdfWidth = ti.width;

          const [vx1, vy1] = viewport.convertToViewportPoint(rawPdfX, rawPdfY + rawPdfHeight);
          const [vx2, vy2] = viewport.convertToViewportPoint(rawPdfX + rawPdfWidth, rawPdfY);

          const viewX = Math.min(vx1, vx2);
          const viewY = Math.min(vy1, vy2);
          const viewW = Math.abs(vx2 - vx1);
          const viewH = Math.abs(vy2 - vy1);

          const [cpX1, cpY1] = viewport.convertToPdfPoint(viewX, viewY);
          const [cpX2, cpY2] = viewport.convertToPdfPoint(viewX + viewW, viewY + viewH);

          const pdfX = Math.min(cpX1, cpX2);
          const pdfY = Math.min(cpY1, cpY2);
          const pdfWidth = Math.abs(cpX2 - cpX1);
          const pdfHeight = Math.abs(cpY2 - cpY1);

          spans.push({
            text: ti.str,
            x: viewX, y: viewY, width: viewW, height: viewH,
            pdfX, pdfY, pdfWidth, pdfHeight,
          });

          pageTextParts.push(ti.str);
        }

        pageTexts.push(pageTextParts.join(""));

        allPageData.push({
          pageNum: i,
          textSpans: spans,
          width: viewport.width,
          height: viewport.height,
          scale,
        });
      }

      if (isCancelled) return;
      setPageData(allPageData);
      setLoading(false);

      // Pages with no usable text layer (scans, photos, outlined text) or
      // that are mostly images are OCR'd; their OCR text joins the page text
      // so all detection layers see it.
      const counts = pageTexts.map(visibleChars);
      const byCount = (): PageClass[] => counts.map((c, i) => ({
        page: i, textChars: c, imageCoverage: 0, needsOcr: c < 32,
        reason: c === 0 ? "noText" : c < 32 ? "littleText" : null,
      }));
      let classes: PageClass[];
      try {
        classes = (await classifyPdfPages(documentId, counts)) ?? byCount();
      } catch (e) {
        console.warn("[Qvault] Page classification failed; using text counts only:", e);
        classes = byCount();
      }
      const needing = classes.filter((c) => c.needsOcr).map((c) => c.page);
      if (needing.length > 0) {
        const layouts = await runOcr(pdf, needing, allPageData);
        for (const [p, layout] of layouts) {
          const pd = allPageData[p];
          if (!pd) continue;
          const sep = pageTexts[p].length > 0 ? "\n" : "";
          if (sep) pd.textSpans.push({ text: sep, x: 0, y: 0, width: 0, height: 0, pdfX: 0, pdfY: 0, pdfWidth: 0, pdfHeight: 0, ocr: true });
          pd.textSpans.push(...ocrSpans(layout, pd.width));
          pageTexts[p] = pageTexts[p] + sep + layout.text;
        }
        if (isCancelled) return;
        setPageData([...allPageData]);
      }

      setScanning(true);
      const piiCount = await scanDocumentText(documentId, pageTexts);
      console.log("[Qvault] PII scan complete:", piiCount, "entities found");

      if (isCancelled) return;
      setScanning(false); setScanProgress(null);

      await reloadRedactions();
      await loadDocument(documentId);
    }

    /**
     * OCR the given pages (0-based): reuse stored results at the current DPI,
     * otherwise render each page with PDF.js at the OCR DPI and send it to
     * the backend. Pages that cannot be scanned are recorded as such, so the
     * viewer flags them and export warns.
     */
    async function runOcr(
      pdf: PDFDocumentProxy,
      pages: number[],
      allPageData: PageData[],
    ): Promise<Map<number, OcrPageLayout>> {
      const out = new Map<number, OcrPageLayout>();
      const state: Record<number, PageOcrState> = {};
      for (const p of pages) state[p] = { status: "pending" };
      setOcrPages({ ...state });
      ocrCancelRef.current = false;
      setOcrCancelling(false);

      const noOcr: OcrStatus = { available: false, code: "unsupported", reason: null, dpi: 200 };
      let status: OcrStatus;
      try {
        status = (await getOcrStatus()) ?? noOcr;
      } catch {
        status = noOcr;
      }
      setOcrInfo(status);
      let cached: OcrPageInfo[] = [];
      try {
        cached = (await getOcrPages(documentId)) ?? [];
      } catch (e) {
        console.warn("[Qvault] Could not read stored OCR results:", e);
      }

      const unscanned: UnscannedPage[] = [];
      let stop: { status: "unavailable" | "cancelled"; reason: string | null } | null =
        status.available ? null : { status: "unavailable", reason: status.reason };
      try {
        for (let i = 0; i < pages.length; i++) {
          const p = pages[i];
          const hit = cached.find((c) => c.page === p && c.status === "done" && c.layout && c.dpi === status.dpi);
          if (hit?.layout) {
            out.set(p, hit.layout);
            state[p] = { status: "done", dpi: hit.dpi };
            setOcrPages({ ...state });
            continue;
          }
          if (!stop && ocrCancelRef.current) stop = { status: "cancelled", reason: null };
          if (stop || !allPageData[p]) {
            const st = stop ?? { status: "unavailable" as const, reason: null };
            unscanned.push({ page: p, status: st.status, reason: st.reason });
            state[p] = { status: st.status, reason: st.reason };
            setOcrPages({ ...state });
            continue;
          }
          state[p] = { status: "running" };
          setOcrPages({ ...state });
          setOcrProgress({ index: i, total: pages.length, page: p });
          try {
            const png = await renderPagePng(pdf, p + 1, status.dpi / 72);
            const layout = await ocrPage(documentId, p, png, status.dpi);
            out.set(p, layout);
            state[p] = { status: "done", dpi: status.dpi };
          } catch (e) {
            const msg = e instanceof Error ? e.message : String(e);
            if (msg.startsWith("OCR_CANCELLED")) {
              stop = { status: "cancelled", reason: null };
            } else if (msg.startsWith("OCR_UNAVAILABLE")) {
              stop = { status: "unavailable", reason: msg.replace(/^OCR_UNAVAILABLE:\s*/, "") };
            }
            const st = stop ?? { status: "failed" as const, reason: msg.replace(/^OCR_FAILED:\s*/, "") };
            unscanned.push({ page: p, status: st.status, reason: st.reason });
            state[p] = { status: st.status, reason: st.reason };
          }
          setOcrPages({ ...state });
        }
      } finally {
        setOcrProgress(null);
        setOcrCancelling(false);
        try {
          await finishOcr(documentId, unscanned);
        } catch (e) {
          console.warn("[Qvault] Could not record OCR results:", e);
        }
      }
      return out;
    }

    loadContent();
    return () => { cancelled = true; };
  }, [documentId, doc?.file_type]);

  // Expose the export renderer. PDF.js renders the page images because it is
  // already bundled for the viewer (no pdfium binary to ship per platform).
  // It renders only pages that carry an approved redaction, and only the
  // plain page: the backend burns the boxes into the pixels, swaps the pages,
  // and refuses the export if a redaction has no box or approved text would
  // remain (including on pages we did not render).
  useEffect(() => {
    if (!pdfDoc || pageData.length === 0) return;

    onRedactedPagesReady(async (): Promise<RedactedPageInput[]> => {
      const approved = redactions.filter((r) => r.status === "approved");

      // Every approved string is boxed wherever it occurs on a rendered page,
      // not only at the detected offset: a second occurrence left in the image
      // would be invisible to the backend's text-layer check.
      const byKey = new Map<string, { needle: Needle; rows: Redaction[] }>();
      for (const r of approved) {
        const needle = makeNeedle(r.original_text);
        if (!needle) continue;
        const entry = byKey.get(needle.key);
        if (entry) entry.rows.push(r);
        else byKey.set(needle.key, { needle, rows: [r] });
      }

      const pages = [...new Set(approved.map((r) => r.page))].sort((a, b) => a - b);
      const results: RedactedPageInput[] = [];
      for (const pageIdx of pages) {
        const pd = pageData[pageIdx];
        if (!pd) continue; // the backend reports these redactions as unplaceable

        const needles = [...byKey.values()].map(({ needle, rows }) => ({
          needle,
          redactionId: (rows.find((r) => r.page === pageIdx) ?? rows[0]).id,
        }));
        // Scanned pages are rendered at their OCR resolution (the backend
        // places their boxes from the OCR layout and OCRs the burned result);
        // other pages at 144 DPI.
        const ocr = ocrPages[pageIdx];
        const renderScale = ocr?.status === "done" && ocr.dpi ? ocr.dpi / 72 : 2.0;
        const k = renderScale / pd.scale;
        const textLayerSpans = pd.textSpans.filter((s) => !s.ocr);
        const rects: PixelRect[] = occurrenceBoxes(textLayerSpans, needles).map((b) => ({
          redactionId: b.redactionId,
          occurrence: b.occurrence,
          x: b.span.x * k,
          y: b.span.y * k,
          width: b.span.width * k,
          height: b.span.height * k,
        }));

        const imageBase64 = await renderPagePng(pdfDoc, pd.pageNum, renderScale);
        results.push({ page: pageIdx, imageBase64, rects });
      }
      return results;
    });
  }, [pdfDoc, pageData, redactions, ocrPages, onRedactedPagesReady]);

  // Handle manual redaction from text selection
  const handleManualRedaction = useCallback(
    async (pageIdx: number, selectedText: string) => {
      if (!selectedText.trim()) return;

      // Find the offset in the page's concatenated text
      const pd = pageData[pageIdx];
      if (!pd) return;
      const fullText = pd.textSpans.map((s) => s.text).join("");
      const startOffset = fullText.indexOf(selectedText);
      const endOffset = startOffset >= 0 ? startOffset + selectedText.length : 0;

      try {
        await addManualRedaction(
          documentId,
          pageIdx,
          startOffset,
          endOffset,
          selectedText,
          "[REDACTED]",
          "PersonName"
        );
        await reloadRedactions();
        await loadDocument(documentId);
      } catch (e) {
        console.error("Failed to add manual redaction:", e);
      }
    },
    [documentId, pageData, reloadRedactions, loadDocument]
  );

  if (loading || docLoading) {
    return (
      <div className="flex items-center justify-center h-full">
        <div className="flex flex-col items-center gap-3">
          <div className="w-8 h-8 border-2 border-blue-500 border-t-transparent rounded-full animate-spin" />
          <p className="text-gray-500 text-sm">{t('viewer.loading')}</p>
        </div>
      </div>
    );
  }

  if (pdfError || error) {
    return (
      <div className="p-8">
        <div className="bg-red-50 border border-red-200 rounded-lg p-4 text-red-700 text-sm">
          <p className="font-medium">{t('viewer.errorTitle')}</p>
          <p className="mt-1">{pdfError || error}</p>
        </div>
      </div>
    );
  }

  if (!doc) return null;
  const isDocx = doc.file_type === "docx";
  if (!isDocx && !pdfDoc) return null;

  return (
    <div className="p-6 overflow-auto">
      <div className="mb-4 flex items-center justify-between">
        <h2 className="text-lg font-semibold text-gray-900">{doc.filename}</h2>
        <div className="flex items-center gap-3">
          {ocrProgress && (
            <span className="text-sm text-amber-600 flex items-center gap-1.5">
              <div className="w-3 h-3 border-2 border-amber-500 border-t-transparent rounded-full animate-spin" />
              {t('viewer.ocrProgress', { current: ocrProgress.index + 1, total: ocrProgress.total, page: ocrProgress.page + 1 })}
              <button
                onClick={handleCancelOcr}
                disabled={ocrCancelling}
                className="ml-1 px-2 py-0.5 text-xs border border-amber-300 rounded hover:bg-amber-50 disabled:opacity-50"
              >
                {ocrCancelling ? t('viewer.ocrCancelling') : t('viewer.ocrCancel')}
              </button>
            </span>
          )}
          {scanning && (
            <span className="text-sm text-amber-600 flex items-center gap-1.5">
              <div className="w-3 h-3 border-2 border-amber-500 border-t-transparent rounded-full animate-spin" />
              {scanProgress
                ? (() => {
                    const pageLabel = `${scanProgress.page + 1}/${scanProgress.totalPages}`;
                    if (scanProgress.llmActive && !llmProgress && scanProgress.phase === "llm-loading") {
                      return `${t('viewer.scanning')} (${t('viewer.aiLoading')})`;
                    }
                    if (!scanProgress.llmActive || !llmProgress) {
                      return `${t('viewer.scanning')} (${pageLabel}${scanProgress.llmActive ? " · AI" : ""})`;
                    }
                    const phase = llmProgress.phase;
                    let detail = "";
                    if (phase === "tokenizing") detail = "AI: tokenizing";
                    else if (phase.startsWith("processing")) detail = `AI: ${phase}`;
                    else if (phase === "generating") detail = `AI: generating (${llmProgress.tokensGenerated} tokens)`;
                    else if (phase === "done") detail = "AI: done";
                    else if (phase.startsWith("chunk")) detail = `AI: ${phase}`;
                    else detail = `AI: ${phase}`;
                    return `${t('viewer.scanning')} (${pageLabel} · ${detail})`;
                  })()
                : t('viewer.scanning')}
            </span>
          )}
          {advancedMode && (
            <span className="text-xs bg-purple-100 text-purple-700 px-2 py-0.5 rounded-full font-medium">
              {t('viewer.advancedMode')}
            </span>
          )}
          <span className="text-sm text-gray-500">
            {t('viewer.pageInfo', { count: isDocx ? pageData.length : pdfDoc?.numPages ?? 0, piiCount: doc.pii_count })}
          </span>
        </div>
      </div>

      {(() => {
        const unscanned = Object.entries(ocrPages)
          .filter(([, st]) => st.status === "unavailable" || st.status === "cancelled" || st.status === "failed")
          .map(([p]) => Number(p) + 1)
          .sort((a, b) => a - b);
        if (unscanned.length === 0) return null;
        const statuses = new Set(Object.values(ocrPages).map((st) => st.status));
        const pages = unscanned.join(", ");
        let detail: string;
        if (statuses.has("unavailable")) {
          detail = ocrInfo?.code === "models_missing"
            ? t('viewer.ocrModelsMissing')
            : ocrInfo?.code === "not_bundled"
            ? t('viewer.ocrNotBundled')
            : ocrInfo?.code === "unsupported"
            ? t('viewer.ocrUnsupported')
            : t('viewer.ocrEngineUnavailable');
        } else if (statuses.has("cancelled")) {
          detail = t('viewer.ocrCancelledDetail');
        } else {
          detail = t('viewer.ocrFailedDetail');
        }
        return (
          <div className="mb-4 text-sm bg-red-50 border border-red-200 text-red-800 rounded-lg px-3 py-2">
            <p className="font-medium">{t('viewer.ocrUnscannedTitle', { count: unscanned.length, pages })}</p>
            <p className="mt-0.5">{detail}</p>
          </div>
        );
      })()}

      {layer3Notice && (
        <div className="mb-4 text-sm bg-amber-50 border border-amber-200 text-amber-800 rounded-lg px-3 py-2">
          {layer3Notice.status === "skipped"
            ? t('viewer.layer3Skipped', { reason: layer3Notice.reason ?? "" })
            : t('viewer.layer3Partial', { reason: layer3Notice.reason ?? "" })}
        </div>
      )}

      <div className="space-y-6 flex flex-col items-center">
        {pageData.map((pd, pageIdx) =>
          isDocx ? (
            <DocxPage
              key={pd.pageNum}
              pageData={pd}
              pageIdx={pageIdx}
              redactions={redactions}
              selectedId={selectedId}
              onSelectRedaction={setSelectedId}
              advancedMode={advancedMode}
              onManualRedaction={handleManualRedaction}
            />
          ) : (
            <PdfPage
              key={pd.pageNum}
              ocr={ocrPages[pageIdx]}
              pageNum={pd.pageNum}
              pdfDoc={pdfDoc!}
              pageData={pd}
              pageIdx={pageIdx}
              redactions={redactions}
              selectedId={selectedId}
              onSelectRedaction={setSelectedId}
              advancedMode={advancedMode}
              onManualRedaction={handleManualRedaction}
            />
          )
        )}
      </div>
    </div>
  );
}

/** Renders a DOCX page as text with PII highlight overlays */
function DocxPage({
  pageData: pd,
  pageIdx,
  redactions,
  selectedId,
  onSelectRedaction,
  advancedMode,
  onManualRedaction,
}: {
  pageData: PageData;
  pageIdx: number;
  redactions: Redaction[];
  selectedId: string | null;
  onSelectRedaction: (id: string) => void;
  advancedMode: boolean;
  onManualRedaction: (pageIdx: number, text: string) => void;
}) {
  const { t } = useTranslation();
  const containerRef = useRef<HTMLDivElement>(null);
  const [selectionPopup, setSelectionPopup] = useState<{
    x: number;
    y: number;
    text: string;
  } | null>(null);

  const pageRedactions = redactions.filter((r) => r.page === pageIdx);
  const mapped = mapRedactionsToRects(pd.textSpans, pageRedactions);

  const handleMouseUp = useCallback(() => {
    if (!advancedMode) return;

    const selection = window.getSelection();
    if (!selection || selection.isCollapsed) {
      setSelectionPopup(null);
      return;
    }

    const text = selection.toString().trim();
    if (!text) {
      setSelectionPopup(null);
      return;
    }

    const range = selection.getRangeAt(0);
    const rect = range.getBoundingClientRect();
    const pageEl = containerRef.current;
    if (!pageEl) return;

    const pageRect = pageEl.getBoundingClientRect();
    setSelectionPopup({
      x: rect.left - pageRect.left + rect.width / 2,
      y: rect.top - pageRect.top - 8,
      text,
    });
  }, [advancedMode]);

  const handleAddRedaction = useCallback(() => {
    if (selectionPopup) {
      onManualRedaction(pageIdx, selectionPopup.text);
      setSelectionPopup(null);
      window.getSelection()?.removeAllRanges();
    }
  }, [selectionPopup, pageIdx, onManualRedaction]);

  return (
    <div
      ref={containerRef}
      className="relative bg-white shadow-lg border border-gray-200 p-8 font-serif text-sm leading-relaxed select-text"
      style={{ width: pd.width, minHeight: pd.height, cursor: advancedMode ? "text" : "default" }}
      onMouseUp={handleMouseUp}
    >
      {/* Text content */}
      {pd.textSpans.map((span, i) => (
        <div key={i} className="whitespace-pre-wrap text-gray-900" style={{ minHeight: span.height }}>
          {span.text}
        </div>
      ))}

      {/* Redaction overlays (positioned over the text flow) */}
      {mapped.map(({ redaction, rects }) =>
        rects.map((rect, ri) => {
          const color = PII_COLORS[redaction.category] || "#6B7280";
          const isSelected = redaction.id === selectedId;
          const isApproved = redaction.status === "approved";

          return (
            <div
              key={`${redaction.id}-${ri}`}
              onClick={() => onSelectRedaction(redaction.id)}
              className="absolute cursor-pointer transition-all"
              style={{
                left: rect.x,
                top: rect.y,
                width: rect.width,
                height: rect.height,
                backgroundColor: isApproved ? `${color}55` : `${color}30`,
                border: isSelected ? `2px solid ${color}` : `1px solid ${color}80`,
                borderRadius: 2,
                zIndex: isSelected ? 10 : 1,
                pointerEvents: advancedMode ? "none" : "auto",
              }}
              title={`${redaction.category}: ${redaction.original_text}`}
            />
          );
        })
      )}

      {/* Selection popup for manual redaction */}
      {selectionPopup && (
        <div
          className="absolute z-30 bg-gray-900 text-white text-xs rounded-lg shadow-lg px-3 py-1.5 flex items-center gap-2 -translate-x-1/2 -translate-y-full"
          style={{ left: selectionPopup.x, top: selectionPopup.y }}
        >
          <span className="max-w-[200px] truncate">
            &quot;{selectionPopup.text}&quot;
          </span>
          <button
            onClick={handleAddRedaction}
            className="bg-red-500 hover:bg-red-600 text-white px-2 py-0.5 rounded text-xs font-medium whitespace-nowrap"
          >
            {t('viewer.redact')}
          </button>
          <button
            onClick={() => {
              setSelectionPopup(null);
              window.getSelection()?.removeAllRanges();
            }}
            className="text-gray-400 hover:text-white"
          >
            ✕
          </button>
        </div>
      )}
    </div>
  );
}

/** Renders a single PDF page with canvas + redaction overlays + optional text selection */
function PdfPage({
  ocr,
  pageNum,
  pdfDoc,
  pageData: pd,
  pageIdx,
  redactions,
  selectedId,
  onSelectRedaction,
  advancedMode,
  onManualRedaction,
}: {
  ocr?: PageOcrState;
  pageNum: number;
  pdfDoc: PDFDocumentProxy;
  pageData: PageData;
  pageIdx: number;
  redactions: Redaction[];
  selectedId: string | null;
  onSelectRedaction: (id: string) => void;
  advancedMode: boolean;
  onManualRedaction: (pageIdx: number, text: string) => void;
}) {
  const { t } = useTranslation();
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const [rendered, setRendered] = useState(false);
  const [selectionPopup, setSelectionPopup] = useState<{
    x: number;
    y: number;
    text: string;
  } | null>(null);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;

    let cancelled = false;

    async function render() {
      const page = await pdfDoc.getPage(pageNum);
      const viewport = page.getViewport({ scale: pd.scale });
      const ctx = canvas!.getContext("2d");
      if (!ctx || cancelled) return;

      canvas!.width = viewport.width;
      canvas!.height = viewport.height;

      await page.render({
        canvasContext: ctx,
        viewport,
      }).promise;

      if (!cancelled) setRendered(true);
    }

    render().catch((e) => console.error("[Qvault] Render failed:", e));
    return () => { cancelled = true; };
  }, [pdfDoc, pageNum, pd.scale]);

  // Handle text selection in advanced mode
  const handleMouseUp = useCallback(() => {
    if (!advancedMode) return;

    const selection = window.getSelection();
    if (!selection || selection.isCollapsed) {
      setSelectionPopup(null);
      return;
    }

    const text = selection.toString().trim();
    if (!text) {
      setSelectionPopup(null);
      return;
    }

    const range = selection.getRangeAt(0);
    const rect = range.getBoundingClientRect();
    const pageEl = canvasRef.current?.parentElement;
    if (!pageEl) return;

    const pageRect = pageEl.getBoundingClientRect();
    setSelectionPopup({
      x: rect.left - pageRect.left + rect.width / 2,
      y: rect.top - pageRect.top - 8,
      text,
    });
  }, [advancedMode]);

  const handleAddRedaction = useCallback(() => {
    if (selectionPopup) {
      onManualRedaction(pageIdx, selectionPopup.text);
      setSelectionPopup(null);
      window.getSelection()?.removeAllRanges();
    }
  }, [selectionPopup, pageIdx, onManualRedaction]);

  const pageRedactions = redactions.filter((r) => r.page === pageIdx);
  const mapped = mapRedactionsToRects(pd.textSpans, pageRedactions);

  return (
    <div
      className="relative bg-white shadow-lg border border-gray-200"
      style={{ width: pd.width, height: pd.height }}
    >
      <canvas ref={canvasRef} style={{ position: "absolute", top: 0, left: 0 }} />

      {/* Transparent text selection layer (advanced mode) */}
      {advancedMode && rendered && (
        <div
          className="absolute inset-0"
          style={{ zIndex: 20 }}
          onMouseUp={handleMouseUp}
        >
          {pd.textSpans.map((span, i) => span.width <= 0 || span.height <= 0 ? null : (
            <span
              key={i}
              className="absolute select-text"
              style={{
                left: span.x,
                top: span.y,
                width: span.width,
                height: span.height,
                fontSize: span.height * 0.85,
                lineHeight: `${span.height}px`,
                color: "transparent",
                cursor: "text",
                whiteSpace: "pre",
              }}
            >
              {span.text}
            </span>
          ))}
        </div>
      )}

      {/* Selection popup */}
      {selectionPopup && (
        <div
          className="absolute z-30 bg-gray-900 text-white text-xs rounded-lg shadow-lg px-3 py-1.5 flex items-center gap-2 -translate-x-1/2 -translate-y-full"
          style={{ left: selectionPopup.x, top: selectionPopup.y }}
        >
          <span className="max-w-[200px] truncate">
            &quot;{selectionPopup.text}&quot;
          </span>
          <button
            onClick={handleAddRedaction}
            className="bg-red-500 hover:bg-red-600 text-white px-2 py-0.5 rounded text-xs font-medium whitespace-nowrap"
          >
            {t('viewer.redact')}
          </button>
          <button
            onClick={() => {
              setSelectionPopup(null);
              window.getSelection()?.removeAllRanges();
            }}
            className="text-gray-400 hover:text-white"
          >
            &#10005;
          </button>
        </div>
      )}

      {/* Redaction overlays */}
      {rendered &&
        mapped.map(({ redaction, rects }) =>
          rects.filter((rect) => rect.width > 0 && rect.height > 0).map((rect, ri) => {
            const color = PII_COLORS[redaction.category] || "#6B7280";
            const isSelected = redaction.id === selectedId;
            const isApproved = redaction.status === "approved";

            return (
              <div
                key={`${redaction.id}-${ri}`}
                onClick={() => onSelectRedaction(redaction.id)}
                className="absolute cursor-pointer transition-all"
                style={{
                  left: rect.x,
                  top: rect.y,
                  width: rect.width,
                  height: rect.height,
                  backgroundColor: isApproved ? `${color}55` : `${color}30`,
                  border: isSelected
                    ? `2px solid ${color}`
                    : `1px solid ${color}80`,
                  borderRadius: 2,
                  zIndex: isSelected ? 10 : 1,
                  boxShadow: isSelected ? `0 0 0 2px ${color}40` : undefined,
                  pointerEvents: advancedMode ? "none" : "auto",
                }}
                title={`${t('pii.' + redaction.category)}: ${redaction.original_text}`}
              />
            );
          })
        )}

      {ocr && (ocr.status === "unavailable" || ocr.status === "cancelled" || ocr.status === "failed") && (
        <div
          className="absolute top-0 left-0 right-0 z-20 bg-red-600/90 text-white text-xs font-medium px-3 py-1.5"
          role="alert"
        >
          {ocr.status === "unavailable"
            ? t('viewer.pageNotScannedUnavailable')
            : ocr.status === "cancelled"
            ? t('viewer.pageNotScannedCancelled')
            : t('viewer.pageNotScannedFailed')}
        </div>
      )}
      {ocr && (ocr.status === "pending" || ocr.status === "running") && (
        <div className="absolute top-2 left-2 z-20 text-xs bg-amber-100 text-amber-800 px-2 py-0.5 rounded flex items-center gap-1">
          {ocr.status === "running" && (
            <div className="w-2.5 h-2.5 border-2 border-amber-500 border-t-transparent rounded-full animate-spin" />
          )}
          {ocr.status === "running" ? t('viewer.pageOcrRunning') : t('viewer.pageOcrPending')}
        </div>
      )}
      {ocr?.status === "done" && (
        <div
          className="absolute top-2 left-2 z-20 text-[10px] bg-blue-50 text-blue-700 border border-blue-200 px-1.5 py-0.5 rounded"
          title={t('viewer.pageOcrDoneTitle')}
        >
          {t('viewer.pageOcrDone', { dpi: ocr.dpi ?? "" })}
        </div>
      )}
      <div className="absolute bottom-2 right-3 text-xs text-gray-400 bg-white/80 px-1.5 py-0.5 rounded">
        {t('viewer.page', { number: pageNum })}
      </div>
    </div>
  );
}
