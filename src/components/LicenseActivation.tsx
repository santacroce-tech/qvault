import { useState, useEffect } from "react";
import { getLicense, activateLicense, deactivateLicense } from "../lib/tauri-commands";
import type { LicenseInfo } from "../lib/types";

export default function LicenseActivation() {
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
    const expiresDate = new Date(license.expires_at).toLocaleDateString();
    const daysLeft = Math.ceil(
      (new Date(license.expires_at).getTime() - Date.now()) / (1000 * 60 * 60 * 24)
    );

    return (
      <div className="bg-white rounded-lg border border-gray-200 p-4">
        <div className="flex items-center justify-between mb-3">
          <h3 className="font-medium text-gray-900">License</h3>
          <span className="inline-flex items-center px-2.5 py-0.5 rounded-full text-xs font-medium bg-green-100 text-green-800">
            Active
          </span>
        </div>
        <div className="space-y-2 text-sm text-gray-600">
          <div className="flex justify-between">
            <span>Plan</span>
            <span className="font-medium text-gray-900">Yearly — Unlimited Documents</span>
          </div>
          <div className="flex justify-between">
            <span>Key</span>
            <span className="font-mono text-gray-500">
              {license.license_key.slice(0, 9)}...{license.license_key.slice(-4)}
            </span>
          </div>
          <div className="flex justify-between">
            <span>Expires</span>
            <span className={daysLeft < 30 ? "text-amber-600 font-medium" : ""}>
              {expiresDate} ({daysLeft} days left)
            </span>
          </div>
        </div>
        <button
          onClick={handleDeactivate}
          className="mt-4 text-xs text-red-500 hover:text-red-700 transition-colors"
        >
          Deactivate License
        </button>
      </div>
    );
  }

  // Activation form
  return (
    <div className="bg-white rounded-lg border border-gray-200 p-4">
      <h3 className="font-medium text-gray-900 mb-1">Activate License</h3>
      <p className="text-sm text-gray-500 mb-3">
        A yearly license gives you unlimited document processing.
      </p>
      <div className="flex gap-2">
        <input
          type="text"
          value={key}
          onChange={(e) => setKey(e.target.value.toUpperCase())}
          placeholder="QVLT-XXXX-XXXX-XXXX"
          className="flex-1 px-3 py-2 border border-gray-300 rounded-lg text-sm font-mono focus:outline-none focus:ring-2 focus:ring-blue-500"
          onKeyDown={(e) => e.key === "Enter" && handleActivate()}
        />
        <button
          onClick={handleActivate}
          disabled={status === "loading" || !key.trim()}
          className="px-4 py-2 bg-blue-600 text-white text-sm rounded-lg hover:bg-blue-700 transition-colors disabled:opacity-50"
        >
          {status === "loading" ? "..." : "Activate"}
        </button>
      </div>
      {status === "success" && (
        <p className="mt-2 text-sm text-green-600">License activated successfully!</p>
      )}
      {status === "error" && (
        <p className="mt-2 text-sm text-red-600">{errorMsg || "Invalid license key."}</p>
      )}
    </div>
  );
}
