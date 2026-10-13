import { useCallback, useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { open, save } from "@tauri-apps/plugin-dialog";
import {
  getNotPersonalData,
  addNotPersonalData,
  removeNotPersonalData,
  exportNotPersonalData,
  previewNotPersonalDataImport,
  applyNotPersonalDataImport,
  getNotPersonalDataCandidates,
  getLearningSummary,
  clearReviewMemory,
} from "../lib/tauri-commands";
import { ALL_PII_CATEGORIES, piiCategoryLabel } from "../lib/types";
import type { NotPersonalDataItem, ListCandidate, ListImportPreview, LearningSummary, PiiCategory } from "../lib/types";

/**
 * Settings → Learning from your reviews: the "Not personal data" list (view,
 * search, add, remove, export, import with a preview) and suggestions for it,
 * plus the decision memory. Items only ever get on the list by a click here
 * or in the review sidebar.
 */
export default function ReviewLearningSettings() {
  const { t } = useTranslation();
  const [items, setItems] = useState<NotPersonalDataItem[]>([]);
  const [candidates, setCandidates] = useState<ListCandidate[]>([]);
  const [summary, setSummary] = useState<LearningSummary | null>(null);
  const [query, setQuery] = useState("");
  const [newText, setNewText] = useState("");
  const [newCategory, setNewCategory] = useState<PiiCategory>("PersonName");
  const [preview, setPreview] = useState<ListImportPreview | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const reload = useCallback(async () => {
    try {
      const [list, cands, sum] = await Promise.all([getNotPersonalData(), getNotPersonalDataCandidates(), getLearningSummary()]);
      setItems(list ?? []);
      setCandidates(cands ?? []);
      setSummary(sum ?? null);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  }, []);

  useEffect(() => {
    reload();
  }, [reload]);

  const run = async (f: () => Promise<string | null | void>) => {
    setError(null);
    setMessage(null);
    try {
      const m = await f();
      if (m) setMessage(m);
      await reload();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  };

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (!q) return items;
    return items.filter(
      (i) => i.text.toLowerCase().includes(q) || piiCategoryLabel(i.category, t).toLowerCase().includes(q) || (i.note ?? "").toLowerCase().includes(q)
    );
  }, [items, query, t]);

  const handleAdd = () =>
    run(async () => {
      if (!newText.trim()) return;
      await addNotPersonalData(newCategory, newText, "settings");
      setNewText("");
      return t("learning.added");
    });

  const handleExport = () =>
    run(async () => {
      const path = await save({ defaultPath: "qvault-not-personal-data.json", filters: [{ name: "JSON", extensions: ["json"] }] });
      if (!path) return;
      const n = await exportNotPersonalData(path);
      return t("learning.exported", { count: n });
    });

  const handleImportPick = () =>
    run(async () => {
      setPreview(null);
      const path = await open({ multiple: false, filters: [{ name: "JSON", extensions: ["json"] }] });
      if (!path || Array.isArray(path)) return;
      setPreview(await previewNotPersonalDataImport(path));
    });

  const handleImportApply = () =>
    run(async () => {
      if (!preview) return;
      const n = await applyNotPersonalDataImport(preview.newItems);
      setPreview(null);
      return t("learning.imported", { count: n });
    });

  const handleClearMemory = () =>
    run(async () => {
      if (!window.confirm(t("learning.forgetConfirm"))) return;
      const n = await clearReviewMemory();
      return t("learning.forgotten", { count: n });
    });

  return (
    <div className="space-y-5">
      <p className="text-sm text-gray-600">{t("learning.intro")}</p>
      <p className="text-xs text-gray-500">{t("learning.storage")}</p>

      {summary && (
        <p className="text-xs text-gray-500">
          {t("learning.summaryDecisions", { count: summary.decisions })} · {t("learning.summaryRejected", { count: summary.rejectedStrings })} ·{" "}
          {t("learning.summaryList", { count: summary.listItems })}
        </p>
      )}

      {message && <div className="p-2 bg-green-50 border border-green-200 rounded text-green-800 text-xs">{message}</div>}
      {error && <div className="p-2 bg-red-50 border border-red-200 rounded text-red-600 text-xs">{error}</div>}

      {/* Suggestions: strings rejected several times */}
      {candidates.length > 0 && (
        <div>
          <h4 className="text-sm font-medium text-gray-700 mb-1">{t("learning.candidatesTitle")}</h4>
          <p className="text-xs text-gray-500 mb-2">{t("learning.candidatesHint")}</p>
          <ul className="divide-y divide-gray-100 border border-gray-200 rounded-lg">
            {candidates.map((c) => (
              <li key={`${c.category}|${c.text}`} className="px-3 py-2 flex items-center justify-between gap-2 text-sm">
                <span className="min-w-0">
                  <span className="font-mono truncate">{c.text}</span>
                  <span className="text-xs text-gray-500">
                    {" "}· {piiCategoryLabel(c.category, t)} · {t("learning.candidateRejected", { count: c.rejected, documents: c.rejectedDocuments })}
                  </span>
                </span>
                <button
                  onClick={() => run(async () => { await addNotPersonalData(c.category, c.text, "suggestion"); return t("learning.added"); })}
                  className="shrink-0 px-2 py-1 text-xs bg-indigo-50 text-indigo-700 rounded hover:bg-indigo-100"
                >
                  {t("learning.addToList")}
                </button>
              </li>
            ))}
          </ul>
        </div>
      )}

      {/* The list */}
      <div>
        <h4 className="text-sm font-medium text-gray-700 mb-1">{t("learning.listTitle")}</h4>
        <p className="text-xs text-gray-500 mb-2">{t("learning.listHint")}</p>
        <div className="flex gap-2 mb-2">
          <input
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder={t("learning.search")}
            className="flex-1 px-3 py-1.5 border border-gray-300 rounded-lg text-sm"
          />
          <button onClick={handleExport} disabled={items.length === 0} className="px-3 py-1.5 text-sm border border-gray-300 rounded-lg hover:bg-gray-50 disabled:opacity-50">
            {t("learning.export")}
          </button>
          <button onClick={handleImportPick} className="px-3 py-1.5 text-sm border border-gray-300 rounded-lg hover:bg-gray-50">
            {t("learning.import")}
          </button>
        </div>

        {preview && (
          <div className="mb-3 p-3 bg-amber-50 border border-amber-200 rounded-lg text-xs text-amber-900 space-y-2" role="dialog">
            <p className="font-medium">{t("learning.previewTitle", { count: preview.newItems.length })}</p>
            {preview.newItems.length > 0 && (
              <ul className="max-h-40 overflow-auto bg-white border border-amber-100 rounded p-2 space-y-0.5">
                {preview.newItems.map((i) => (
                  <li key={`${i.category}|${i.text}`}>
                    <span className="font-mono">{i.text}</span> · {piiCategoryLabel(i.category, t)}
                    {i.note ? <span className="text-gray-500"> · {i.note}</span> : null}
                  </li>
                ))}
              </ul>
            )}
            {(preview.alreadyListed > 0 || preview.duplicatesInFile > 0) && (
              <p>
                {t("learning.previewSkipped", { count: preview.alreadyListed + preview.duplicatesInFile })}
              </p>
            )}
            {preview.invalid.length > 0 && (
              <div>
                <p>{t("learning.previewInvalid", { count: preview.invalid.length })}</p>
                <ul className="list-disc ml-4">
                  {preview.invalid.slice(0, 20).map((i) => (
                    <li key={i.index}>
                      #{i.index} {i.text ? <span className="font-mono">{i.text}</span> : null}: {i.reason}
                    </li>
                  ))}
                </ul>
              </div>
            )}
            <div className="flex gap-2">
              <button
                onClick={handleImportApply}
                disabled={preview.newItems.length === 0}
                className="px-2 py-1 bg-amber-600 text-white rounded hover:bg-amber-700 disabled:opacity-50"
              >
                {t("learning.previewApply", { count: preview.newItems.length })}
              </button>
              <button onClick={() => setPreview(null)} className="px-2 py-1 border border-amber-300 rounded hover:bg-amber-100">
                {t("learning.cancel")}
              </button>
            </div>
          </div>
        )}

        {items.length === 0 ? (
          <p className="text-xs text-gray-400 py-2">{t("learning.empty")}</p>
        ) : (
          <ul className="divide-y divide-gray-100 border border-gray-200 rounded-lg max-h-72 overflow-auto">
            {filtered.map((i) => (
              <li key={i.id} className="px-3 py-2 flex items-center justify-between gap-2 text-sm">
                <span className="min-w-0">
                  <span className="font-mono">{i.text}</span>
                  <span className="text-xs text-gray-500">
                    {" "}· {piiCategoryLabel(i.category, t)} · {t(`learning.source.${i.source}`)}
                    {i.suppressed > 0 ? <> · {t("learning.itemSuppressed", { count: i.suppressed })}</> : null}
                    {i.note ? <> · {i.note}</> : null}
                  </span>
                </span>
                <button
                  onClick={() => run(async () => { await removeNotPersonalData(i.id); return t("learning.removed"); })}
                  className="shrink-0 px-2 py-1 text-xs text-red-600 hover:bg-red-50 rounded"
                >
                  {t("learning.remove")}
                </button>
              </li>
            ))}
            {filtered.length === 0 && <li className="px-3 py-2 text-xs text-gray-400">{t("learning.noMatch")}</li>}
          </ul>
        )}

        <div className="flex gap-2 mt-2">
          <input
            value={newText}
            onChange={(e) => setNewText(e.target.value)}
            onKeyDown={(e) => { if (e.key === "Enter") handleAdd(); }}
            placeholder={t("learning.newPlaceholder")}
            maxLength={200}
            className="flex-1 px-3 py-1.5 border border-gray-300 rounded-lg text-sm"
          />
          <select
            value={newCategory}
            onChange={(e) => setNewCategory(e.target.value as PiiCategory)}
            className="px-2 py-1.5 border border-gray-300 rounded-lg text-sm"
          >
            {ALL_PII_CATEGORIES.map((c) => (
              <option key={c} value={c}>{piiCategoryLabel(c, t)}</option>
            ))}
          </select>
          <button onClick={handleAdd} disabled={!newText.trim()} className="px-3 py-1.5 text-sm bg-indigo-600 text-white rounded-lg hover:bg-indigo-700 disabled:opacity-50">
            {t("learning.add")}
          </button>
        </div>
      </div>

      {/* Decision memory */}
      <div>
        <h4 className="text-sm font-medium text-gray-700 mb-1">{t("learning.memoryTitle")}</h4>
        <p className="text-xs text-gray-500 mb-2">{t("learning.memoryHint")}</p>
        <button onClick={handleClearMemory} className="px-3 py-1.5 text-sm bg-red-50 text-red-600 rounded-lg hover:bg-red-100">
          {t("learning.forget")}
        </button>
      </div>
    </div>
  );
}
