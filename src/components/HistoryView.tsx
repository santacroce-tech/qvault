import { useState, useEffect, useCallback } from "react";
import { useTranslation } from "react-i18next";
import { getHistory, searchHistory, deleteDocument, openFile } from "../lib/tauri-commands";
import type { Document } from "../lib/types";

interface Props {
  onOpenDocument: (docId: string, fileType?: string) => void;
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
    <div className="flex-1 p-4 md:p-6 bg-content">
      <div className="max-w-4xl mx-auto">
        <h2 className="text-lg md:text-xl font-bold text-gray-900 mb-4">{t('history.title')}</h2>

        <input
          type="text"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          placeholder={t('history.search')}
          className="w-full px-4 py-2 min-h-[44px] border border-gray-300 rounded-lg mb-4 text-sm focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-transparent"
        />

        {loading ? (
          <div className="text-gray-400 text-sm">{t('history.loading')}</div>
        ) : documents.length === 0 ? (
          <div className="text-gray-400 text-sm text-center py-12">
            {t('history.empty')}
          </div>
        ) : (
          <>
            {/* Desktop: table layout */}
            <div className="hidden md:block bg-white rounded-lg border border-gray-200 overflow-hidden">
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
                          onClick={() => onOpenDocument(doc.id, doc.file_type)}
                          className="text-blue-600 hover:text-blue-800 text-sm mr-3"
                        >
                          {t('history.open')}
                        </button>
                        {doc.clean_path && (
                          <button
                            onClick={() => openFile(doc.clean_path!)}
                            className="text-green-600 hover:text-green-800 text-sm mr-3"
                          >
                            {t('history.viewClean', 'View Clean')}
                          </button>
                        )}
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

            {/* Mobile: card layout */}
            <div className="md:hidden flex flex-col gap-3">
              {documents.map((doc) => (
                <div key={doc.id} className="bg-white rounded-lg border border-gray-200 p-4">
                  <div className="flex items-start justify-between mb-2">
                    <h3 className="font-medium text-gray-900 text-sm truncate flex-1 mr-2">
                      {doc.filename}
                    </h3>
                    {statusBadge(doc.status)}
                  </div>
                  <div className="flex items-center gap-3 text-xs text-gray-500 mb-3">
                    <span>{new Date(doc.created_at).toLocaleDateString(i18n.language)}</span>
                    <span>{t('history.pii')}: {doc.pii_count}</span>
                  </div>
                  <div className="flex items-center gap-2 flex-wrap">
                    <button
                      onClick={() => onOpenDocument(doc.id, doc.file_type)}
                      className="min-h-[44px] px-4 py-2 text-blue-600 hover:bg-blue-50 text-sm rounded-lg border border-blue-200 transition-colors"
                    >
                      {t('history.open')}
                    </button>
                    {doc.clean_path && (
                      <button
                        onClick={() => openFile(doc.clean_path!)}
                        className="min-h-[44px] px-4 py-2 text-green-600 hover:bg-green-50 text-sm rounded-lg border border-green-200 transition-colors"
                      >
                        {t('history.viewClean', 'View Clean')}
                      </button>
                    )}
                    <button
                      onClick={() => handleDelete(doc.id)}
                      className="min-h-[44px] px-4 py-2 text-red-500 hover:bg-red-50 text-sm rounded-lg border border-red-200 transition-colors ml-auto"
                    >
                      {t('history.delete')}
                    </button>
                  </div>
                </div>
              ))}
            </div>
          </>
        )}
      </div>
    </div>
  );
}
