import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";

interface Props {
  onCancel: () => void;
  onSave: (pngDataUrl: string, naturalWidth: number, naturalHeight: number) => void;
}

const CANVAS_W = 600;
const CANVAS_H = 200;

/** Draw signature on a canvas, return as transparent-background PNG data URL. */
export default function SignaturePad({ onCancel, onSave }: Props) {
  const { t } = useTranslation();
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const drawingRef = useRef(false);
  const lastPtRef = useRef<{ x: number; y: number } | null>(null);
  const [hasInk, setHasInk] = useState(false);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;
    ctx.clearRect(0, 0, canvas.width, canvas.height);
    ctx.lineCap = "round";
    ctx.lineJoin = "round";
    ctx.strokeStyle = "#0d2042";
    ctx.lineWidth = 2.5;
  }, []);

  const pointer = (e: React.PointerEvent<HTMLCanvasElement>) => {
    const c = canvasRef.current!;
    const rect = c.getBoundingClientRect();
    return {
      x: ((e.clientX - rect.left) / rect.width) * c.width,
      y: ((e.clientY - rect.top) / rect.height) * c.height,
    };
  };

  const onDown = (e: React.PointerEvent<HTMLCanvasElement>) => {
    drawingRef.current = true;
    lastPtRef.current = pointer(e);
    (e.target as HTMLCanvasElement).setPointerCapture(e.pointerId);
  };

  const onMove = (e: React.PointerEvent<HTMLCanvasElement>) => {
    if (!drawingRef.current) return;
    const ctx = canvasRef.current!.getContext("2d")!;
    const pt = pointer(e);
    const last = lastPtRef.current!;
    ctx.beginPath();
    ctx.moveTo(last.x, last.y);
    ctx.lineTo(pt.x, pt.y);
    ctx.stroke();
    lastPtRef.current = pt;
    if (!hasInk) setHasInk(true);
  };

  const onUp = () => {
    drawingRef.current = false;
    lastPtRef.current = null;
  };

  const clear = () => {
    const c = canvasRef.current!;
    c.getContext("2d")!.clearRect(0, 0, c.width, c.height);
    setHasInk(false);
  };

  const save = () => {
    const c = canvasRef.current!;
    // Crop to ink bounding box so the signature stays tight
    const ctx = c.getContext("2d")!;
    const img = ctx.getImageData(0, 0, c.width, c.height).data;
    let minX = c.width, minY = c.height, maxX = -1, maxY = -1;
    for (let y = 0; y < c.height; y++) {
      for (let x = 0; x < c.width; x++) {
        const a = img[(y * c.width + x) * 4 + 3];
        if (a > 0) {
          if (x < minX) minX = x;
          if (x > maxX) maxX = x;
          if (y < minY) minY = y;
          if (y > maxY) maxY = y;
        }
      }
    }
    let dataUrl: string;
    let w: number;
    let h: number;
    if (maxX < 0) {
      dataUrl = c.toDataURL("image/png");
      w = c.width;
      h = c.height;
    } else {
      const pad = 4;
      const cw = Math.max(1, maxX - minX + 1 + pad * 2);
      const ch = Math.max(1, maxY - minY + 1 + pad * 2);
      const tmp = document.createElement("canvas");
      tmp.width = cw;
      tmp.height = ch;
      const tctx = tmp.getContext("2d")!;
      tctx.drawImage(c, minX - pad, minY - pad, cw, ch, 0, 0, cw, ch);
      dataUrl = tmp.toDataURL("image/png");
      w = cw;
      h = ch;
    }
    onSave(dataUrl, w, h);
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50">
      <div className="bg-white rounded-xl shadow-2xl p-5 w-[640px] max-w-[95vw]">
        <div className="flex items-center justify-between mb-3">
          <h3 className="font-semibold text-gray-900">{t("annotations.signTitle", "Draw signature")}</h3>
          <button onClick={onCancel} className="text-gray-400 hover:text-gray-700 text-xl leading-none">
            ✕
          </button>
        </div>
        <p className="text-xs text-gray-500 mb-2">
          {t("annotations.signHint", "Use mouse, trackpad, or touchscreen to draw your signature.")}
        </p>
        <div className="border-2 border-dashed border-gray-300 rounded-lg bg-gray-50">
          <canvas
            ref={canvasRef}
            width={CANVAS_W}
            height={CANVAS_H}
            className="w-full block touch-none cursor-crosshair"
            onPointerDown={onDown}
            onPointerMove={onMove}
            onPointerUp={onUp}
            onPointerCancel={onUp}
          />
        </div>
        <div className="flex items-center justify-end gap-2 mt-4">
          <button onClick={clear} className="px-3 py-1.5 text-sm text-gray-700 border border-gray-300 rounded-lg hover:bg-gray-50">
            {t("annotations.clear", "Clear")}
          </button>
          <button onClick={onCancel} className="px-3 py-1.5 text-sm text-gray-700 border border-gray-300 rounded-lg hover:bg-gray-50">
            {t("common.cancel", "Cancel")}
          </button>
          <button
            onClick={save}
            disabled={!hasInk}
            className="px-3 py-1.5 text-sm bg-blue-600 text-white rounded-lg hover:bg-blue-700 disabled:opacity-50 disabled:cursor-not-allowed"
          >
            {t("annotations.useSignature", "Use signature")}
          </button>
        </div>
      </div>
    </div>
  );
}
