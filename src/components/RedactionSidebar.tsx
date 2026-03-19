import { useState } from "react";
import { useTranslation } from "react-i18next";
import { exportClean } from "../lib/tauri-commands";
import type { RedactionRect } from "../lib/tauri-commands";
import type { UseRedactionsReturn } from "../hooks/useRedactions";
import { PII_COLORS, piiCategoryLabel } from "../lib/types";
import type { PiiCategory } from "../lib/types";

interface Props {
  documentId: string;
  getPdfRects: () => RedactionRect[];
  advancedMode: boolean;
  onToggleAdvanced: () => void;
  redactionsHook: UseRedactionsReturn;
  onExportSuccess?: (outputPath: string) => void;
}

export default function RedactionSidebar({
  documentId,
  getPdfRects,
  advancedMode,
  onToggleAdvanced,
  redactionsHook,
  onExportSuccess,
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
  const [editingId, setEditingId] = useState<string | null>(null);
  const [editValue, setEditValue] = useState("");

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

  const handleExport = async () => {
    setExporting(true);
    setExportError(null);
    try {
      const rects = getPdfRects();
      console.log("[Qvault] Exporting with", rects.length, "redaction rects");
      if (rects.length === 0) {
        setExportError(t('sidebar.exportErrorNoApproved'));
        return;
      }
      const outputPath = await exportClean(documentId, rects);
      console.log("[Qvault] Exported to:", outputPath);
      onExportSuccess?.(outputPath);
    } catch (e) {
      console.error("Export failed:", e);
      setExportError(e instanceof Error ? e.message : String(e));
    } finally {
      setExporting(false);
    }
  };

  return (
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
            onClick={handleExport}
            disabled={exporting || approvedCount === 0}
            className="flex-1 px-3 py-1.5 bg-blue-600 text-white text-sm rounded-lg hover:bg-blue-700 transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
          >
            {exporting ? t('sidebar.exporting') : t('sidebar.exportClean')}
          </button>
        </div>

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
}
