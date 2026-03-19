import { useState, useEffect } from "react";
import { checkLicense } from "../lib/tauri-commands";

export default function LicenseStatus() {
  const [licensed, setLicensed] = useState<boolean | null>(null);

  useEffect(() => {
    checkLicense()
      .then(setLicensed)
      .catch(() => setLicensed(false));
  }, []);

  if (licensed === null) return null;

  if (licensed) {
    return (
      <div className="mb-6 p-3 bg-green-50 border border-green-200 rounded-lg text-sm text-green-700 flex items-center gap-2">
        <svg className="w-4 h-4" fill="currentColor" viewBox="0 0 20 20">
          <path fillRule="evenodd" d="M10 18a8 8 0 100-16 8 8 0 000 16zm3.707-9.293a1 1 0 00-1.414-1.414L9 10.586 7.707 9.293a1 1 0 00-1.414 1.414l2 2a1 1 0 001.414 0l4-4z" clipRule="evenodd" />
        </svg>
        Licensed — Unlimited documents
      </div>
    );
  }

  return (
    <div className="mb-6 p-3 bg-amber-50 border border-amber-200 rounded-lg text-sm text-amber-700">
      No active license. Please activate a yearly license in Settings to process documents.
    </div>
  );
}
