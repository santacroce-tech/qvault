import { useState, useCallback, useEffect } from "react";
import { checkLicense } from "../lib/tauri-commands";

// In Tauri dev mode, __TAURI_INTERNALS__ is present and the backend
// already returns true for checkLicense — but guard the frontend too.
const isDevMode = typeof window !== "undefined" && !!(window as any).__TAURI_INTERNALS__;

export function useLicense() {
  const [licensed, setLicensed] = useState<boolean>(isDevMode);
  const [loading, setLoading] = useState(!isDevMode);

  const load = useCallback(async () => {
    try {
      const isLicensed = await checkLicense();
      setLicensed(isLicensed);
    } catch (e) {
      if (isDevMode) {
        console.debug("License check failed in dev mode, defaulting to licensed:", e);
        setLicensed(true);
      } else {
        console.error("Failed to check license:", e);
        setLicensed(false);
      }
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    load();
  }, [load]);

  return { licensed, loading, reload: load };
}
