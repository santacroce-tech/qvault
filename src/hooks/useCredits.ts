import { useState, useCallback, useEffect } from "react";
import { getCredits } from "../lib/tauri-commands";
import type { CreditInfo } from "../lib/types";

export function useCredits() {
  const [credits, setCredits] = useState<CreditInfo | null>(null);
  const [loading, setLoading] = useState(true);

  const load = useCallback(async () => {
    try {
      const info = await getCredits();
      setCredits(info);
    } catch (e) {
      console.error("Failed to load credits:", e);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    load();
  }, [load]);

  return { credits, loading, reload: load };
}
