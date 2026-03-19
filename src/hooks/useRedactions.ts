import { useState, useCallback, useEffect } from "react";
import {
  getRedactions,
  updateRedaction as updateRedactionCmd,
  bulkApprove as bulkApproveCmd,
} from "../lib/tauri-commands";
import type { Redaction, PiiCategory } from "../lib/types";

export type UseRedactionsReturn = ReturnType<typeof useRedactions>;

export function useRedactions(documentId: string | null) {
  const [redactions, setRedactions] = useState<Redaction[]>([]);
  const [loading, setLoading] = useState(false);
  const [selectedId, setSelectedId] = useState<string | null>(null);

  const load = useCallback(async () => {
    if (!documentId) return;
    setLoading(true);
    try {
      const items = await getRedactions(documentId);
      setRedactions(items);
    } catch (e) {
      console.error("Failed to load redactions:", e);
    } finally {
      setLoading(false);
    }
  }, [documentId]);

  useEffect(() => {
    load();
  }, [load]);

  const updateRedaction = useCallback(
    async (id: string, status: string, replacement?: string) => {
      await updateRedactionCmd(id, status, replacement);
      setRedactions((prev) =>
        prev.map((r) =>
          r.id === id
            ? { ...r, status: status as Redaction["status"], replacement: replacement ?? r.replacement }
            : r
        )
      );
    },
    []
  );

  const bulkApprove = useCallback(
    async (category?: PiiCategory) => {
      if (!documentId) return;
      await bulkApproveCmd(documentId, category);
      setRedactions((prev) =>
        prev.map((r) =>
          !category || r.category === category
            ? { ...r, status: "approved" }
            : r
        )
      );
    },
    [documentId]
  );

  const selected = redactions.find((r) => r.id === selectedId) ?? null;

  return {
    redactions,
    loading,
    selected,
    selectedId,
    setSelectedId,
    updateRedaction,
    bulkApprove,
    reload: load,
  };
}
