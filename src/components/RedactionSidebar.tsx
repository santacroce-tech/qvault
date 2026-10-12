import { useState, useRef, useCallback, useEffect } from "react";
import { useTranslation } from "react-i18next";
import { exportRedactedDocx, exportRedactedPdf, openFile } from "../lib/tauri-commands";
import type { UseRedactionsReturn } from "../hooks/useRedactions";
import type { RenderRedactedPages } from "../App";
import { PII_COLORS, piiCategoryLabel } from "../lib/types";
import type { PiiCategory } from "../lib/types";

interface Props {
  documentId: string;
  fileType: string;
  renderRedactedPages: RenderRedactedPages;
  advancedMode: boolean;
  onToggleAdvanced: () => void;
  redactionsHook: UseRedactionsReturn;
  onExportSuccess?: (outputPath: string) => void;
  /** When true, renders as a mobile slide-up bottom sheet */
  mobile?: boolean;
}

export default function RedactionSidebar({
  documentId,
  fileType,
  renderRedactedPages,
  advancedMode,
  onToggleAdvanced,
  redactionsHook,
  onExportSuccess,
  mobile,
}: Props) {
  const { t } = useTranslation();
  const {
    redactions,
    loading,
    selectedId,
    setSelectedId,
    updateRedaction,
    bulkApprove,
  } = redactionsHook;
  const [exporting, setExporting] = useState(false);
  const [exportError, setExportError] = useState<string | null>(null);
  // Pages that need OCR but were not scanned: export waits for the user to
  // confirm (the backend refuses without that acknowledgement).
  const [unscannedWarning, setUnscannedWarning] = useState<{ pages: number[]; message: string } | null>(null);
  const [verifyProgress, setVerifyProgress] = useState<{ done: number; total: number } | null>(null);

  useEffect(() => {
    setUnscannedWarning(null);
    let unlisten: (() => void) | undefined;
    let cancelled = false;
    (async () => {
      try {
        const { listen } = await import("@tauri-apps/api/event");
        const u = await listen<{ documentId: string; done: number; total: number }>("export-progress", (e) => {
          if (e.payload.documentId === documentId) setVerifyProgress({ done: e.payload.done, total: e.payload.total });
        });
        if (cancelled) u(); else unlisten = u;
      } catch {
        // Not running inside Tauri (browser preview).
      }
    })();
    return () => { cancelled = true; unlisten?.(); };
  }, [documentId]);
  const [editingId, setEditingId] = useState<string | null>(null);
  const [editValue, setEditValue] = useState("");

  // Mobile bottom sheet state
  const [sheetOpen, setSheetOpen] = useState(false);
  const dragStartY = useRef(0);
  const sheetRef = useRef<HTMLDivElement>(null);

  const handleDragStart = useCallback((e: React.TouchEvent | React.MouseEvent) => {
    const clientY = "touches" in e ? e.touches[0].clientY : e.clientY;
    dragStartY.current = clientY;
  }, []);

  const handleDragEnd = useCallback((e: React.TouchEvent | React.MouseEvent) => {
    const clientY = "changedTouches" in e ? e.changedTouches[0].clientY : e.clientY;
    const delta = clientY - dragStartY.current;
    if (delta < -30) setSheetOpen(true);
    if (delta > 30) setSheetOpen(false);
  }, []);

  if (loading) {
    return (
      <div className="p-4 text-gray-400 text-sm">{t('sidebar.loading')}</div>
    );
  }

  // Group by category
  const grouped = redactions.reduce(
    (acc, r) => {
      if (!acc[r.category]) acc[r.category] = [];
      acc[r.category].push(r);
      return acc;
    },
    {} as Record<string, typeof redactions>
  );

  const pendingCount = redactions.filter((r) => r.status === "pending").length;
  const approvedCount = redactions.filter((r) => r.status === "approved").length;

  const handleExport = async (acknowledgeUnscanned = false) => {
    setExporting(true);
    setExportError(null);
    setUnscannedWarning(null);
    setVerifyProgress(null);
    try {
      if (approvedCount === 0) {
        setExportError(t('sidebar.exportErrorNoApproved'));
        return;
      }

      let outputPath: string;

      if (fileType === "docx") {
        // DOCX: the backend rewrites every text-bearing part of the package
        // and verifies the result before returning.
        outputPath = await exportRedactedDocx(documentId, "text_replace");
      } else {
        // PDF: render only the pages that carry approved redactions; the
        // backend burns the boxes into the pixels, replaces those pages,
        // strips metadata and refuses if anything approved would survive.
        const pages = await renderRedactedPages();
        console.log("[Qvault] Exporting redacted PDF:", pages.length, "page(s) rasterized");
        outputPath = await exportRedactedPdf(documentId, pages, acknowledgeUnscanned);
      }

      console.log("[Qvault] Exported to:", outputPath);
      onExportSuccess?.(outputPath);
      await openFile(outputPath).catch((e) =>
        console.error("Failed to open exported file:", e)
      );
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      const m = /^OCR_UNSCANNED:([\d,]*)\|/.exec(msg);
      if (m) {
        const pages = m[1].split(",").filter(Boolean).map((p) => Number(p) + 1);
        setUnscannedWarning({ pages, message: msg.slice(m[0].length) });
      } else {
        console.error("Export failed:", e);
        setExportError(msg);
      }
    } finally {
      setExporting(false);
      setVerifyProgress(null);
    }
  };

  // Inner content shared between mobile and desktop
  const content = (
    <div className="flex flex-col h-full">
      {/* Header */}
      <div className="p-4 border-b border-gray-200">
        <h3 className="font-semibold text-gray-900 mb-2">{t('sidebar.title')}</h3>
        <div className="flex gap-3 text-xs text-gray-500">
          <span>{t('sidebar.total', { count: redactions.length })}</span>
          <span>{t('sidebar.pending', { count: pendingCount })}</span>
          <span className="text-green-600">{t('sidebar.approved', { count: approvedCount })}</span>
        </div>

        <div className="mt-3 flex gap-2">
          <button
            onClick={() => bulkApprove()}
            disabled={pendingCount === 0}
            className="flex-1 px-3 py-1.5 bg-green-600 text-white text-sm rounded-lg hover:bg-green-700 transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
          >
            {t('sidebar.approveAll')}
          </button>
          <button
            onClick={() => handleExport(false)}
            disabled={exporting || approvedCount === 0}
            className="flex-1 px-3 py-1.5 bg-blue-600 text-white text-sm rounded-lg hover:bg-blue-700 transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
          >
            {exporting ? t('sidebar.exporting') : t('sidebar.exportClean')}
          </button>
        </div>

        {exporting && verifyProgress && verifyProgress.done < verifyProgress.total && (
          <p className="mt-2 text-xs text-gray-500">
            {t('sidebar.verifyingOcr', { current: verifyProgress.done + 1, total: verifyProgress.total })}
          </p>
        )}

        {unscannedWarning && (
          <div className="mt-2 p-2 bg-amber-50 border border-amber-300 rounded text-amber-900 text-xs space-y-2" role="alert">
            <p className="font-medium">
              {t('sidebar.unscannedTitle', { count: unscannedWarning.pages.length, pages: unscannedWarning.pages.join(", ") })}
            </p>
            <p>{t('sidebar.unscannedBody')}</p>
            <div className="flex gap-2">
              <button
                onClick={() => handleExport(true)}
                className="px-2 py-1 bg-amber-600 text-white rounded hover:bg-amber-700"
              >
                {t('sidebar.unscannedExportAnyway')}
              </button>
              <button
                onClick={() => setUnscannedWarning(null)}
                className="px-2 py-1 border border-amber-300 rounded hover:bg-amber-100"
              >
                {t('sidebar.unscannedCancel')}
              </button>
            </div>
          </div>
        )}

        {exportError && (
          <div className="mt-2 p-2 bg-red-50 border border-red-200 rounded text-red-600 text-xs">
            {exportError}
          </div>
        )}

        <button
          onClick={onToggleAdvanced}
          className={`mt-2 w-full px-3 py-1.5 text-sm rounded-lg border transition-colors ${
            advancedMode
              ? "bg-purple-50 border-purple-300 text-purple-700"
              : "border-gray-200 text-gray-600 hover:bg-gray-50"
          }`}
        >
          {advancedMode ? t('sidebar.exitAdvanced') : t('sidebar.enterAdvanced')}
        </button>
      </div>

      {/* Redaction List */}
      <div className="flex-1 overflow-auto">
        {Object.entries(grouped).map(([category, items]) => {
          const color = PII_COLORS[category] || "#6B7280";
          return (
            <div key={category} className="border-b border-gray-100">
              <div className="px-4 py-2 bg-gray-50 flex items-center justify-between">
                <div className="flex items-center gap-2">
                  <span
                    className="w-2.5 h-2.5 rounded-full"
                    style={{ backgroundColor: color }}
                  />
                  <span className="text-xs font-medium text-gray-700">
                    {piiCategoryLabel(category as PiiCategory, t)}
                  </span>
                </div>
                <button
                  onClick={() => bulkApprove(category as PiiCategory)}
                  className="text-xs text-green-600 hover:text-green-800"
                >
                  {t('sidebar.approveCount', { count: items.length })}
                </button>
              </div>

              {items.map((r) => (
                <div
                  key={r.id}
                  onClick={() => setSelectedId(r.id)}
                  className={`px-4 py-2 cursor-pointer transition-colors ${
                    selectedId === r.id
                      ? "bg-blue-50"
                      : "hover:bg-gray-50"
                  }`}
                >
                  <div className="flex items-center justify-between">
                    <span className="text-sm text-gray-800 font-mono truncate max-w-[180px]">
                      {r.original_text}
                    </span>
                    <div className="flex items-center gap-1 ml-2 shrink-0">
                      {editingId === r.id ? (
                        <div className="flex gap-1">
                          <input
                            value={editValue}
                            onChange={(e) => setEditValue(e.target.value)}
                            className="w-24 px-1 py-0.5 text-xs border rounded"
                            autoFocus
                            onKeyDown={(e) => {
                              if (e.key === "Enter") {
                                updateRedaction(r.id, "approved", editValue);
                                setEditingId(null);
                              }
                              if (e.key === "Escape") setEditingId(null);
                            }}
                          />
                        </div>
                      ) : (
                        <>
                          <button
                            onClick={(e) => {
                              e.stopPropagation();
                              updateRedaction(r.id, "approved");
                            }}
                            className={`px-2 py-0.5 text-xs rounded transition-colors ${
                              r.status === "approved"
                                ? "bg-green-100 text-green-700"
                                : "text-gray-500 hover:bg-green-50 hover:text-green-600"
                            }`}
                            title={t('sidebar.approve')}
                          >
                            &#10003;
                          </button>
                          <button
                            onClick={(e) => {
                              e.stopPropagation();
                              updateRedaction(r.id, "rejected");
                            }}
                            className={`px-2 py-0.5 text-xs rounded transition-colors ${
                              r.status === "rejected"
                                ? "bg-red-100 text-red-700"
                                : "text-gray-500 hover:bg-red-50 hover:text-red-600"
                            }`}
                            title={t('sidebar.reject')}
                          >
                            &#10005;
                          </button>
                          <button
                            onClick={(e) => {
                              e.stopPropagation();
                              setEditingId(r.id);
                              setEditValue(r.replacement);
                            }}
                            className="px-2 py-0.5 text-xs text-gray-500 hover:bg-gray-100 rounded transition-colors"
                            title={t('sidebar.editReplacement')}
                          >
                            &#9998;
                          </button>
                        </>
                      )}
                    </div>
                  </div>
                  <div className="flex items-center gap-2 mt-0.5">
                    <span className="text-xs text-gray-400">
                      → {r.replacement}
                    </span>
                    <span className="text-xs text-gray-300">
                      {Math.round(r.confidence * 100)}%
                    </span>
                  </div>
                </div>
              ))}
            </div>
          );
        })}
      </div>
    </div>
  );

  if (!mobile) return content;

  // Mobile: slide-up bottom sheet
  return (
    <div
      ref={sheetRef}
      className={`fixed bottom-16 left-0 right-0 z-30 bg-white rounded-t-2xl shadow-2xl border-t border-gray-200 transition-transform duration-300 ease-out ${
        sheetOpen ? "translate-y-0" : "translate-y-[calc(100%-3rem)]"
      }`}
      style={{ maxHeight: "60vh" }}
    >
      {/* Drag handle */}
      <div
        className="flex items-center justify-center py-2 cursor-grab active:cursor-grabbing"
        onTouchStart={handleDragStart}
        onTouchEnd={handleDragEnd}
        onMouseDown={handleDragStart}
        onMouseUp={handleDragEnd}
        onClick={() => setSheetOpen((v) => !v)}
      >
        <div className="w-10 h-1 rounded-full bg-gray-300" />
      </div>
      <div className="overflow-auto" style={{ maxHeight: "calc(60vh - 2rem)" }}>
        {content}
      </div>
    </div>
  );
}
