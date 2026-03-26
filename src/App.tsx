import { useState, useRef, useSyncExternalStore } from "react";
import { useTranslation } from "react-i18next";
import QueueDashboard from "./components/QueueDashboard";
import DocumentViewer from "./components/DocumentViewer";
import RedactionSidebar from "./components/RedactionSidebar";
import HistoryView from "./components/HistoryView";
import EntityDatabaseView from "./components/EntityDatabaseView";
import Settings from "./components/Settings";
import type { RedactionRect, PageImageData } from "./lib/tauri-commands";
import { useRedactions } from "./hooks/useRedactions";

type View = "upload" | "review" | "history" | "entities" | "settings";

export type GetPdfRectsForExport = () => RedactionRect[];
export type RenderFlatPages = () => Promise<PageImageData[]>;

// Hook: returns true when viewport <= 768px
const mobileQuery = typeof window !== "undefined" ? window.matchMedia("(max-width: 768px)") : null;
function useIsMobile() {
  return useSyncExternalStore(
    (cb) => {
      mobileQuery?.addEventListener("change", cb);
      return () => mobileQuery?.removeEventListener("change", cb);
    },
    () => mobileQuery?.matches ?? false,
    () => false,
  );
}

function App() {
  const { t } = useTranslation();
  const isMobile = useIsMobile();
  const [view, setView] = useState<View>("upload");
  const [activeDocumentId, setActiveDocumentId] = useState<string | null>(null);
  const [activeFileType, setActiveFileType] = useState<string>("pdf");
  const pdfRectsRef = useRef<GetPdfRectsForExport>(() => []);
  const flatPagesRef = useRef<RenderFlatPages>(async () => []);
  const [advancedMode, setAdvancedMode] = useState(false);
  const [exportSuccess, setExportSuccess] = useState<string | null>(null);

  // Shared redactions state — both DocumentViewer and RedactionSidebar use this
  const redactionsHook = useRedactions(activeDocumentId);

  const handleDocumentReady = (docId: string, fileType?: string) => {
    setActiveDocumentId(docId);
    setActiveFileType(fileType || "pdf");
    setView("review");
    setExportSuccess(null);
  };

  const handleExportSuccess = (outputPath: string) => {
    setExportSuccess(outputPath);
  };

  // Shared nav items definition
  const navItems: { id: View; label: string; icon: React.ReactNode; disabled?: boolean }[] = [
    {
      id: "upload",
      label: t('nav.upload'),
      icon: (
        <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M7 16a4 4 0 01-.88-7.903A5 5 0 1115.9 6L16 6a5 5 0 011 9.9M15 13l-3-3m0 0l-3 3m3-3v12" />
        </svg>
      ),
    },
    {
      id: "review",
      label: t('nav.review'),
      disabled: !activeDocumentId,
      icon: (
        <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z" />
        </svg>
      ),
    },
    {
      id: "history",
      label: t('nav.history'),
      icon: (
        <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 8v4l3 3m6-3a9 9 0 11-18 0 9 9 0 0118 0z" />
        </svg>
      ),
    },
    {
      id: "entities",
      label: t('nav.entities'),
      icon: (
        <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 15v2m-6 4h12a2 2 0 002-2v-6a2 2 0 00-2-2H6a2 2 0 00-2 2v6a2 2 0 002 2zm10-10V7a4 4 0 00-8 0v4h8z" />
        </svg>
      ),
    },
    {
      id: "settings",
      label: t('nav.settings'),
      icon: (
        <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.066 2.573c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.573 1.066c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.066-2.573c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z" />
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" />
        </svg>
      ),
    },
  ];

  return (
    <div className="flex flex-col md:flex-row h-screen">
      {/* Desktop Sidebar Navigation */}
      {!isMobile && (
        <nav className="w-16 bg-sidebar flex flex-col items-center py-4 gap-4">
          {navItems.filter((n) => n.id !== "settings").map((item) => (
            <button
              key={item.id}
              onClick={() => !item.disabled && setView(item.id)}
              className={`w-10 h-10 rounded-lg flex items-center justify-center text-sm font-medium transition-colors ${
                view === item.id
                  ? "bg-white/20 text-white"
                  : item.disabled
                  ? "text-white/20 cursor-not-allowed"
                  : "text-white/60 hover:text-white hover:bg-white/10"
              }`}
              title={item.label}
            >
              {item.icon}
            </button>
          ))}
          <div className="flex-1" />
          <button
            onClick={() => setView("settings")}
            className={`w-10 h-10 rounded-lg flex items-center justify-center text-sm font-medium transition-colors ${
              view === "settings"
                ? "bg-white/20 text-white"
                : "text-white/60 hover:text-white hover:bg-white/10"
            }`}
            title={t('nav.settings')}
          >
            {navItems.find((n) => n.id === "settings")!.icon}
          </button>
        </nav>
      )}

      {/* Main Content */}
      <main className="flex-1 flex overflow-hidden pb-16 md:pb-0">
        {view === "upload" && (
          <QueueDashboard onDocumentReady={handleDocumentReady} />
        )}
        {view === "review" && activeDocumentId && (
          <>
            <div className="flex-1 overflow-auto">
              <DocumentViewer
                documentId={activeDocumentId}
                onRectsReady={(getter) => { pdfRectsRef.current = getter; }}
                onFlatPagesReady={(renderer) => { flatPagesRef.current = renderer; }}
                advancedMode={advancedMode}
                redactionsHook={redactionsHook}
              />
            </div>
            <div className="hidden md:block w-96 border-l border-gray-200 overflow-auto">
              <RedactionSidebar
                documentId={activeDocumentId}
                fileType={activeFileType}
                getPdfRects={() => pdfRectsRef.current()}
                renderFlatPages={() => flatPagesRef.current()}
                advancedMode={advancedMode}
                onToggleAdvanced={() => setAdvancedMode((v) => !v)}
                redactionsHook={redactionsHook}
                onExportSuccess={handleExportSuccess}
              />
            </div>
            {/* Mobile: slide-up redaction panel */}
            {isMobile && (
              <RedactionSidebar
                documentId={activeDocumentId}
                fileType={activeFileType}
                getPdfRects={() => pdfRectsRef.current()}
                renderFlatPages={() => flatPagesRef.current()}
                advancedMode={advancedMode}
                onToggleAdvanced={() => setAdvancedMode((v) => !v)}
                redactionsHook={redactionsHook}
                onExportSuccess={handleExportSuccess}
                mobile
              />
            )}
          </>
        )}
        {view === "review" && !activeDocumentId && (
          <div className="flex-1 flex items-center justify-center text-gray-400">
            {t('viewer.noDocument')}
          </div>
        )}
        {view === "history" && <HistoryView onOpenDocument={handleDocumentReady} />}
        {view === "entities" && <EntityDatabaseView onOpenDocument={handleDocumentReady} />}
        {view === "settings" && <Settings />}
      </main>

      {/* Mobile Bottom Tab Bar */}
      {isMobile && (
        <nav className="fixed bottom-0 left-0 right-0 bg-sidebar flex items-center justify-around z-40 border-t border-white/10 safe-area-bottom">
          {navItems.map((item) => (
            <button
              key={item.id}
              onClick={() => !item.disabled && setView(item.id)}
              className={`flex flex-col items-center justify-center min-h-[56px] flex-1 gap-0.5 transition-colors ${
                view === item.id
                  ? "text-white"
                  : item.disabled
                  ? "text-white/20"
                  : "text-white/60 active:text-white"
              }`}
            >
              {item.icon}
              <span className="text-[10px] leading-tight">{item.label}</span>
            </button>
          ))}
        </nav>
      )}

      {/* Export Success Toast */}
      {exportSuccess && (
        <div className="fixed bottom-6 right-6 bg-green-600 text-white px-5 py-3 rounded-lg shadow-xl flex items-center gap-3 z-50 animate-slide-up">
          <svg className="w-5 h-5 shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M5 13l4 4L19 7" />
          </svg>
          <div>
            <p className="font-medium text-sm">{t('toast.exportComplete')}</p>
            <p className="text-xs text-green-100 truncate max-w-xs">{exportSuccess.split("/").pop()}</p>
          </div>
          <button
            onClick={() => setExportSuccess(null)}
            className="ml-2 text-green-200 hover:text-white"
          >
            <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
            </svg>
          </button>
        </div>
      )}
    </div>
  );
}

export default App;
