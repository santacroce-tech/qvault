import { useState, useCallback, useEffect, useRef } from "react";
import { useTranslation } from "react-i18next";
import { open } from "@tauri-apps/plugin-dialog";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { useDocument } from "../hooks/useDocument";
import { getDocument } from "../lib/tauri-commands";

interface Props {
  onDocumentReady: (docId: string, fileType?: string) => void;
}

/** Check if the device is likely mobile based on viewport width */
function isMobile(): boolean {
  return window.innerWidth <= 768;
}

export default function QueueDashboard({ onDocumentReady }: Props) {
  const { t } = useTranslation();
  const { upload, loading, error: uploadError } = useDocument();
  const [mobileLoading, setMobileLoading] = useState(false);
  const [mobileError, setMobileError] = useState<string | null>(null);
  const [dragOver, setDragOver] = useState(false);
  const fileInputRef = useRef<HTMLInputElement>(null);

  const loading_ = loading || mobileLoading;
  const error = uploadError || mobileError;

  const handleFile = useCallback(
    async (filePath: string) => {
      const ext = filePath.split(".").pop()?.toLowerCase() || "pdf";
      const docId = await upload(filePath, 0);
      if (docId) {
        onDocumentReady(docId, ext);
      }
    },
    [upload, onDocumentReady]
  );

  /** Handle file selected via HTML file input (mobile fallback) */
  const handleFileInputChange = useCallback(
    async (e: React.ChangeEvent<HTMLInputElement>) => {
      const file = e.target.files?.[0];
      if (!file) return;

      const ext = file.name.split(".").pop()?.toLowerCase() || "";
      if (ext !== "pdf" && ext !== "docx" && ext !== "doc") return;

      setMobileLoading(true);
      setMobileError(null);
      try {
        const arrayBuffer = await file.arrayBuffer();
        const bytes = new Uint8Array(arrayBuffer);
        let binary = "";
        for (let i = 0; i < bytes.length; i++) {
          binary += String.fromCharCode(bytes[i]);
        }
        const base64 = btoa(binary);

        const docId = await invoke<string>("upload_document_bytes", {
          filename: file.name,
          data: base64,
        });
        const doc = await getDocument(docId);
        if (doc) {
          onDocumentReady(docId, ext);
        }
      } catch (err) {
        setMobileError(err instanceof Error ? err.message : String(err));
      } finally {
        setMobileLoading(false);
        // Reset input so the same file can be selected again
        if (fileInputRef.current) {
          fileInputRef.current.value = "";
        }
      }
    },
    [onDocumentReady]
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
    if (isMobile()) {
      // On mobile, trigger the hidden HTML file input for reliable file picking
      fileInputRef.current?.click();
    } else {
      // On desktop, use the native Tauri file dialog
      const selected = await open({
        multiple: false,
        filters: [
          { name: "Documents", extensions: ["pdf", "docx"] },
        ],
      });
      if (selected) {
        await handleFile(selected as string);
      }
    }
  }, [handleFile]);

  return (
    <div className="flex-1 flex flex-col items-center justify-center p-4 md:p-8 bg-content">
      <div className="max-w-lg w-full">
        <h1 className="text-xl md:text-2xl font-bold text-gray-900 mb-2">{t('dashboard.title')}</h1>
        <p className="text-gray-500 mb-6 md:mb-8 text-sm md:text-base">
          {t('dashboard.subtitle')}
        </p>

        {/* Hidden file input for mobile fallback */}
        <input
          type="file"
          ref={fileInputRef}
          accept=".pdf,.docx,.doc"
          onChange={handleFileInputChange}
          className="hidden"
        />

        {/* Drop Zone */}
        <div
          onClick={handleClick}
          className={`border-2 border-dashed rounded-xl p-6 md:p-12 text-center cursor-pointer transition-colors min-h-[44px] ${
            dragOver
              ? "border-blue-400 bg-blue-50"
              : "border-gray-300 hover:border-blue-400 hover:bg-blue-50/50"
          }`}
        >
          {loading_ ? (
            <div className="flex flex-col items-center gap-3">
              <div className="w-8 h-8 border-2 border-blue-500 border-t-transparent rounded-full animate-spin" />
              <p className="text-gray-600">{t('dashboard.processing')}</p>
            </div>
          ) : (
            <div className="flex flex-col items-center gap-3">
              <svg className="w-10 h-10 md:w-12 md:h-12 text-gray-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.5} d="M7 16a4 4 0 01-.88-7.903A5 5 0 1115.9 6L16 6a5 5 0 011 9.9M15 13l-3-3m0 0l-3 3m3-3v12" />
              </svg>
              {/* On mobile hide drag-drop hint, show tap-friendly text */}
              <p className="text-gray-600 font-medium hidden md:block">
                {t('dashboard.dropPrompt')}
              </p>
              <p className="text-gray-600 font-medium md:hidden">
                {t('dashboard.tapToSelect', 'Tap to select a file')}
              </p>
              <p className="text-gray-400 text-sm">
                {t('dashboard.supportedFormats')}
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
