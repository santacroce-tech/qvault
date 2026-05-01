import { useState, useEffect, useCallback } from "react";
import { useTranslation } from "react-i18next";
import LanguageSwitcher from "./LanguageSwitcher";
import { getLlmStatus, downloadLlmModel, loadLlmModel, setLlmEnabled, type LlmStatus } from "../lib/tauri-commands";

export default function Settings() {
  const { t } = useTranslation();
  const [jurisdiction, setJurisdiction] = useState("global");
  const [sensitivity, setSensitivity] = useState(50);
  const [redactionStyle, setRedactionStyle] = useState(() =>
    localStorage.getItem("qvault_redaction_style") || "black_bar"
  );
  const [llmStatus, setLlmStatus] = useState<LlmStatus | null>(null);
  const [llmLoading, setLlmLoading] = useState(false);

  const refreshLlmStatus = useCallback(async () => {
    try {
      const status = await getLlmStatus();
      setLlmStatus(status);
    } catch (e) {
      console.warn("Failed to get LLM status:", e);
    }
  }, []);

  useEffect(() => {
    refreshLlmStatus();
  }, [refreshLlmStatus]);

  // Poll status while downloading
  useEffect(() => {
    if (!llmStatus?.downloading) return;
    const interval = setInterval(refreshLlmStatus, 1000);
    return () => clearInterval(interval);
  }, [llmStatus?.downloading, refreshLlmStatus]);

  const handleDownloadModel = async () => {
    try {
      await downloadLlmModel();
      // Start polling
      const status = await getLlmStatus();
      setLlmStatus(status);
    } catch (e) {
      console.error("Download failed:", e);
    }
  };

  const handleLoadModel = async () => {
    setLlmLoading(true);
    try {
      await loadLlmModel();
      await refreshLlmStatus();
    } catch (e) {
      console.error("Load failed:", e);
    } finally {
      setLlmLoading(false);
    }
  };

  const handleToggleLlm = async (enabled: boolean) => {
    try {
      await setLlmEnabled(enabled);
      await refreshLlmStatus();
    } catch (e) {
      console.error("Toggle failed:", e);
    }
  };

  const handleRedactionStyleChange = (value: string) => {
    setRedactionStyle(value);
    localStorage.setItem("qvault_redaction_style", value);
  };

  return (
    <div className="flex-1 p-6 bg-content overflow-auto">
      <div className="max-w-lg mx-auto">
        <h2 className="text-xl font-bold text-gray-900 mb-6">{t('settings.title')}</h2>

        {/* Language */}
        <section className="mb-8">
          <h3 className="text-sm font-semibold text-gray-700 mb-3 uppercase tracking-wider">
            {t('settings.language')}
          </h3>
          <LanguageSwitcher />
        </section>

        {/* Detection Settings */}
        <section className="mb-8">
          <h3 className="text-sm font-semibold text-gray-700 mb-3 uppercase tracking-wider">
            {t('settings.detection')}
          </h3>

          <div className="bg-white rounded-lg border border-gray-200 p-4 space-y-4">
            <div>
              <label className="block text-sm font-medium text-gray-700 mb-1">
                {t('settings.jurisdiction')}
              </label>
              <select
                value={jurisdiction}
                onChange={(e) => setJurisdiction(e.target.value)}
                className="w-full px-3 py-2 border border-gray-300 rounded-lg text-sm focus:outline-none focus:ring-2 focus:ring-blue-500"
              >
                <option value="global">{t('settings.jurisdictionGlobal')}</option>
                <option value="us">{t('settings.jurisdictionUS')}</option>
                <option value="eu">{t('settings.jurisdictionEU')}</option>
                <option value="br">{t('settings.jurisdictionBR')}</option>
                <option value="de">{t('settings.jurisdictionDE')}</option>
              </select>
            </div>

            <div>
              <label className="block text-sm font-medium text-gray-700 mb-1">
                {t('settings.sensitivity', { value: sensitivity })}
              </label>
              <input
                type="range"
                min={0}
                max={100}
                value={sensitivity}
                onChange={(e) => setSensitivity(Number(e.target.value))}
                className="w-full"
              />
              <div className="flex justify-between text-xs text-gray-400 mt-1">
                <span>{t('settings.fewerDetections')}</span>
                <span>{t('settings.moreDetections')}</span>
              </div>
            </div>

            <div>
              <label className="block text-sm font-medium text-gray-700 mb-1">
                {t('settings.redactionStyle')}
              </label>
              <select
                value={redactionStyle}
                onChange={(e) => handleRedactionStyleChange(e.target.value)}
                className="w-full px-3 py-2 border border-gray-300 rounded-lg text-sm focus:outline-none focus:ring-2 focus:ring-blue-500"
              >
                <option value="black_bar">{t('settings.redactionBlackBar')}</option>
                <option value="text_replace">{t('settings.redactionTextReplace')}</option>
              </select>
              <p className="text-xs text-gray-400 mt-1">
                {redactionStyle === "text_replace"
                  ? t('settings.redactionTextReplaceDesc')
                  : t('settings.redactionBlackBarDesc')}
              </p>
            </div>
          </div>
        </section>

        {/* AI Detection */}
        <section className="mb-8">
          <h3 className="text-sm font-semibold text-gray-700 mb-3 uppercase tracking-wider">
            {t('settings.aiDetection')}
          </h3>
          <div className="bg-white rounded-lg border border-gray-200 p-4 space-y-4">
            <p className="text-sm text-gray-600">
              {t('settings.aiDetectionDesc')}
            </p>

            {/* Model status */}
            {llmStatus && (
              <div className="flex items-center gap-2">
                <span
                  className={`inline-block w-2 h-2 rounded-full ${
                    llmStatus.loaded
                      ? "bg-green-500"
                      : llmStatus.available
                      ? "bg-yellow-500"
                      : "bg-gray-400"
                  }`}
                />
                <span className="text-sm text-gray-700">
                  {llmStatus.loaded
                    ? `${t('settings.aiModelLoaded')} (${llmStatus.device})`
                    : llmStatus.downloading
                    ? t('settings.aiModelDownloading')
                    : llmStatus.available
                    ? t('settings.aiModelReady')
                    : t('settings.aiModelNotDownloaded')}
                </span>
              </div>
            )}

            {/* Download progress bar */}
            {llmStatus?.downloading && (
              <div className="w-full bg-gray-200 rounded-full h-2">
                <div
                  className="bg-blue-500 h-2 rounded-full transition-all duration-300"
                  style={{ width: `${(llmStatus.downloadProgress * 100).toFixed(0)}%` }}
                />
              </div>
            )}

            {/* Action buttons */}
            <div className="flex gap-2">
              {llmStatus && !llmStatus.available && !llmStatus.downloading && (
                <button
                  onClick={handleDownloadModel}
                  className="px-4 py-2 bg-blue-50 text-blue-600 text-sm rounded-lg hover:bg-blue-100 transition-colors"
                >
                  {t('settings.aiDownloadModel')}
                </button>
              )}
              {llmStatus?.available && !llmStatus.loaded && !llmLoading && (
                <button
                  onClick={handleLoadModel}
                  className="px-4 py-2 bg-blue-50 text-blue-600 text-sm rounded-lg hover:bg-blue-100 transition-colors"
                >
                  {t('settings.aiLoadModel')}
                </button>
              )}
              {llmLoading && (
                <span className="text-sm text-gray-500 py-2">Loading model...</span>
              )}
            </div>

            {/* Enable/disable toggle */}
            {llmStatus?.loaded && (
              <label className="flex items-center gap-3 cursor-pointer">
                <input
                  type="checkbox"
                  checked={llmStatus.enabled}
                  onChange={(e) => handleToggleLlm(e.target.checked)}
                  className="w-4 h-4 text-blue-600 rounded focus:ring-blue-500"
                />
                <span className="text-sm text-gray-700">
                  {t('settings.aiDetectionToggle')}
                </span>
              </label>
            )}
          </div>
        </section>

        {/* Data Retention */}
        <section>
          <h3 className="text-sm font-semibold text-gray-700 mb-3 uppercase tracking-wider">
            {t('settings.dataRetention')}
          </h3>
          <div className="bg-white rounded-lg border border-gray-200 p-4">
            <p className="text-sm text-gray-600 mb-3">
              {t('settings.dataRetentionDesc')}
            </p>
            <button className="px-4 py-2 bg-red-50 text-red-600 text-sm rounded-lg hover:bg-red-100 transition-colors">
              {t('settings.clearAll')}
            </button>
          </div>
        </section>
      </div>
    </div>
  );
}
