import { useState, useRef, useCallback, useEffect } from "react";
import { useTranslation } from "react-i18next";
import {
  exportRedactedDocx,
  exportRedactedPdf,
  openFile,
  getRedactionNotes,
  getDocumentLearning,
  rejectPreviouslyRejected,
  restoreSuppressedFinding,
  addNotPersonalData,
} from "../lib/tauri-commands";
import type { UseRedactionsReturn } from "../hooks/useRedactions";
import type { RenderRedactedPages } from "../App";
import { PII_COLORS, piiCategoryLabel } from "../lib/types";
import type { PiiCategory, RedactionNote, DocumentLearning, RedactionMark } from "../lib/types";

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
    reload,
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
  // Why each known-entity suggestion was made (loaded with the list).
  const [notes, setNotes] = useState<Record<string, RedactionNote>>({});
  const knownCount = redactions.filter((r) => r.detection_layer === "known-entity").length;
  useEffect(() => {
    if (knownCount === 0) {
      setNotes({});
      return;
    }
    let cancelled = false;
    getRedactionNotes(documentId)
      .then((list) => {
        if (!cancelled) setNotes(Object.fromEntries((list ?? []).map((n) => [n.redactionId, n])));
      })
      .catch((e) => console.warn("[Qvault] redaction notes unavailable:", e));
    return () => { cancelled = true; };
  }, [documentId, knownCount]);
  // Review memory (§13): findings rejected before (marked, still pending)
  // and findings the "Not personal data" list set aside in this document.
  const [learning, setLearning] = useState<DocumentLearning>({ marks: [], suppressed: [] });
  const [showSuppressed, setShowSuppressed] = useState(false);
  const [listedNow, setListedNow] = useState<Record<string, boolean>>({});
  const [learningError, setLearningError] = useState<string | null>(null);
  const reviewedKey = redactions.map((r) => r.status[0]).join("");
  const loadLearning = useCallback(() => {
    getDocumentLearning(documentId)
      .then((l) => setLearning(l ?? { marks: [], suppressed: [] }))
      .catch((e) => console.warn("[Qvault] review memory unavailable:", e));
  }, [documentId]);
  useEffect(() => {
    setShowSuppressed(false);
    setListedNow({});
    setLearningError(null);
  }, [documentId]);
  useEffect(() => {
    loadLearning();
  }, [loadLearning, redactions.length, reviewedKey]);
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let cancelled = false;
    (async () => {
      try {
        const { listen } = await import("@tauri-apps/api/event");
        const u = await listen<{ documentId: string }>("review-memory", (e) => {
          if (e.payload.documentId === documentId) loadLearning();
        });
        if (cancelled) u(); else unlisten = u;
      } catch {
        // Not running inside Tauri (browser preview).
      }
    })();
    return () => { cancelled = true; unlisten?.(); };
  }, [documentId, loadLearning]);
  const marks: Record<string, RedactionMark> = Object.fromEntries(learning.marks.map((m) => [m.redactionId, m]));
  const markedPending = redactions.filter((r) => r.status === "pending" && marks[r.id]).length;

  const handleRejectPreviouslyRejected = async () => {
    setLearningError(null);
    try {
      await rejectPreviouslyRejected(documentId);
      await reload();
    } catch (e) {
      setLearningError(e instanceof Error ? e.message : String(e));
    }
  };
  const handleRestore = async (id: string) => {
    setLearningError(null);
    try {
      await restoreSuppressedFinding(id, documentId);
      await reload();
      loadLearning();
    } catch (e) {
      setLearningError(e instanceof Error ? e.message : String(e));
    }
  };
  const handleNeverFlag = async (redactionId: string, category: string, text: string) => {
    setLearningError(null);
    try {
      await addNotPersonalData(category, text, "review");
      setListedNow((m) => ({ ...m, [redactionId]: true }));
    } catch (e) {
      setLearningError(e instanceof Error ? e.message : String(e));
    }
  };

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

        {markedPending > 0 && (
          <div className="mt-2 p-2 bg-slate-50 border border-slate-200 rounded text-slate-700 text-xs space-y-1.5">
            <p>{t('sidebar.rejectedBeforeSummary', { count: markedPending })}</p>
            <button
              onClick={handleRejectPreviouslyRejected}
              className="px-2 py-1 bg-white border border-slate-300 rounded hover:bg-slate-100"
            >
              {t('sidebar.rejectPreviouslyRejected', { count: markedPending })}
            </button>
          </div>
        )}

        {learning.suppressed.length > 0 && (
          <div className="mt-2 p-2 bg-indigo-50 border border-indigo-200 rounded text-indigo-900 text-xs space-y-1.5">
            <p>{t('sidebar.suppressedSummary', { count: learning.suppressed.length })}</p>
            <button
              onClick={() => setShowSuppressed((v) => !v)}
              className="underline hover:no-underline"
            >
              {showSuppressed ? t('sidebar.suppressedHide') : t('sidebar.suppressedShow')}
            </button>
            {showSuppressed && (
              <ul className="space-y-1">
                {learning.suppressed.map((s) => (
                  <li key={s.id} className="flex items-center justify-between gap-2">
                    <span className="truncate">
                      <span className="font-mono">{s.originalText}</span>
                      <span className="text-indigo-500"> · {piiCategoryLabel(s.category, t)} · {t('sidebar.suppressedPage', { page: s.page + 1 })}</span>
                    </span>
                    <button
                      onClick={() => handleRestore(s.id)}
                      className="shrink-0 px-1.5 py-0.5 bg-white border border-indigo-300 rounded hover:bg-indigo-100"
                      title={t('sidebar.suppressedRestoreTitle')}
                    >
                      {t('sidebar.suppressedRestore')}
                    </button>
                  </li>
                ))}
              </ul>
            )}
          </div>
        )}

        {learningError && (
          <div className="mt-2 p-2 bg-red-50 border border-red-200 rounded text-red-600 text-xs">
            {learningError}
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

              {[...items].sort((a, b) => Number(!!marks[a.id]) - Number(!!marks[b.id])).map((r) => (
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
                    {r.detection_layer === "known-entity" && (
                      <span className="text-[10px] uppercase tracking-wide bg-amber-50 text-amber-700 px-1.5 py-0.5 rounded">
                        {t('sidebar.knownEntity')}
                      </span>
                    )}
                    {marks[r.id] && (
                      <span
                        className="text-[10px] uppercase tracking-wide bg-slate-100 text-slate-600 px-1.5 py-0.5 rounded"
                        title={t('sidebar.rejectedBeforeTitle', { count: marks[r.id].rejectedDocuments, confidence: Math.round(marks[r.id].originalConfidence * 100) })}
                      >
                        {t('sidebar.rejectedBefore', { count: marks[r.id].rejected })}
                      </span>
                    )}
                  </div>
                  {r.status === "rejected" && r.detection_layer !== "manual" && (
                    <div className="mt-1 text-xs">
                      {listedNow[r.id] ? (
                        <span className="text-indigo-700">{t('sidebar.neverFlagDone')}</span>
                      ) : (
                        <button
                          onClick={(e) => {
                            e.stopPropagation();
                            handleNeverFlag(r.id, r.category, r.original_text);
                          }}
                          className="text-indigo-600 hover:text-indigo-800 hover:underline"
                          title={t('sidebar.neverFlagTitle', { text: r.original_text, category: piiCategoryLabel(r.category, t) })}
                        >
                          {t('sidebar.neverFlag')}
                        </button>
                      )}
                    </div>
                  )}
                  {r.detection_layer === "known-entity" && notes[r.id] && (
                    <div className="mt-1 text-xs text-gray-500 leading-snug">
                      {t(`sidebar.knownRule.${notes[r.id].rule}`, { source: notes[r.id].sourceText })}
                      {" · "}
                      {notes[r.id].sourceApproved
                        ? t('sidebar.knownApproved', { count: notes[r.id].documents })
                        : t('sidebar.knownUnreviewed')}
                      {notes[r.id].groupLabel && notes[r.id].groupLabel !== notes[r.id].sourceText && (
                        <> · {t('sidebar.knownGroup', { label: notes[r.id].groupLabel })}</>
                      )}
                    </div>
                  )}
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
