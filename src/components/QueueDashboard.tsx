import { useState, useCallback, useEffect } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { useDocument } from "../hooks/useDocument";
import { useLicense } from "../hooks/useCredits";
import LicenseStatus from "./CreditBalance";

interface Props {
  onDocumentReady: (docId: string) => void;
}

export default function QueueDashboard({ onDocumentReady }: Props) {
  const { upload, loading, error } = useDocument();
  const { licensed } = useLicense();
  const [dragOver, setDragOver] = useState(false);

  const handleFile = useCallback(
    async (filePath: string) => {
      const docId = await upload(filePath, 0);
      if (docId) {
        onDocumentReady(docId);
      }
    },
    [upload, onDocumentReady]
  );

  // Listen to Tauri's native drag-and-drop events
  useEffect(() => {
    let unlisten: (() => void) | undefined;

    getCurrentWebview()
      .onDragDropEvent((event) => {
        const type = event.payload.type;
        if (type === "enter" || type === "over") {
          setDragOver(true);
        } else if (type === "drop") {
          setDragOver(false);
          const paths = event.payload.paths;
          if (paths && paths.length > 0) {
            const filePath = paths[0];
            const ext = filePath.split(".").pop()?.toLowerCase();
            if (ext === "pdf" || ext === "docx") {
              handleFile(filePath);
            }
          }
        } else if (type === "leave") {
          setDragOver(false);
        }
      })
      .then((fn) => {
        unlisten = fn;
      });

    return () => {
      unlisten?.();
    };
  }, [handleFile]);

  const handleClick = useCallback(async () => {
    if (!licensed) return;
    const selected = await open({
      multiple: false,
      filters: [
        { name: "Documents", extensions: ["pdf", "docx"] },
      ],
    });
    if (selected) {
      await handleFile(selected as string);
    }
  }, [handleFile, licensed]);

  return (
    <div className="flex-1 flex flex-col items-center justify-center p-8 bg-content">
      <div className="max-w-lg w-full">
        <h1 className="text-2xl font-bold text-gray-900 mb-2">Qvault</h1>
        <p className="text-gray-500 mb-8">
          Detect and redact PII from your documents. Everything stays on your
          computer.
        </p>

        {/* License Status */}
        <LicenseStatus />

        {/* Drop Zone */}
        <div
          onClick={licensed ? handleClick : undefined}
          className={`border-2 border-dashed rounded-xl p-12 text-center cursor-pointer transition-colors ${
            !licensed
              ? "border-gray-200 bg-gray-50 cursor-not-allowed"
              : dragOver
              ? "border-blue-400 bg-blue-50"
              : "border-gray-300 hover:border-blue-400 hover:bg-blue-50/50"
          }`}
        >
          {loading ? (
            <div className="flex flex-col items-center gap-3">
              <div className="w-8 h-8 border-2 border-blue-500 border-t-transparent rounded-full animate-spin" />
              <p className="text-gray-600">Processing document...</p>
            </div>
          ) : !licensed ? (
            <div className="flex flex-col items-center gap-3">
              <svg className="w-12 h-12 text-gray-300" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.5} d="M18.364 18.364A9 9 0 005.636 5.636m12.728 12.728A9 9 0 015.636 5.636m12.728 12.728L5.636 5.636" />
              </svg>
              <p className="text-gray-400 font-medium">
                License required
              </p>
              <p className="text-gray-400 text-sm">
                Activate a yearly license in Settings to start processing documents
              </p>
            </div>
          ) : (
            <div className="flex flex-col items-center gap-3">
              <svg className="w-12 h-12 text-gray-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.5} d="M7 16a4 4 0 01-.88-7.903A5 5 0 1115.9 6L16 6a5 5 0 011 9.9M15 13l-3-3m0 0l-3 3m3-3v12" />
              </svg>
              <p className="text-gray-600 font-medium">
                Drop a PDF or DOCX here, or click to browse
              </p>
              <p className="text-gray-400 text-sm">
                Supported: PDF (text-based), DOCX
              </p>
            </div>
          )}
        </div>

        {error && (
          <div className="mt-4 p-3 bg-red-50 border border-red-200 rounded-lg text-red-700 text-sm">
            {error}
          </div>
        )}
      </div>
    </div>
  );
}
