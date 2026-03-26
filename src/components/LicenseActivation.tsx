import { useState, useEffect } from "react";
import { useTranslation } from "react-i18next";
import { getLicense, activateLicense, deactivateLicense } from "../lib/tauri-commands";
import type { LicenseInfo } from "../lib/types";

export default function LicenseActivation() {
  const { t, i18n } = useTranslation();
  const [key, setKey] = useState("");
  const [license, setLicense] = useState<LicenseInfo | null>(null);
  const [status, setStatus] = useState<"idle" | "loading" | "success" | "error">("idle");
  const [errorMsg, setErrorMsg] = useState("");
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    loadLicense();
  }, []);

  const loadLicense = async () => {
    try {
      const lic = await getLicense();
      setLicense(lic);
    } catch (e) {
      console.error("Failed to load license:", e);
    } finally {
      setLoading(false);
    }
  };

  const handleActivate = async () => {
    if (!key.trim()) return;
    setStatus("loading");
    setErrorMsg("");
    try {
      const lic = await activateLicense(key.trim());
      setLicense(lic);
      setStatus("success");
      setKey("");
    } catch (e: any) {
      setErrorMsg(typeof e === "string" ? e : e.message || "Activation failed");
      setStatus("error");
    }
  };

  const handleDeactivate = async () => {
    try {
      await deactivateLicense();
      setLicense(null);
      setStatus("idle");
    } catch (e) {
      console.error("Failed to deactivate:", e);
    }
  };

  if (loading) return null;

  // Active license view
  if (license && license.status === "active") {
    const expiresDate = new Date(license.expires_at).toLocaleDateString(i18n.language);
    const daysLeft = Math.ceil(
      (new Date(license.expires_at).getTime() - Date.now()) / (1000 * 60 * 60 * 24)
    );

    return (
      <div className="bg-white rounded-lg border border-gray-200 p-4">
        <div className="flex items-center justify-between mb-3">
          <h3 className="font-medium text-gray-900">{t('license.title')}</h3>
          <span className="inline-flex items-center px-2.5 py-0.5 rounded-full text-xs font-medium bg-green-100 text-green-800">
            {t('license.active')}
          </span>
        </div>
        <div className="space-y-2 text-sm text-gray-600">
          <div className="flex justify-between">
            <span>{t('license.plan')}</span>
            <span className="font-medium text-gray-900">{t('license.planDetail')}</span>
          </div>
          <div className="flex justify-between">
            <span>{t('license.key')}</span>
            <span className="font-mono text-gray-500">
              {license.license_key.slice(0, 9)}...{license.license_key.slice(-4)}
            </span>
          </div>
          <div className="flex justify-between">
            <span>{t('license.expires')}</span>
            <span className={daysLeft < 30 ? "text-amber-600 font-medium" : ""}>
              {t('license.daysLeft', { date: expiresDate, days: daysLeft })}
            </span>
          </div>
        </div>
        <button
          onClick={handleDeactivate}
          className="mt-4 text-xs text-red-500 hover:text-red-700 transition-colors"
        >
          {t('license.deactivate')}
        </button>
      </div>
    );
  }

  // Activation form
  return (
    <div className="bg-white rounded-lg border border-gray-200 p-4">
      <h3 className="font-medium text-gray-900 mb-1">{t('license.activateTitle')}</h3>
      <p className="text-sm text-gray-500 mb-3">
        {t('license.activateDesc')}
      </p>
      <div className="flex gap-2">
        <input
          type="text"
          value={key}
          onChange={(e) => setKey(e.target.value.toUpperCase())}
          placeholder={t('license.placeholder')}
          className="flex-1 px-3 py-2 border border-gray-300 rounded-lg text-sm font-mono focus:outline-none focus:ring-2 focus:ring-blue-500"
          onKeyDown={(e) => e.key === "Enter" && handleActivate()}
        />
        <button
          onClick={handleActivate}
          disabled={status === "loading" || !key.trim()}
          className="px-4 py-2 bg-blue-600 text-white text-sm rounded-lg hover:bg-blue-700 transition-colors disabled:opacity-50"
        >
          {status === "loading" ? t('license.activating') : t('license.activate')}
        </button>
      </div>
      {status === "success" && (
        <p className="mt-2 text-sm text-green-600">{t('license.success')}</p>
      )}
      {status === "error" && (
        <p className="mt-2 text-sm text-red-600">{errorMsg || t('license.defaultError')}</p>
      )}
    </div>
  );
}
