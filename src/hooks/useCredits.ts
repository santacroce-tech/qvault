import { useState, useCallback, useEffect } from "react";
import { checkLicense } from "../lib/tauri-commands";

export function useLicense() {
  const [licensed, setLicensed] = useState<boolean>(false);
  const [loading, setLoading] = useState(true);

  const load = useCallback(async () => {
    try {
      const isLicensed = await checkLicense();
      setLicensed(isLicensed);
    } catch (e) {
      console.error("Failed to check license:", e);
      setLicensed(false);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    load();
  }, [load]);

  return { licensed, loading, reload: load };
}
