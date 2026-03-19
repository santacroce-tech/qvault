import { useState, useEffect, useCallback } from "react";
import { useTranslation } from "react-i18next";
import { getHistory, searchHistory, deleteDocument } from "../lib/tauri-commands";
import type { Document } from "../lib/types";

interface Props {
  onOpenDocument: (docId: string) => void;
}

export default function HistoryView({ onOpenDocument }: Props) {
  const { t, i18n } = useTranslation();
  const [documents, setDocuments] = useState<Document[]>([]);
  const [query, setQuery] = useState("");
  const [loading, setLoading] = useState(true);

  const load = useCallback(async () => {
    setLoading(true);
    try {
      const docs = query
        ? await searchHistory(query)
        : await getHistory(0, 50);
      setDocuments(docs);
    } catch (e) {
      console.error("Failed to load history:", e);
    } finally {
      setLoading(false);
    }
  }, [query]);

  useEffect(() => {
    load();
  }, [load]);

  const handleDelete = async (id: string) => {
    await deleteDocument(id);
    setDocuments((prev) => prev.filter((d) => d.id !== id));
  };

  const statusBadge = (status: string) => {
    const styles: Record<string, string> = {
      processing: "bg-yellow-100 text-yellow-700",
      review: "bg-blue-100 text-blue-700",
      complete: "bg-green-100 text-green-700",
    };
    return (
      <span className={`px-2 py-0.5 text-xs rounded-full ${styles[status] || "bg-gray-100 text-gray-600"}`}>
        {t(`status.${status}`, status)}
      </span>
    );
  };

  return (
    <div className="flex-1 p-6 bg-content">
      <div className="max-w-4xl mx-auto">
        <h2 className="text-xl font-bold text-gray-900 mb-4">{t('history.title')}</h2>

        <input
          type="text"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          placeholder={t('history.search')}
          className="w-full px-4 py-2 border border-gray-300 rounded-lg mb-4 text-sm focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-transparent"
        />

        {loading ? (
          <div className="text-gray-400 text-sm">{t('history.loading')}</div>
        ) : documents.length === 0 ? (
          <div className="text-gray-400 text-sm text-center py-12">
            {t('history.empty')}
          </div>
        ) : (
          <div className="bg-white rounded-lg border border-gray-200 overflow-hidden">
            <table className="w-full text-sm">
              <thead>
                <tr className="border-b border-gray-200 bg-gray-50">
                  <th className="text-left px-4 py-3 font-medium text-gray-600">{t('history.filename')}</th>
                  <th className="text-left px-4 py-3 font-medium text-gray-600">{t('history.date')}</th>
                  <th className="text-left px-4 py-3 font-medium text-gray-600">{t('history.pii')}</th>
                  <th className="text-left px-4 py-3 font-medium text-gray-600">{t('history.status')}</th>
                  <th className="text-right px-4 py-3 font-medium text-gray-600">{t('history.actions')}</th>
                </tr>
              </thead>
              <tbody>
                {documents.map((doc) => (
                  <tr key={doc.id} className="border-b border-gray-100 hover:bg-gray-50">
                    <td className="px-4 py-3 font-medium text-gray-900">{doc.filename}</td>
                    <td className="px-4 py-3 text-gray-500">
                      {new Date(doc.created_at).toLocaleDateString(i18n.language)}
                    </td>
                    <td className="px-4 py-3 text-gray-500">{doc.pii_count}</td>
                    <td className="px-4 py-3">{statusBadge(doc.status)}</td>
                    <td className="px-4 py-3 text-right">
                      <button
                        onClick={() => onOpenDocument(doc.id)}
                        className="text-blue-600 hover:text-blue-800 text-sm mr-3"
                      >
                        {t('history.open')}
                      </button>
                      <button
                        onClick={() => handleDelete(doc.id)}
                        className="text-red-500 hover:text-red-700 text-sm"
                      >
                        {t('history.delete')}
                      </button>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </div>
    </div>
  );
}
