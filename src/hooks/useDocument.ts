import { useState, useCallback } from "react";
import { uploadDocument, getDocument } from "../lib/tauri-commands";
import type { Document } from "../lib/types";

export function useDocument() {
  const [document, setDocument] = useState<Document | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const upload = useCallback(async (filePath: string, pageCount: number) => {
    setLoading(true);
    setError(null);
    try {
      const docId = await uploadDocument(filePath, pageCount);
      const doc = await getDocument(docId);
      setDocument(doc);
      return docId;
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
      return null;
    } finally {
      setLoading(false);
    }
  }, []);

  const loadDocument = useCallback(async (docId: string) => {
    setLoading(true);
    setError(null);
    try {
      const doc = await getDocument(docId);
      setDocument(doc);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  }, []);

  return { document, loading, error, upload, loadDocument };
}
