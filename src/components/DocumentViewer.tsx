import { useEffect, useRef, useState, useCallback } from "react";
import * as pdfjsLib from "pdfjs-dist";
import type { PDFDocumentProxy, TextItem } from "pdfjs-dist/types/src/display/api";
import { useDocument } from "../hooks/useDocument";
import { useRedactions } from "../hooks/useRedactions";
import { getDocumentBytes, scanDocumentText, addManualRedaction } from "../lib/tauri-commands";
import type { RedactionRect } from "../lib/tauri-commands";
import { PII_COLORS } from "../lib/types";
import type { Redaction } from "../lib/types";
import type { GetPdfRectsForExport } from "../App";

// Set up PDF.js worker
pdfjsLib.GlobalWorkerOptions.workerSrc = "/pdf.worker.min.mjs";

interface Props {
  documentId: string;
  onRectsReady: (getter: GetPdfRectsForExport) => void;
  advancedMode: boolean;
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

    const matchIdx = fullSpanText.indexOf(targetText);
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

export default function DocumentViewer({ documentId, onRectsReady, advancedMode }: Props) {
  const { document: doc, loading: docLoading, error, loadDocument } = useDocument();
  const { redactions, selectedId, setSelectedId, reload: reloadRedactions } = useRedactions(documentId);
  const [pdfDoc, setPdfDoc] = useState<PDFDocumentProxy | null>(null);
  const [pageData, setPageData] = useState<PageData[]>([]);
  const [loading, setLoading] = useState(true);
  const [scanning, setScanning] = useState(false);
  const [pdfError, setPdfError] = useState<string | null>(null);

  // Load document metadata
  useEffect(() => {
    loadDocument(documentId);
  }, [documentId, loadDocument]);

  // Load PDF, extract text, and trigger PII scan
  useEffect(() => {
    let cancelled = false;

    async function loadPdf() {
      setLoading(true);
      setPdfError(null);
      try {
        const base64 = await getDocumentBytes(documentId);

        const binaryStr = atob(base64);
        const bytes = new Uint8Array(binaryStr.length);
        for (let i = 0; i < binaryStr.length; i++) {
          bytes[i] = binaryStr.charCodeAt(i);
        }

        const pdf = await pdfjsLib.getDocument({ data: bytes }).promise;
        if (cancelled) return;
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
            const pdfX = tx[4];
            const pdfY = tx[5];
            const pdfHeight = ti.height;
            const pdfWidth = ti.width;

            const [x, y] = viewport.convertToViewportPoint(pdfX, pdfY + pdfHeight);
            const [x2, y2] = viewport.convertToViewportPoint(pdfX + pdfWidth, pdfY);

            spans.push({
              text: ti.str,
              x: Math.min(x, x2),
              y: Math.min(y, y2),
              width: Math.abs(x2 - x),
              height: Math.abs(y2 - y),
              pdfX,
              pdfY,
              pdfWidth,
              pdfHeight,
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

        if (cancelled) return;
        setPageData(allPageData);
        setLoading(false);

        // Run PII scan via Rust backend
        setScanning(true);
        const piiCount = await scanDocumentText(documentId, pageTexts);
        console.log("[Qvault] PII scan complete:", piiCount, "entities found");

        if (cancelled) return;
        setScanning(false);

        await reloadRedactions();
        await loadDocument(documentId);
      } catch (e) {
        console.error("[Qvault] Failed to load PDF:", e);
        if (!cancelled) setPdfError(e instanceof Error ? e.message : String(e));
      } finally {
        if (!cancelled) {
          setLoading(false);
          setScanning(false);
        }
      }
    }

    loadPdf();
    return () => { cancelled = true; };
  }, [documentId]);

  // Expose PDF-coordinate rects for export
  useEffect(() => {
    if (pageData.length === 0) return;

    onRectsReady(() => {
      const rects: RedactionRect[] = [];
      const approved = redactions.filter((r) => r.status === "approved");

      for (let pageIdx = 0; pageIdx < pageData.length; pageIdx++) {
        const pd = pageData[pageIdx];
        const pageRedactions = approved.filter((r) => r.page === pageIdx);
        const mapped = mapRedactionsToRects(pd.textSpans, pageRedactions);

        for (const { rects: spanRects } of mapped) {
          for (const span of spanRects) {
            rects.push({
              page: pageIdx,
              x: span.pdfX,
              y: span.pdfY,
              width: span.pdfWidth,
              height: span.pdfHeight,
            });
          }
        }
      }
      return rects;
    });
  }, [pageData, redactions, onRectsReady]);

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
          <p className="text-gray-500 text-sm">Loading document...</p>
        </div>
      </div>
    );
  }

  if (pdfError || error) {
    return (
      <div className="p-8">
        <div className="bg-red-50 border border-red-200 rounded-lg p-4 text-red-700 text-sm">
          <p className="font-medium">Error loading document</p>
          <p className="mt-1">{pdfError || error}</p>
        </div>
      </div>
    );
  }

  if (!doc || !pdfDoc) return null;

  return (
    <div className="p-6 overflow-auto">
      <div className="mb-4 flex items-center justify-between">
        <h2 className="text-lg font-semibold text-gray-900">{doc.filename}</h2>
        <div className="flex items-center gap-3">
          {scanning && (
            <span className="text-sm text-amber-600 flex items-center gap-1.5">
              <div className="w-3 h-3 border-2 border-amber-500 border-t-transparent rounded-full animate-spin" />
              Scanning for PII...
            </span>
          )}
          {advancedMode && (
            <span className="text-xs bg-purple-100 text-purple-700 px-2 py-0.5 rounded-full font-medium">
              Advanced Mode — Select text to redact
            </span>
          )}
          <span className="text-sm text-gray-500">
            {pdfDoc.numPages} page{pdfDoc.numPages !== 1 ? "s" : ""} | {doc.pii_count} PII detected
          </span>
        </div>
      </div>

      <div className="space-y-6 flex flex-col items-center">
        {pageData.map((pd, pageIdx) => (
          <PdfPage
            key={pd.pageNum}
            pageNum={pd.pageNum}
            pdfDoc={pdfDoc}
            pageData={pd}
            pageIdx={pageIdx}
            redactions={redactions}
            selectedId={selectedId}
            onSelectRedaction={setSelectedId}
            advancedMode={advancedMode}
            onManualRedaction={handleManualRedaction}
          />
        ))}
      </div>
    </div>
  );
}

/** Renders a single PDF page with canvas + redaction overlays + optional text selection */
function PdfPage({
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
          {pd.textSpans.map((span, i) => (
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
            Redact
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
                  border: isSelected
                    ? `2px solid ${color}`
                    : `1px solid ${color}80`,
                  borderRadius: 2,
                  zIndex: isSelected ? 10 : 1,
                  boxShadow: isSelected ? `0 0 0 2px ${color}40` : undefined,
                  pointerEvents: advancedMode ? "none" : "auto",
                }}
                title={`${redaction.category}: ${redaction.original_text}`}
              />
            );
          })
        )}

      <div className="absolute bottom-2 right-3 text-xs text-gray-400 bg-white/80 px-1.5 py-0.5 rounded">
        Page {pageNum}
      </div>
    </div>
  );
}
