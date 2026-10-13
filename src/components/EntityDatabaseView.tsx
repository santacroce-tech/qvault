import { useState, useEffect, useCallback, useMemo } from "react";
import { useTranslation } from "react-i18next";
import {
  getHistory,
  regroupEntities,
  getEntityGroups,
  getEntityProposals,
  getEntityGroupDocuments,
  mergeEntityGroups,
  splitEntityVariant,
  dismissEntityProposal,
} from "../lib/tauri-commands";
import type { Document, EntityGroup, EntityProposal, EntityGroupDocument } from "../lib/types";
import { PII_COLORS, piiCategoryLabel } from "../lib/types";
import type { PiiCategory } from "../lib/types";

interface Props {
  onOpenDocument: (docId: string, fileType?: string) => void;
}

type SortField = "text" | "category" | "occurrences" | "documents";
type SortDir = "asc" | "desc";

const HIGH_RISK = ["SocialSecurityNumber", "CreditCard", "BankAccount", "PassportNumber", "DriversLicense", "IBAN", "MedicalRecordNumber"];

export default function EntityDatabaseView({ onOpenDocument }: Props) {
  const { t, i18n } = useTranslation();
  const [groups, setGroups] = useState<EntityGroup[]>([]);
  const [proposals, setProposals] = useState<EntityProposal[]>([]);
  const [documents, setDocuments] = useState<Document[]>([]);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [filter, setFilter] = useState("");
  const [categoryFilter, setCategoryFilter] = useState<string>("all");
  const [sortField, setSortField] = useState<SortField>("occurrences");
  const [sortDir, setSortDir] = useState<SortDir>("desc");
  const [expanded, setExpanded] = useState<string | null>(null);
  const [groupDocs, setGroupDocs] = useState<Record<string, EntityGroupDocument[]>>({});
  const [selected, setSelected] = useState<string[]>([]);

  const reload = useCallback(async () => {
    const [g, p, docs] = await Promise.all([getEntityGroups(), getEntityProposals(), getHistory(0, 100)]);
    setGroups(g ?? []);
    setProposals(p ?? []);
    setDocuments(docs ?? []);
    setGroupDocs({});
  }, []);

  const load = useCallback(async () => {
    setLoading(true);
    try {
      // Fast regroup (rules + stored vectors) so new detections are grouped.
      await regroupEntities(false);
      await reload();
    } catch (e) {
      console.error("Failed to load entity data:", e);
    } finally {
      setLoading(false);
    }
  }, [reload]);

  useEffect(() => {
    load();
  }, [load]);

  // A scan finished grouping in the background.
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let cancelled = false;
    (async () => {
      try {
        const { listen } = await import("@tauri-apps/api/event");
        const u = await listen("entities-updated", () => { reload().catch(() => {}); });
        if (cancelled) u(); else unlisten = u;
      } catch {
        // Not running inside Tauri (browser preview).
      }
    })();
    return () => { cancelled = true; unlisten?.(); };
  }, [reload]);

  const act = async (key: string, fn: () => Promise<unknown>) => {
    setBusy(key);
    setError(null);
    try {
      await fn();
      await reload();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(null);
    }
  };

  const byId = useMemo(() => new Map(groups.map((g) => [g.id, g])), [groups]);
  const categories = Array.from(new Set(groups.map((g) => g.category))).sort();

  const filtered = groups
    .filter((g) => {
      if (categoryFilter !== "all" && g.category !== categoryFilter) return false;
      if (filter) {
        const f = filter.toLowerCase();
        if (!g.label.toLowerCase().includes(f) && !g.variants.some((v) => v.text.toLowerCase().includes(f))) return false;
      }
      return true;
    })
    .sort((a, b) => {
      let cmp = 0;
      switch (sortField) {
        case "text":
          cmp = a.label.localeCompare(b.label);
          break;
        case "category":
          cmp = a.category.localeCompare(b.category);
          break;
        case "occurrences":
          cmp = a.occurrences - b.occurrences;
          break;
        case "documents":
          cmp = a.documents - b.documents;
          break;
      }
      return sortDir === "asc" ? cmp : -cmp;
    });

  const visibleProposals = proposals.filter((p) => categoryFilter === "all" || p.category === categoryFilter);

  const handleSort = (field: SortField) => {
    if (sortField === field) {
      setSortDir((d) => (d === "asc" ? "desc" : "asc"));
    } else {
      setSortField(field);
      setSortDir("desc");
    }
  };

  const sortIcon = (field: SortField) => {
    if (sortField !== field) return "";
    return sortDir === "asc" ? " ▲" : " ▼";
  };

  const toggleExpand = async (g: EntityGroup) => {
    if (expanded === g.id) {
      setExpanded(null);
      return;
    }
    setExpanded(g.id);
    if (!groupDocs[g.id]) {
      try {
        const docs = await getEntityGroupDocuments(g.id);
        setGroupDocs((m) => ({ ...m, [g.id]: docs }));
      } catch (e) {
        console.error("Failed to load documents for entity:", e);
      }
    }
  };

  const toggleSelect = (id: string) =>
    setSelected((s) => (s.includes(id) ? s.filter((x) => x !== id) : [...s, id]));

  const selectedGroups = selected.map((id) => byId.get(id)).filter(Boolean) as EntityGroup[];
  const sameCategory = selectedGroups.length > 0 && selectedGroups.every((g) => g.category === selectedGroups[0].category);

  const mergeSelected = () =>
    act("merge-selected", async () => {
      // Keep the group with the most occurrences as the target.
      const sorted = [...selectedGroups].sort((a, b) => b.occurrences - a.occurrences);
      await mergeEntityGroups(sorted[0].id, sorted.slice(1).map((g) => g.id));
      setSelected([]);
    });

  const acceptProposal = (p: EntityProposal) =>
    act(`accept-${p.groupA}-${p.groupB}`, async () => {
      const a = byId.get(p.groupA);
      const b = byId.get(p.groupB);
      const [target, other] = (a?.occurrences ?? 0) >= (b?.occurrences ?? 0) ? [p.groupA, p.groupB] : [p.groupB, p.groupA];
      await mergeEntityGroups(target, [other]);
    });

  const dismissProposal = (p: EntityProposal) =>
    act(`dismiss-${p.groupA}-${p.groupB}`, () => dismissEntityProposal(p.groupA, p.groupB));

  const regroup = () =>
    act("regroup", async () => {
      const rep = await regroupEntities(true);
      if (rep.embeddingError) setError(t("entities.regroupEmbeddingFailed"));
    });

  // Stats
  const totalOccurrences = groups.reduce((s, g) => s + g.occurrences, 0);
  const highRiskCount = groups.filter((g) => HIGH_RISK.includes(g.category)).reduce((s, g) => s + g.occurrences, 0);

  const ruleText = (p: EntityProposal) => t(`entities.rule.${p.rule}`);

  return (
    <div className="flex-1 p-6 bg-content overflow-auto">
      <div className="max-w-6xl mx-auto">
        <h2 className="text-xl font-bold text-gray-900 mb-1">{t('entities.title')}</h2>
        <p className="text-sm text-gray-500 mb-6">
          {t('entities.subtitle')}
        </p>

        {/* Stats Cards */}
        <div className="grid grid-cols-2 md:grid-cols-4 gap-4 mb-6">
          <div className="bg-white rounded-lg border border-gray-200 p-4">
            <div className="text-2xl font-bold text-gray-900">{documents.length}</div>
            <div className="text-xs text-gray-500 mt-1">{t('entities.documentsProcessed')}</div>
          </div>
          <div className="bg-white rounded-lg border border-gray-200 p-4">
            <div className="text-2xl font-bold text-blue-600">{groups.length}</div>
            <div className="text-xs text-gray-500 mt-1">{t('entities.uniqueEntities')}</div>
          </div>
          <div className="bg-white rounded-lg border border-gray-200 p-4">
            <div className="text-2xl font-bold text-amber-600">{totalOccurrences}</div>
            <div className="text-xs text-gray-500 mt-1">{t('entities.totalOccurrences')}</div>
          </div>
          <div className="bg-white rounded-lg border border-gray-200 p-4">
            <div className="text-2xl font-bold text-red-600">{highRiskCount}</div>
            <div className="text-xs text-gray-500 mt-1">{t('entities.highRiskItems')}</div>
          </div>
        </div>

        {/* Category Breakdown */}
        {categories.length > 0 && (
          <div className="bg-white rounded-lg border border-gray-200 p-4 mb-6">
            <h3 className="text-sm font-semibold text-gray-700 mb-3">{t('entities.categoryBreakdown')}</h3>
            <div className="flex flex-wrap gap-2">
              {categories.map((cat) => {
                const color = PII_COLORS[cat] || "#6B7280";
                const count = groups.filter((g) => g.category === cat).reduce((s, g) => s + g.occurrences, 0);
                return (
                  <button
                    key={cat}
                    onClick={() => setCategoryFilter(categoryFilter === cat ? "all" : cat)}
                    className={`inline-flex items-center gap-1.5 px-3 py-1.5 rounded-full text-xs font-medium transition-colors border ${
                      categoryFilter === cat
                        ? "border-gray-400 bg-gray-100"
                        : "border-gray-200 hover:bg-gray-50"
                    }`}
                  >
                    <span
                      className="w-2 h-2 rounded-full"
                      style={{ backgroundColor: color }}
                    />
                    {piiCategoryLabel(cat as PiiCategory, t)} ({count})
                  </button>
                );
              })}
              {categoryFilter !== "all" && (
                <button
                  onClick={() => setCategoryFilter("all")}
                  className="px-3 py-1.5 rounded-full text-xs font-medium text-gray-500 hover:text-gray-700 border border-dashed border-gray-300"
                >
                  {t('entities.clearFilter')}
                </button>
              )}
            </div>
          </div>
        )}

        {/* Suggested merges: nothing is merged until the user confirms */}
        {visibleProposals.length > 0 && (
          <div className="bg-white rounded-lg border border-amber-200 mb-6 overflow-hidden">
            <div className="px-4 py-3 border-b border-amber-100 bg-amber-50">
              <h3 className="text-sm font-semibold text-amber-900">
                {t('entities.suggestions', { count: visibleProposals.length })}
              </h3>
              <p className="text-xs text-amber-800 mt-0.5">{t('entities.suggestionsHelp')}</p>
            </div>
            <ul className="divide-y divide-gray-100">
              {visibleProposals.map((p) => {
                const key = `${p.groupA}-${p.groupB}`;
                return (
                  <li key={key} className="px-4 py-3 flex flex-col md:flex-row md:items-center md:justify-between gap-2">
                    <div className="min-w-0">
                      <div className="text-sm text-gray-900 break-words">
                        <span className="font-mono">{p.variantA}</span>
                        <span className="mx-2 text-gray-400">↔</span>
                        <span className="font-mono">{p.variantB}</span>
                        <span className="ml-2 text-xs text-gray-400">{piiCategoryLabel(p.category as PiiCategory, t)}</span>
                      </div>
                      <div className="text-xs text-gray-500 mt-0.5">
                        {ruleText(p)}
                        {p.cosine !== null && <> · {t('entities.similarity', { value: Math.round(p.cosine * 100) })}</>}
                        {(p.labelA !== p.variantA || p.labelB !== p.variantB) && (
                          <> · {t('entities.groupsLabel', { a: p.labelA, b: p.labelB })}</>
                        )}
                      </div>
                      {p.ambiguous && (
                        <div className="text-xs text-amber-700 mt-0.5">{t('entities.ambiguous')}</div>
                      )}
                    </div>
                    <div className="flex gap-2 flex-shrink-0">
                      <button
                        onClick={() => acceptProposal(p)}
                        disabled={busy !== null}
                        className="px-3 py-1.5 text-xs rounded-lg bg-blue-600 text-white hover:bg-blue-700 disabled:opacity-50"
                      >
                        {t('entities.merge')}
                      </button>
                      <button
                        onClick={() => dismissProposal(p)}
                        disabled={busy !== null}
                        className="px-3 py-1.5 text-xs rounded-lg border border-gray-300 text-gray-700 hover:bg-gray-50 disabled:opacity-50"
                      >
                        {t('entities.keepSeparate')}
                      </button>
                    </div>
                  </li>
                );
              })}
            </ul>
          </div>
        )}

        {/* Search + Table */}
        <div className="bg-white rounded-lg border border-gray-200 overflow-hidden">
          <div className="p-4 border-b border-gray-200 flex flex-col md:flex-row gap-2">
            <input
              type="text"
              value={filter}
              onChange={(e) => setFilter(e.target.value)}
              placeholder={t('entities.searchEntities')}
              className="flex-1 min-w-0 px-3 py-2 border border-gray-300 rounded-lg text-sm focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-transparent"
            />
            <button
              onClick={mergeSelected}
              disabled={busy !== null || selectedGroups.length < 2 || !sameCategory}
              title={selectedGroups.length >= 2 && !sameCategory ? t('entities.mergeDifferentCategories') : undefined}
              className="px-3 py-2 text-sm rounded-lg bg-blue-50 text-blue-700 hover:bg-blue-100 disabled:opacity-50 whitespace-nowrap"
            >
              {t('entities.mergeSelected', { count: selectedGroups.length })}
            </button>
            <button
              onClick={regroup}
              disabled={busy !== null}
              title={t('entities.regroupHelp')}
              className="px-3 py-2 text-sm rounded-lg bg-gray-100 text-gray-700 hover:bg-gray-200 disabled:opacity-50 whitespace-nowrap"
            >
              {busy === "regroup" ? t('entities.regrouping') : t('entities.regroup')}
            </button>
          </div>
          {selectedGroups.length >= 2 && !sameCategory && (
            <p className="px-4 pt-2 text-xs text-amber-700">{t('entities.mergeDifferentCategories')}</p>
          )}
          {error && <p className="px-4 pt-2 text-xs text-red-600 break-words">{error}</p>}

          {loading ? (
            <div className="p-8 text-center text-gray-400 text-sm">{t('entities.loading')}</div>
          ) : filtered.length === 0 ? (
            <div className="p-8 text-center text-gray-400 text-sm">
              {groups.length === 0
                ? t('entities.emptyNoData')
                : t('entities.emptyNoMatch')
              }
            </div>
          ) : (
            <div className="overflow-x-auto">
            <table className="w-full text-sm">
              <thead>
                <tr className="border-b border-gray-200 bg-gray-50">
                  <th className="w-8 px-2 py-3" aria-label={t('entities.select')} />
                  <th
                    onClick={() => handleSort("text")}
                    className="text-left px-4 py-3 font-medium text-gray-600 cursor-pointer hover:text-gray-900 select-none"
                  >
                    {t('entities.entity')}{sortIcon("text")}
                  </th>
                  <th
                    onClick={() => handleSort("category")}
                    className="text-left px-4 py-3 font-medium text-gray-600 cursor-pointer hover:text-gray-900 select-none"
                  >
                    {t('entities.category')}{sortIcon("category")}
                  </th>
                  <th
                    onClick={() => handleSort("occurrences")}
                    className="text-right px-4 py-3 font-medium text-gray-600 cursor-pointer hover:text-gray-900 select-none"
                  >
                    {t('entities.occurrences')}{sortIcon("occurrences")}
                  </th>
                  <th
                    onClick={() => handleSort("documents")}
                    className="text-right px-4 py-3 font-medium text-gray-600 cursor-pointer hover:text-gray-900 select-none"
                  >
                    {t('entities.documents')}{sortIcon("documents")}
                  </th>
                  <th className="text-right px-4 py-3 font-medium text-gray-600">{t('entities.risk')}</th>
                </tr>
              </thead>
              <tbody>
                {filtered.map((g) => {
                  const color = PII_COLORS[g.category] || "#6B7280";
                  const isHighRisk = HIGH_RISK.includes(g.category);
                  const open = expanded === g.id;
                  return [
                    <tr key={g.id} className={`border-b border-gray-100 hover:bg-gray-50 ${open ? "bg-gray-50" : ""}`}>
                      <td className="px-2 py-3 text-center">
                        <input
                          type="checkbox"
                          checked={selected.includes(g.id)}
                          onChange={() => toggleSelect(g.id)}
                          aria-label={t('entities.select')}
                          className="w-4 h-4 text-blue-600 rounded focus:ring-blue-500"
                        />
                      </td>
                      <td className="px-4 py-3">
                        <button onClick={() => toggleExpand(g)} className="text-left w-full" aria-expanded={open}>
                          <span className="text-gray-400 mr-1">{open ? "▾" : "▸"}</span>
                          <span className="font-mono text-gray-900">{g.label}</span>
                          {g.variants.length > 1 && (
                            <span className="ml-2 inline-flex items-center px-1.5 py-0.5 rounded text-[11px] bg-blue-50 text-blue-700">
                              {t('entities.variantsCount', { count: g.variants.length })}
                            </span>
                          )}
                          {g.proposals > 0 && (
                            <span className="ml-1 inline-flex items-center px-1.5 py-0.5 rounded text-[11px] bg-amber-50 text-amber-700">
                              {t('entities.suggestionBadge', { count: g.proposals })}
                            </span>
                          )}
                        </button>
                      </td>
                      <td className="px-4 py-3">
                        <span className="inline-flex items-center gap-1.5">
                          <span
                            className="w-2 h-2 rounded-full"
                            style={{ backgroundColor: color }}
                          />
                          <span className="text-gray-600">
                            {piiCategoryLabel(g.category as PiiCategory, t)}
                          </span>
                        </span>
                      </td>
                      <td className="px-4 py-3 text-right text-gray-600 font-medium">
                        {g.occurrences}
                      </td>
                      <td className="px-4 py-3 text-right text-gray-600">
                        {g.documents}
                      </td>
                      <td className="px-4 py-3 text-right">
                        {isHighRisk ? (
                          <span className="inline-flex items-center px-2 py-0.5 rounded-full text-xs font-medium bg-red-100 text-red-700">
                            {t('entities.riskHigh')}
                          </span>
                        ) : (
                          <span className="inline-flex items-center px-2 py-0.5 rounded-full text-xs font-medium bg-gray-100 text-gray-600">
                            {t('entities.riskNormal')}
                          </span>
                        )}
                      </td>
                    </tr>,
                    open && (
                      <tr key={`${g.id}-detail`} className="border-b border-gray-100 bg-gray-50">
                        <td />
                        <td colSpan={5} className="px-4 pb-4">
                          <div className="grid md:grid-cols-2 gap-4">
                            <div>
                              <h4 className="text-xs font-semibold text-gray-500 uppercase tracking-wider mb-1">{t('entities.variants')}</h4>
                              <ul className="space-y-1">
                                {g.variants.map((v) => (
                                  <li key={v.id} className="flex items-center justify-between gap-2 text-sm">
                                    <span className="min-w-0 break-words">
                                      <span className="font-mono text-gray-800">{v.text}</span>
                                      <span className="ml-2 text-xs text-gray-500">
                                        {t('entities.variantStats', { count: v.occurrences, documents: v.documents })}
                                      </span>
                                      {v.pinned && <span className="ml-2 text-xs text-gray-400">· {t('entities.pinned')}</span>}
                                    </span>
                                    {g.variants.length > 1 && (
                                      <button
                                        onClick={() => act(`split-${v.id}`, () => splitEntityVariant(v.id))}
                                        disabled={busy !== null}
                                        title={t('entities.splitHelp')}
                                        className="flex-shrink-0 px-2 py-0.5 text-xs rounded border border-gray-300 text-gray-600 hover:bg-white disabled:opacity-50"
                                      >
                                        {t('entities.split')}
                                      </button>
                                    )}
                                  </li>
                                ))}
                              </ul>
                            </div>
                            <div>
                              <h4 className="text-xs font-semibold text-gray-500 uppercase tracking-wider mb-1">{t('entities.documentsMentioning')}</h4>
                              {!groupDocs[g.id] ? (
                                <p className="text-xs text-gray-400">{t('entities.loading')}</p>
                              ) : groupDocs[g.id].length === 0 ? (
                                <p className="text-xs text-gray-400">{t('entities.noDocuments')}</p>
                              ) : (
                                <ul className="space-y-1">
                                  {groupDocs[g.id].map((d) => (
                                    <li key={d.documentId}>
                                      <button
                                        onClick={() => onOpenDocument(d.documentId, documents.find((x) => x.id === d.documentId)?.file_type)}
                                        className="text-sm text-blue-600 hover:underline text-left break-words"
                                      >
                                        {d.filename}
                                      </button>
                                      <span className="ml-2 text-xs text-gray-400">
                                        {new Date(d.createdAt).toLocaleDateString(i18n.language)} · {t('entities.occurrencesCount', { count: d.occurrences })}
                                      </span>
                                    </li>
                                  ))}
                                </ul>
                              )}
                            </div>
                          </div>
                        </td>
                      </tr>
                    ),
                  ];
                })}
              </tbody>
            </table>
            </div>
          )}

          {filtered.length > 0 && (
            <div className="px-4 py-3 border-t border-gray-200 text-xs text-gray-500 bg-gray-50">
              {t('entities.showing', { shown: filtered.length, total: groups.length })}
            </div>
          )}
        </div>

        {/* Recent Documents */}
        {documents.length > 0 && (
          <div className="mt-6 bg-white rounded-lg border border-gray-200 overflow-hidden">
            <div className="px-4 py-3 border-b border-gray-200 bg-gray-50">
              <h3 className="text-sm font-semibold text-gray-700">{t('entities.recentDocuments')}</h3>
            </div>
            <div className="divide-y divide-gray-100">
              {documents.slice(0, 10).map((doc) => (
                <div
                  key={doc.id}
                  className="px-4 py-3 flex items-center justify-between hover:bg-gray-50 cursor-pointer"
                  onClick={() => onOpenDocument(doc.id, doc.file_type)}
                >
                  <div>
                    <span className="text-sm font-medium text-gray-900">{doc.filename}</span>
                    <span className="ml-3 text-xs text-gray-400">
                      {new Date(doc.created_at).toLocaleDateString(i18n.language)}
                    </span>
                  </div>
                  <div className="flex items-center gap-3">
                    <span className="text-xs text-gray-500">{t('entities.piiCount', { count: doc.pii_count })}</span>
                    <span className={`px-2 py-0.5 text-xs rounded-full ${
                      doc.status === "complete"
                        ? "bg-green-100 text-green-700"
                        : doc.status === "review"
                        ? "bg-blue-100 text-blue-700"
                        : "bg-yellow-100 text-yellow-700"
                    }`}>
                      {t(`status.${doc.status}`)}
                    </span>
                  </div>
                </div>
              ))}
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
