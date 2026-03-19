import { useState, useRef } from "react";
import QueueDashboard from "./components/QueueDashboard";
import DocumentViewer from "./components/DocumentViewer";
import RedactionSidebar from "./components/RedactionSidebar";
import HistoryView from "./components/HistoryView";
import Settings from "./components/Settings";
import type { RedactionRect } from "./lib/tauri-commands";

type View = "upload" | "review" | "history" | "settings";

export type GetPdfRectsForExport = () => RedactionRect[];

function App() {
  const [view, setView] = useState<View>("upload");
  const [activeDocumentId, setActiveDocumentId] = useState<string | null>(null);
  const pdfRectsRef = useRef<GetPdfRectsForExport>(() => []);
  const [advancedMode, setAdvancedMode] = useState(false);

  const handleDocumentReady = (docId: string) => {
    setActiveDocumentId(docId);
    setView("review");
  };

  return (
    <div className="flex h-screen">
      {/* Sidebar Navigation */}
      <nav className="w-16 bg-sidebar flex flex-col items-center py-4 gap-4">
        <button
          onClick={() => setView("upload")}
          className={`w-10 h-10 rounded-lg flex items-center justify-center text-sm font-medium transition-colors ${
            view === "upload"
              ? "bg-white/20 text-white"
              : "text-white/60 hover:text-white hover:bg-white/10"
          }`}
          title="Upload"
        >
          <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M7 16a4 4 0 01-.88-7.903A5 5 0 1115.9 6L16 6a5 5 0 011 9.9M15 13l-3-3m0 0l-3 3m3-3v12" />
          </svg>
        </button>
        <button
          onClick={() => view === "review" || activeDocumentId ? setView("review") : null}
          className={`w-10 h-10 rounded-lg flex items-center justify-center text-sm font-medium transition-colors ${
            view === "review"
              ? "bg-white/20 text-white"
              : activeDocumentId
              ? "text-white/60 hover:text-white hover:bg-white/10"
              : "text-white/20 cursor-not-allowed"
          }`}
          title="Review"
        >
          <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z" />
          </svg>
        </button>
        <button
          onClick={() => setView("history")}
          className={`w-10 h-10 rounded-lg flex items-center justify-center text-sm font-medium transition-colors ${
            view === "history"
              ? "bg-white/20 text-white"
              : "text-white/60 hover:text-white hover:bg-white/10"
          }`}
          title="History"
        >
          <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 8v4l3 3m6-3a9 9 0 11-18 0 9 9 0 0118 0z" />
          </svg>
        </button>
        <div className="flex-1" />
        <button
          onClick={() => setView("settings")}
          className={`w-10 h-10 rounded-lg flex items-center justify-center text-sm font-medium transition-colors ${
            view === "settings"
              ? "bg-white/20 text-white"
              : "text-white/60 hover:text-white hover:bg-white/10"
          }`}
          title="Settings"
        >
          <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.066 2.573c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.573 1.066c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.066-2.573c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z" />
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" />
          </svg>
        </button>
      </nav>

      {/* Main Content */}
      <main className="flex-1 flex overflow-hidden">
        {view === "upload" && (
          <QueueDashboard onDocumentReady={handleDocumentReady} />
        )}
        {view === "review" && activeDocumentId && (
          <>
            <div className="flex-1 overflow-auto">
              <DocumentViewer
                documentId={activeDocumentId}
                onRectsReady={(getter) => { pdfRectsRef.current = getter; }}
                advancedMode={advancedMode}
              />
            </div>
            <div className="w-96 border-l border-gray-200 overflow-auto">
              <RedactionSidebar
                documentId={activeDocumentId}
                getPdfRects={() => pdfRectsRef.current()}
                advancedMode={advancedMode}
                onToggleAdvanced={() => setAdvancedMode((v) => !v)}
              />
            </div>
          </>
        )}
        {view === "review" && !activeDocumentId && (
          <div className="flex-1 flex items-center justify-center text-gray-400">
            No document selected. Upload a document first.
          </div>
        )}
        {view === "history" && <HistoryView onOpenDocument={handleDocumentReady} />}
        {view === "settings" && <Settings />}
      </main>
    </div>
  );
}

export default App;
