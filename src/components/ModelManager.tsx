import { useCallback, useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { open, ask } from "@tauri-apps/plugin-dialog";
import {
  getAiCapabilities,
  downloadModel,
  cancelModelDownload,
  importModel,
  openManualDownload,
  deleteModel,
  setLayer3,
  runAiSelfTest,
  setOcrDpi,
  type AiCapabilities,
  type ModelInfo,
  type SelfTestReport,
} from "../lib/tauri-commands";

function formatBytes(n: number): string {
  if (n >= 1024 ** 3) return `${(n / 1024 ** 3).toFixed(2)} GB`;
  if (n >= 1024 ** 2) return `${Math.round(n / 1024 ** 2)} MB`;
  return `${Math.max(1, Math.round(n / 1024))} KB`;
}

function hostOf(source: string): string {
  return source.toLowerCase().includes("hugging face") ? "huggingface.co" : source;
}

type Progress = Record<string, { received: number; total: number }>;

export default function ModelManager() {
  const { t } = useTranslation();
  const [caps, setCaps] = useState<AiCapabilities | null>(null);
  const [progress, setProgress] = useState<Progress>({});
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [selfTest, setSelfTest] = useState<SelfTestReport | null>(null);

  const refresh = useCallback(async (probe = false) => {
    try {
      setCaps(await getAiCapabilities(probe));
    } catch (e) {
      console.warn("[Qvault] AI capabilities unavailable:", e);
    }
  }, []);

  useEffect(() => {
    refresh(false);
  }, [refresh]);

  // Download progress / completion events from the backend.
  useEffect(() => {
    let unlisteners: Array<() => void> = [];
    let cancelled = false;
    (async () => {
      try {
        const { listen } = await import("@tauri-apps/api/event");
        const u1 = await listen<{ id: string; received: number; total: number }>("ai-model-download", (e) => {
          setProgress((p) => ({ ...p, [e.payload.id]: { received: e.payload.received, total: e.payload.total } }));
        });
        const u2 = await listen<{ id: string; ok: boolean; error: string | null }>("ai-model-download-done", (e) => {
          setProgress((p) => {
            const next = { ...p };
            delete next[e.payload.id];
            return next;
          });
          if (e.payload.error) setError(t("ai.downloadFailed", { error: e.payload.error }));
          refresh(false);
        });
        if (cancelled) { u1(); u2(); } else unlisteners = [u1, u2];
      } catch {
        // Not running inside Tauri (browser preview).
      }
    })();
    return () => { cancelled = true; unlisteners.forEach((u) => u()); };
  }, [refresh, t]);

  const llms = useMemo(() => caps?.models.filter((m) => m.purpose === "llm") ?? [], [caps]);
  const ocr = useMemo(() => caps?.models.filter((m) => m.purpose !== "llm") ?? [], [caps]);
  const installedLlms = llms.filter((m) => m.installed);

  const run = async (key: string, fn: () => Promise<void>) => {
    setBusy(key);
    setError(null);
    setNotice(null);
    try {
      await fn();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(null);
      refresh(false);
    }
  };

  // Before download only an estimate is possible (assessFit needs the file,
  // and the app never asks QVAC about files it does not have).
  const ramEstimate = (m: ModelInfo): "ok" | "low" | null => {
    if (!m.ramNeeded || !caps?.systemMemory) return null;
    // Leave room for the OS, the app and the document being scanned.
    return m.ramNeeded <= caps.systemMemory * 0.6 ? "ok" : "low";
  };

  const handleDownload = (m: ModelInfo) =>
    run(`dl-${m.id}`, async () => {
      const est = ramEstimate(m);
      const lines = [t("ai.confirmDownload", { name: m.name, size: formatBytes(m.size), host: hostOf(m.source) })];
      if (m.ramNeeded) lines.push(t("ai.ramNeeded", { size: formatBytes(m.ramNeeded) }));
      if (est === "low") lines.push(t("ai.ramEstimateLow", { total: formatBytes(caps!.systemMemory!) }));
      if (m.note) lines.push(m.note);
      const ok = await ask(lines.join("\n\n"), { title: t("ai.downloadTitle"), kind: est === "low" ? "warning" : "info" });
      if (!ok) return;
      setProgress((p) => ({ ...p, [m.id]: { received: 0, total: m.size } }));
      await downloadModel(m.id);
    });

  const handleOcrDpi = (dpi: number) => run("ocr-dpi", async () => { await setOcrDpi(dpi); });

  const handleCancel = (m: ModelInfo) => run(`cancel-${m.id}`, async () => { await cancelModelDownload(m.id); });

  const handleDelete = (m: ModelInfo) =>
    run(`del-${m.id}`, async () => {
      const ok = await ask(t("ai.confirmDelete", { name: m.name, size: formatBytes(m.size) }), {
        title: t("ai.deleteTitle"),
        kind: "warning",
      });
      if (!ok) return;
      await deleteModel(m.id);
    });

  // Several files can be picked at once (e.g. both OCR models). Each one is
  // verified against the catalog; unknown files still need explicit consent.
  const handleImport = () =>
    run("import", async () => {
      const selected = await open({ multiple: true, filters: [{ name: "GGUF", extensions: ["gguf"] }] });
      if (!selected) return;
      const paths = Array.isArray(selected) ? selected : [selected];
      let imported = 0;
      for (const path of paths) {
        let outcome = await importModel(path, false);
        if (outcome.status === "needsConsent") {
          const ok = await ask(
            t("ai.confirmUnknownImport", {
              file: outcome.fileName,
              size: formatBytes(outcome.size),
              sha: outcome.sha256.slice(0, 16),
            }),
            { title: t("ai.unknownImportTitle"), kind: "warning" }
          );
          if (!ok) continue;
          outcome = await importModel(path, true);
        }
        if (outcome.status !== "needsConsent") imported += 1;
      }
      if (imported > 0) setNotice(t("ai.importedCount", { count: imported }));
    });

  const handleManualDownload = (m: ModelInfo) =>
    run(`manual-${m.id}`, async () => { await openManualDownload(m.id); });

  const handleToggle = (enabled: boolean) =>
    run("toggle", async () => {
      await setLayer3(enabled, caps?.layer3Model ?? installedLlms[0]?.id);
    });

  const handleSelectModel = (id: string) =>
    run("select", async () => {
      await setLayer3(caps?.layer3Enabled ?? false, id);
    });

  const handleProbe = () => run("probe", async () => { await refresh(true); });

  const handleSelfTest = () =>
    run("selftest", async () => {
      setSelfTest(null);
      setSelfTest(await runAiSelfTest());
    });

  if (!caps) {
    return <p className="text-sm text-gray-500">{t("ai.loading")}</p>;
  }

  const engineStatus = !caps.sidecarBundled
    ? { dot: "bg-gray-400", text: t("ai.engineNotBundled") }
    : !caps.sidecarAvailable
    ? { dot: "bg-red-500", text: t("ai.engineUnavailable", { reason: caps.unavailableReason ?? "" }) }
    : caps.sidecarRunning
    ? { dot: "bg-green-500", text: t("ai.engineRunning", { version: caps.sidecarVersion ?? "" }) }
    : { dot: "bg-yellow-500", text: t("ai.engineIdle") };

  const renderModel = (m: ModelInfo) => {
    const p = progress[m.id] ?? (m.download?.active ? { received: m.download.received, total: m.download.total } : null);
    const pct = p && p.total > 0 ? Math.min(100, (p.received / p.total) * 100) : 0;
    return (
      <li key={m.id} className="py-3 border-t border-gray-100 first:border-t-0">
        <div className="flex items-start justify-between gap-3">
          <div className="min-w-0">
            <div className="text-sm font-medium text-gray-800 truncate">
              {m.name}
              {m.defaultLlm && (
                <span className="ml-2 text-[10px] uppercase tracking-wide bg-blue-50 text-blue-600 px-1.5 py-0.5 rounded">
                  {t("ai.recommended")}
                </span>
              )}
            </div>
            <div className="text-xs text-gray-500">
              {formatBytes(m.size)} · {m.licence}
              {m.purpose === "llm" && m.ramNeeded > 0 && <> · {t("ai.ramNeeded", { size: formatBytes(m.ramNeeded) })}</>}
            </div>
            {m.purpose === "llm" && !m.installed && ramEstimate(m) && (
              <div className={`text-xs mt-0.5 ${ramEstimate(m) === "ok" ? "text-gray-500" : "text-amber-600"}`}>
                {ramEstimate(m) === "ok"
                  ? t("ai.ramEstimateOk", { total: formatBytes(caps.systemMemory!) })
                  : t("ai.ramEstimateLow", { total: formatBytes(caps.systemMemory!) })}
              </div>
            )}
            {m.note && <div className="text-xs text-gray-500 mt-0.5">{m.note}</div>}
            {m.purpose === "llm" && m.installed && (
              <div className="text-xs mt-0.5">
                {m.fit ? (
                  <span
                    className={
                      m.fit.verdict === "likely-fits"
                        ? "text-green-600"
                        : m.fit.verdict === "likely-too-large"
                        ? "text-red-600"
                        : "text-gray-500"
                    }
                  >
                    {t(`ai.fit.${m.fit.verdict}`)}
                  </span>
                ) : (
                  <span className="text-gray-400">{t("ai.fit.notChecked")}</span>
                )}
              </div>
            )}
            {!m.installed && !m.downloadable && (
              <div className="text-xs text-amber-600 mt-0.5">{t("ai.importOnly")}</div>
            )}
            {m.download?.error && !p && <div className="text-xs text-red-600 mt-0.5">{m.download.error}</div>}
          </div>
          <div className="flex-shrink-0 flex gap-2">
            {m.installed ? (
              <button
                onClick={() => handleDelete(m)}
                disabled={busy !== null}
                className="px-3 py-1.5 text-xs rounded-lg bg-red-50 text-red-600 hover:bg-red-100 disabled:opacity-50"
              >
                {t("ai.delete")}
              </button>
            ) : p ? (
              <button
                onClick={() => handleCancel(m)}
                className="px-3 py-1.5 text-xs rounded-lg bg-gray-100 text-gray-700 hover:bg-gray-200"
              >
                {t("ai.cancel")}
              </button>
            ) : m.downloadable ? (
              <button
                onClick={() => handleDownload(m)}
                disabled={busy !== null}
                className="px-3 py-1.5 text-xs rounded-lg bg-blue-50 text-blue-600 hover:bg-blue-100 disabled:opacity-50"
              >
                {t("ai.download", { size: formatBytes(m.size) })}
              </button>
            ) : m.manualUrl ? (
              <button
                onClick={() => handleManualDownload(m)}
                disabled={busy !== null}
                title={m.manualUrl}
                className="px-3 py-1.5 text-xs rounded-lg bg-blue-50 text-blue-600 hover:bg-blue-100 disabled:opacity-50"
              >
                {t("ai.manualDownload", { size: formatBytes(m.size) })}
              </button>
            ) : null}
          </div>
        </div>
        {p && (
          <div className="mt-2">
            <div className="w-full bg-gray-200 rounded-full h-1.5">
              <div className="bg-blue-500 h-1.5 rounded-full transition-all duration-300" style={{ width: `${pct.toFixed(1)}%` }} />
            </div>
            <div className="text-[11px] text-gray-500 mt-1">
              {formatBytes(p.received)} / {formatBytes(p.total)} ({pct.toFixed(0)}%)
            </div>
          </div>
        )}
      </li>
    );
  };

  return (
    <div className="space-y-4">
      <p className="text-sm text-gray-600">{t("ai.description")}</p>

      <div className="flex items-center justify-between gap-2">
        <div className="flex items-center gap-2 min-w-0">
          <span className={`inline-block w-2 h-2 rounded-full flex-shrink-0 ${engineStatus.dot}`} />
          <span className="text-sm text-gray-700 truncate">{engineStatus.text}</span>
        </div>
        {caps.sidecarBundled && caps.sidecarAvailable && (
          <button
            onClick={handleProbe}
            disabled={busy !== null}
            className="flex-shrink-0 px-3 py-1.5 text-xs rounded-lg bg-gray-100 text-gray-700 hover:bg-gray-200 disabled:opacity-50"
          >
            {busy === "probe" ? t("ai.checking") : t("ai.checkCompatibility")}
          </button>
        )}
      </div>

      {/* Layer 3 toggle */}
      <div className="rounded-lg bg-gray-50 p-3 space-y-2">
        <label className={`flex items-center gap-3 ${installedLlms.length === 0 || !caps.sidecarBundled ? "opacity-50" : "cursor-pointer"}`}>
          <input
            type="checkbox"
            checked={caps.layer3Enabled}
            disabled={installedLlms.length === 0 || !caps.sidecarBundled || busy !== null}
            onChange={(e) => handleToggle(e.target.checked)}
            className="w-4 h-4 text-blue-600 rounded focus:ring-blue-500"
          />
          <span className="text-sm text-gray-700">{t("ai.layer3Toggle")}</span>
        </label>
        {installedLlms.length === 0 ? (
          <p className="text-xs text-gray-500">{t("ai.layer3NeedsModel")}</p>
        ) : (
          <select
            value={caps.layer3Model ?? ""}
            onChange={(e) => handleSelectModel(e.target.value)}
            disabled={busy !== null}
            className="w-full px-3 py-1.5 border border-gray-300 rounded-lg text-sm bg-white focus:outline-none focus:ring-2 focus:ring-blue-500"
          >
            {installedLlms.map((m) => (
              <option key={m.id} value={m.id} disabled={m.fit?.verdict === "likely-too-large" && m.id !== caps.layer3Model}>
                {m.name} · {formatBytes(m.size)}
                {m.fit ? ` · ${t(`ai.fit.${m.fit.verdict}`)}` : ""}
              </option>
            ))}
          </select>
        )}
        <p className="text-xs text-gray-500">{t("ai.layer3Review")}</p>
      </div>

      {/* Models */}
      <div>
        <h4 className="text-xs font-semibold text-gray-500 uppercase tracking-wider">{t("ai.detectionModels")}</h4>
        <ul>{llms.map(renderModel)}</ul>
      </div>
      <div>
        <h4 className="text-xs font-semibold text-gray-500 uppercase tracking-wider">{t("ai.ocrModels")}</h4>
        <p className="text-xs text-gray-500 mt-1">{t("ai.ocrModelsDesc")}</p>
        <p
          className={`text-xs mt-1 ${caps.ocrAvailable ? "text-green-700" : "text-amber-700"}`}
          role="status"
        >
          {caps.ocrAvailable
            ? t("ai.ocrReady")
            : !caps.sidecarBundled
            ? t("ai.ocrNoEngine")
            : !caps.ocrInstalled
            ? t("ai.ocrNotInstalled")
            : t("ai.ocrEngineUnavailable")}
        </p>
        <ul>{ocr.map(renderModel)}</ul>
        {ocr.some((m) => !m.installed && m.manualUrl) && (
          <div className="mt-2 text-xs text-gray-600 bg-gray-50 rounded-lg p-3 space-y-2">
            <div className="font-medium text-gray-700">{t("ai.ocrHowToTitle")}</div>
            <ol className="list-decimal pl-4 space-y-1">
              <li>{t("ai.ocrHowToStep1")}</li>
              <li>{t("ai.ocrHowToStep2")}</li>
              <li>{t("ai.ocrHowToStep3")}</li>
              <li>{t("ai.ocrHowToStep4")}</li>
            </ol>
            <div className="space-y-1">
              {ocr.filter((m) => m.manualUrl).map((m) => (
                <div key={m.id} className="break-all">
                  <code className="text-gray-700">{m.manualUrl!.split("/").pop()}</code>
                  <span className="text-gray-400"> · {formatBytes(m.size)} · SHA-256 </span>
                  <code className="text-gray-500 select-all">{m.sha256}</code>
                </div>
              ))}
            </div>
            <p className="text-gray-500">{t("ai.ocrHowToAirGapped")}</p>
          </div>
        )}
        <label className="mt-2 flex items-center gap-2 text-sm text-gray-700">
          <span>{t("ai.ocrDpi")}</span>
          <select
            value={caps.ocrDpi}
            onChange={(e) => handleOcrDpi(Number(e.target.value))}
            disabled={busy !== null}
            className="px-2 py-1 border border-gray-300 rounded-lg text-sm bg-white focus:outline-none focus:ring-2 focus:ring-blue-500"
          >
            {[200, 300, 400].map((d) => (
              <option key={d} value={d}>
                {d === 200 ? t("ai.ocrDpiDefault", { dpi: d }) : `${d} DPI`}
              </option>
            ))}
          </select>
        </label>
        <p className="text-xs text-gray-500 mt-1">{t("ai.ocrDpiHelp")}</p>
      </div>

      <div className="flex flex-wrap gap-2">
        <button
          onClick={handleImport}
          disabled={busy !== null}
          className="px-4 py-2 bg-blue-50 text-blue-600 text-sm rounded-lg hover:bg-blue-100 transition-colors disabled:opacity-50"
        >
          {busy === "import" ? t("ai.importing") : t("ai.importFromFile")}
        </button>
        {caps.sidecarBundled && (
          <button
            onClick={handleSelfTest}
            disabled={busy !== null || !caps.sidecarAvailable}
            className="px-4 py-2 bg-gray-100 text-gray-700 text-sm rounded-lg hover:bg-gray-200 transition-colors disabled:opacity-50"
          >
            {busy === "selftest" ? t("ai.testing") : t("ai.selfTest")}
          </button>
        )}
      </div>

      {notice && <p className="text-xs text-green-700">{notice}</p>}
      {error && <p className="text-xs text-red-600 break-words">{error}</p>}
      {selfTest && (
        <div className="text-xs text-gray-600 bg-gray-50 rounded-lg p-3 space-y-1">
          {selfTest.error ? (
            <div className="text-red-600">{selfTest.error}</div>
          ) : (
            <>
              <div>{t("ai.selfTestEngine", { version: selfTest.sidecar ?? "" })}</div>
              <div>
                {selfTest.ocrError
                  ? t("ai.selfTestOcrSkipped", { reason: selfTest.ocrError })
                  : t("ai.selfTestOcr", { ms: selfTest.ocrMs ?? 0, text: selfTest.ocrText ?? "" })}
              </div>
              <div>
                {selfTest.layer3Error
                  ? t("ai.selfTestLayer3Skipped", { reason: selfTest.layer3Error })
                  : t("ai.selfTestLayer3", { ms: selfTest.layer3Ms ?? 0, count: selfTest.layer3Entities.length })}
              </div>
            </>
          )}
        </div>
      )}
      <p className="text-[11px] text-gray-400">{t("ai.privacyNote")}</p>
    </div>
  );
}
