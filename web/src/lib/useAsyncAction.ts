import { useCallback, useState } from "react";

export function useAsyncAction(): {
  busy: boolean;
  error: string;
  run: (fn: () => Promise<void>) => Promise<void>;
  clearError: () => void;
} {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");

  const clearError = useCallback(() => setError(""), []);

  const run = useCallback(async (fn: () => Promise<void>) => {
    setBusy(true);
    setError("");
    try {
      await fn();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }, []);

  return { busy, error, run, clearError };
}
