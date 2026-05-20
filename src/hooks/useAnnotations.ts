import { useState, useCallback, useEffect } from "react";
import {
  getAnnotations,
  addAnnotation as addAnnotationCmd,
  updateAnnotation as updateAnnotationCmd,
  deleteAnnotation as deleteAnnotationCmd,
} from "../lib/tauri-commands";
import type { Annotation, AnnotationKind } from "../lib/types";

export type UseAnnotationsReturn = ReturnType<typeof useAnnotations>;

export function useAnnotations(documentId: string | null) {
  const [annotations, setAnnotations] = useState<Annotation[]>([]);
  const [loading, setLoading] = useState(false);
  const [selectedId, setSelectedId] = useState<string | null>(null);

  const load = useCallback(async () => {
    if (!documentId) return;
    setLoading(true);
    try {
      const items = await getAnnotations(documentId);
      setAnnotations(items);
    } catch (e) {
      console.error("Failed to load annotations:", e);
    } finally {
      setLoading(false);
    }
  }, [documentId]);

  useEffect(() => {
    load();
  }, [load]);

  const addAnnotation = useCallback(
    async (
      page: number,
      kind: AnnotationKind,
      x: number,
      y: number,
      width: number,
      height: number,
      content: string,
      fontSize?: number,
    ) => {
      if (!documentId) return null;
      const id = await addAnnotationCmd(documentId, page, kind, x, y, width, height, content, fontSize);
      const ann: Annotation = {
        id,
        document_id: documentId,
        page,
        kind,
        x,
        y,
        width,
        height,
        content,
        font_size: fontSize ?? null,
        created_at: new Date().toISOString(),
      };
      setAnnotations((prev) => [...prev, ann]);
      setSelectedId(id);
      return id;
    },
    [documentId]
  );

  const updateAnnotation = useCallback(
    async (
      id: string,
      patch: Partial<Pick<Annotation, "x" | "y" | "width" | "height" | "content" | "font_size">>,
    ) => {
      setAnnotations((prev) => {
        const next = prev.map((a) => (a.id === id ? { ...a, ...patch } : a));
        const a = next.find((x) => x.id === id);
        if (a) {
          updateAnnotationCmd(
            id,
            a.x,
            a.y,
            a.width,
            a.height,
            a.content,
            a.font_size ?? undefined,
          ).catch((e) => console.error("Failed to update annotation:", e));
        }
        return next;
      });
    },
    []
  );

  const deleteAnnotation = useCallback(async (id: string) => {
    await deleteAnnotationCmd(id);
    setAnnotations((prev) => prev.filter((a) => a.id !== id));
    setSelectedId((cur) => (cur === id ? null : cur));
  }, []);

  return {
    annotations,
    loading,
    selectedId,
    setSelectedId,
    addAnnotation,
    updateAnnotation,
    deleteAnnotation,
    reload: load,
  };
}
