import { useState } from "react";

export default function LicenseActivation() {
  const [key, setKey] = useState("");
  const [status, setStatus] = useState<"idle" | "loading" | "success" | "error">("idle");

  const handleActivate = async () => {
    if (!key.trim()) return;
    setStatus("loading");
    try {
      // TODO: call activate_license command
      setStatus("success");
    } catch {
      setStatus("error");
    }
  };

  return (
    <div className="bg-white rounded-lg border border-gray-200 p-4">
      <label className="block text-sm font-medium text-gray-700 mb-2">
        License Key
      </label>
      <div className="flex gap-2">
        <input
          type="text"
          value={key}
          onChange={(e) => setKey(e.target.value)}
          placeholder="QVLT-XXXX-XXXX-XXXX"
          className="flex-1 px-3 py-2 border border-gray-300 rounded-lg text-sm font-mono focus:outline-none focus:ring-2 focus:ring-blue-500"
        />
        <button
          onClick={handleActivate}
          disabled={status === "loading"}
          className="px-4 py-2 bg-blue-600 text-white text-sm rounded-lg hover:bg-blue-700 transition-colors disabled:opacity-50"
        >
          {status === "loading" ? "..." : "Activate"}
        </button>
      </div>
      {status === "success" && (
        <p className="mt-2 text-sm text-green-600">License activated!</p>
      )}
      {status === "error" && (
        <p className="mt-2 text-sm text-red-600">Invalid license key.</p>
      )}
    </div>
  );
}
