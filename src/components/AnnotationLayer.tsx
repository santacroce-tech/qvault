import { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import type { Annotation } from "../lib/types";
import type { UseAnnotationsReturn } from "../hooks/useAnnotations";

interface Props {
  pageIdx: number;
  /** Scale factor: screen pixels per page-logical unit. */
  scale: number;
  active: boolean;
  annotations: Annotation[];
  hook: UseAnnotationsReturn;
  /** Called when user wants to place a signature — parent opens SignaturePad and calls onSignatureReady. */
  onRequestSignature: (pageIdx: number, x: number, y: number) => void;
}

type DragMode = "move" | "resize-br" | null;

const DEFAULT_TEXT_W = 180;
const DEFAULT_TEXT_H = 24;
const DEFAULT_FONT_SIZE = 14;

/**
 * Overlay layer for annotations.
 *
 * Coordinate system: annotations are stored in page-logical units (PDF points or
 * DOCX logical px). The layer is the same size as the page container; we scale
 * stored coords by `scale` to render on screen.
 */
export default function AnnotationLayer({
  pageIdx,
  scale,
  active,
  annotations,
  hook,
  onRequestSignature,
}: Props) {
  const { t } = useTranslation();
  const layerRef = useRef<HTMLDivElement>(null);
  const [dragging, setDragging] = useState<{ id: string; mode: DragMode; offX: number; offY: number } | null>(null);
  const [editingId, setEditingId] = useState<string | null>(null);

  const pageAnnotations = annotations.filter((a) => a.page === pageIdx);

  // Click on empty area in active mode → create text annotation
  const handleLayerClick = useCallback(
    (e: React.MouseEvent<HTMLDivElement>) => {
      if (!active) return;
      if (e.target !== e.currentTarget) return; // only when clicking the bare layer
      const rect = layerRef.current!.getBoundingClientRect();
      const px = (e.clientX - rect.left) / scale;
      const py = (e.clientY - rect.top) / scale;
      hook.addAnnotation(
        pageIdx,
        "text",
        px,
        py,
        DEFAULT_TEXT_W,
        DEFAULT_TEXT_H,
        "",
        DEFAULT_FONT_SIZE,
      ).then((id) => {
        if (id) setEditingId(id);
      });
    },
    [active, scale, pageIdx, hook]
  );

  // Drag handling
  useEffect(() => {
    if (!dragging) return;
    const ann = annotations.find((a) => a.id === dragging.id);
    if (!ann) return;

    const onMove = (e: MouseEvent) => {
      const layer = layerRef.current;
      if (!layer) return;
      const rect = layer.getBoundingClientRect();
      const cx = (e.clientX - rect.left) / scale;
      const cy = (e.clientY - rect.top) / scale;

      if (dragging.mode === "move") {
        hook.updateAnnotation(ann.id, { x: cx - dragging.offX, y: cy - dragging.offY });
      } else if (dragging.mode === "resize-br") {
        const newW = Math.max(20, cx - ann.x);
        const newH = Math.max(12, cy - ann.y);
        hook.updateAnnotation(ann.id, { width: newW, height: newH });
      }
    };
    const onUp = () => setDragging(null);
    window.addEventListener("mousemove", onMove);
    window.addEventListener("mouseup", onUp);
    return () => {
      window.removeEventListener("mousemove", onMove);
      window.removeEventListener("mouseup", onUp);
    };
  }, [dragging, annotations, scale, hook]);

  const startDrag = (e: React.MouseEvent, ann: Annotation, mode: DragMode) => {
    e.stopPropagation();
    e.preventDefault();
    const rect = layerRef.current!.getBoundingClientRect();
    const cx = (e.clientX - rect.left) / scale;
    const cy = (e.clientY - rect.top) / scale;
    setDragging({ id: ann.id, mode, offX: cx - ann.x, offY: cy - ann.y });
    hook.setSelectedId(ann.id);
  };

  return (
    <div
      ref={layerRef}
      className="absolute inset-0"
      style={{
        zIndex: 25,
        pointerEvents: active ? "auto" : "none",
        cursor: active ? "crosshair" : "default",
      }}
      onClick={handleLayerClick}
    >
      {active && (
        <div className="absolute top-2 left-2 bg-blue-600/90 text-white text-xs px-2 py-1 rounded shadow pointer-events-none">
          {t("annotations.modeHint", "Click to add text · use the Signature button to insert one")}
        </div>
      )}

      {pageAnnotations.map((ann) => {
        const isSelected = hook.selectedId === ann.id;
        const left = ann.x * scale;
        const top = ann.y * scale;
        const width = ann.width * scale;
        const height = ann.height * scale;

        return (
          <div
            key={ann.id}
            className="absolute group"
            style={{
              left,
              top,
              width,
              height,
              border: active
                ? isSelected
                  ? "2px solid #2563eb"
                  : "1px dashed #94a3b8"
                : "none",
              borderRadius: 3,
              background: ann.kind === "text" && active ? "rgba(255,255,255,0.4)" : "transparent",
              cursor: active ? "move" : "default",
            }}
            onMouseDown={active ? (e) => startDrag(e, ann, "move") : undefined}
            onClick={(e) => {
              e.stopPropagation();
              hook.setSelectedId(ann.id);
            }}
            onDoubleClick={(e) => {
              e.stopPropagation();
              if (ann.kind === "text") setEditingId(ann.id);
            }}
          >
            {ann.kind === "text" ? (
              editingId === ann.id ? (
                <textarea
                  autoFocus
                  defaultValue={ann.content}
                  onMouseDown={(e) => e.stopPropagation()}
                  onBlur={(e) => {
                    hook.updateAnnotation(ann.id, { content: e.target.value });
                    setEditingId(null);
                  }}
                  onKeyDown={(e) => {
                    if (e.key === "Escape") setEditingId(null);
                    if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
                      hook.updateAnnotation(ann.id, { content: (e.target as HTMLTextAreaElement).value });
                      setEditingId(null);
                    }
                  }}
                  className="w-full h-full p-0 m-0 bg-white border-none outline-none resize-none"
                  style={{
                    fontSize: (ann.font_size ?? DEFAULT_FONT_SIZE) * scale,
                    lineHeight: 1.2,
                    fontFamily: "Helvetica, Arial, sans-serif",
                    color: "#111827",
                  }}
                />
              ) : (
                <div
                  className="w-full h-full overflow-hidden whitespace-pre-wrap"
                  style={{
                    fontSize: (ann.font_size ?? DEFAULT_FONT_SIZE) * scale,
                    lineHeight: 1.2,
                    fontFamily: "Helvetica, Arial, sans-serif",
                    color: "#111827",
                    padding: 0,
                  }}
                >
                  {ann.content || (active ? <span className="text-gray-400 italic">{t("annotations.placeholder", "Double-click to edit")}</span> : "")}
                </div>
              )
            ) : (
              <img
                src={ann.content}
                alt="signature"
                draggable={false}
                className="w-full h-full object-contain select-none"
                style={{ pointerEvents: "none" }}
              />
            )}

            {/* Controls (only in active mode and when selected) */}
            {active && isSelected && (
              <>
                <button
                  onClick={(e) => {
                    e.stopPropagation();
                    hook.deleteAnnotation(ann.id);
                  }}
                  className="absolute -top-3 -right-3 w-6 h-6 rounded-full bg-red-500 hover:bg-red-600 text-white text-xs flex items-center justify-center shadow"
                  title={t("annotations.delete", "Delete")}
                >
                  ✕
                </button>
                <div
                  onMouseDown={(e) => startDrag(e, ann, "resize-br")}
                  className="absolute -bottom-1.5 -right-1.5 w-3 h-3 bg-white border-2 border-blue-600 cursor-nwse-resize"
                  title={t("annotations.resize", "Resize")}
                />
              </>
            )}
          </div>
        );
      })}

      {/* Per-page signature button: placed at top-right when active, drops a signature into top-left of page */}
      {active && (
        <button
          onClick={(e) => {
            e.stopPropagation();
            const layer = layerRef.current!;
            const px = 40;
            const py = 40;
            onRequestSignature(pageIdx, px / scale, py / scale);
            void px; void py; void layer;
          }}
          className="absolute top-2 right-2 bg-white border border-gray-300 hover:bg-gray-50 text-gray-700 text-xs px-2.5 py-1 rounded shadow-sm flex items-center gap-1"
        >
          <svg className="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M3 17l6-6 4 4 8-8M3 21h18" />
          </svg>
          {t("annotations.addSignature", "Signature")}
        </button>
      )}
    </div>
  );
}
