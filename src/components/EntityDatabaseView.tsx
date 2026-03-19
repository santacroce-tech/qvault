import { useState, useEffect, useCallback } from "react";
import { useTranslation } from "react-i18next";
import { getEntitySummary, getHistory } from "../lib/tauri-commands";
import type { EntitySummary, Document } from "../lib/types";
import { PII_COLORS, piiCategoryLabel } from "../lib/types";
import type { PiiCategory } from "../lib/types";

interface Props {
  onOpenDocument: (docId: string) => void;
}

type SortField = "text" | "category" | "occurrences" | "documents";
type SortDir = "asc" | "desc";

export default function EntityDatabaseView({ onOpenDocument }: Props) {
  const { t, i18n } = useTranslation();
  const [entities, setEntities] = useState<EntitySummary[]>([]);
  const [documents, setDocuments] = useState<Document[]>([]);
  const [loading, setLoading] = useState(true);
  const [filter, setFilter] = useState("");
  const [categoryFilter, setCategoryFilter] = useState<string>("all");
  const [sortField, setSortField] = useState<SortField>("occurrences");
  const [sortDir, setSortDir] = useState<SortDir>("desc");

  const load = useCallback(async () => {
    setLoading(true);
    try {
      const [ents, docs] = await Promise.all([
        getEntitySummary(),
        getHistory(0, 100),
      ]);
      setEntities(ents);
      setDocuments(docs);
    } catch (e) {
      console.error("Failed to load entity data:", e);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    load();
  }, [load]);

  // Get unique categories
  const categories = Array.from(new Set(entities.map((e) => e.category))).sort();

  // Filter and sort
  const filtered = entities
    .filter((e) => {
      if (categoryFilter !== "all" && e.category !== categoryFilter) return false;
      if (filter && !e.normalized_text.toLowerCase().includes(filter.toLowerCase())) return false;
      return true;
    })
    .sort((a, b) => {
      let cmp = 0;
      switch (sortField) {
        case "text":
          cmp = a.normalized_text.localeCompare(b.normalized_text);
          break;
        case "category":
          cmp = a.category.localeCompare(b.category);
          break;
        case "occurrences":
          cmp = a.occurrence_count - b.occurrence_count;
          break;
        case "documents":
          cmp = a.document_count - b.document_count;
          break;
      }
      return sortDir === "asc" ? cmp : -cmp;
    });

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
    return sortDir === "asc" ? " \u25B2" : " \u25BC";
  };

  // Stats
  const totalEntities = entities.length;
  const totalOccurrences = entities.reduce((s, e) => s + e.occurrence_count, 0);
  const highRiskCategories = ["SocialSecurityNumber", "CreditCard", "BankAccount", "PassportNumber", "DriversLicense", "IBAN", "MedicalRecordNumber"];
  const highRiskCount = entities.filter((e) => highRiskCategories.includes(e.category)).reduce((s, e) => s + e.occurrence_count, 0);

  return (
    <div className="flex-1 p-6 bg-content overflow-auto">
      <div className="max-w-6xl mx-auto">
        <h2 className="text-xl font-bold text-gray-900 mb-1">{t('entities.title')}</h2>
        <p className="text-sm text-gray-500 mb-6">
          {t('entities.subtitle')}
        </p>

        {/* Stats Cards */}
        <div className="grid grid-cols-4 gap-4 mb-6">
          <div className="bg-white rounded-lg border border-gray-200 p-4">
            <div className="text-2xl font-bold text-gray-900">{documents.length}</div>
            <div className="text-xs text-gray-500 mt-1">{t('entities.documentsProcessed')}</div>
          </div>
          <div className="bg-white rounded-lg border border-gray-200 p-4">
            <div className="text-2xl font-bold text-blue-600">{totalEntities}</div>
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
                const count = entities.filter((e) => e.category === cat).reduce((s, e) => s + e.occurrence_count, 0);
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

        {/* Search + Table */}
        <div className="bg-white rounded-lg border border-gray-200 overflow-hidden">
          <div className="p-4 border-b border-gray-200">
            <input
              type="text"
              value={filter}
              onChange={(e) => setFilter(e.target.value)}
              placeholder={t('entities.searchEntities')}
              className="w-full px-3 py-2 border border-gray-300 rounded-lg text-sm focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-transparent"
            />
          </div>

          {loading ? (
            <div className="p-8 text-center text-gray-400 text-sm">{t('entities.loading')}</div>
          ) : filtered.length === 0 ? (
            <div className="p-8 text-center text-gray-400 text-sm">
              {entities.length === 0
                ? t('entities.emptyNoData')
                : t('entities.emptyNoMatch')
              }
            </div>
          ) : (
            <table className="w-full text-sm">
              <thead>
                <tr className="border-b border-gray-200 bg-gray-50">
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
                {filtered.map((entity, idx) => {
                  const color = PII_COLORS[entity.category] || "#6B7280";
                  const isHighRisk = highRiskCategories.includes(entity.category);
                  return (
                    <tr
                      key={`${entity.normalized_text}-${entity.category}-${idx}`}
                      className="border-b border-gray-100 hover:bg-gray-50"
                    >
                      <td className="px-4 py-3">
                        <span className="font-mono text-gray-900">{entity.normalized_text}</span>
                      </td>
                      <td className="px-4 py-3">
                        <span className="inline-flex items-center gap-1.5">
                          <span
                            className="w-2 h-2 rounded-full"
                            style={{ backgroundColor: color }}
                          />
                          <span className="text-gray-600">
                            {piiCategoryLabel(entity.category as PiiCategory, t)}
                          </span>
                        </span>
                      </td>
                      <td className="px-4 py-3 text-right text-gray-600 font-medium">
                        {entity.occurrence_count}
                      </td>
                      <td className="px-4 py-3 text-right text-gray-600">
                        {entity.document_count}
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
                    </tr>
                  );
                })}
              </tbody>
            </table>
          )}

          {filtered.length > 0 && (
            <div className="px-4 py-3 border-t border-gray-200 text-xs text-gray-500 bg-gray-50">
              {t('entities.showing', { shown: filtered.length, total: entities.length })}
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
                  onClick={() => onOpenDocument(doc.id)}
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
