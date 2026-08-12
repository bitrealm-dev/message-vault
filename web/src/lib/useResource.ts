import { useCallback, useEffect, useRef, useState } from "react";

export function useResource<T>(
  key: string | null,
  fetcher: (signal: AbortSignal) => Promise<T>,
): { data: T | null; loading: boolean; error: string; reload: () => void } {
  const [data, setData] = useState<T | null>(null);
  const [loading, setLoading] = useState(key !== null);
  const [error, setError] = useState("");
  const [reloadToken, setReloadToken] = useState(0);

  const fetcherRef = useRef(fetcher);
  fetcherRef.current = fetcher;

  const reload = useCallback(() => setReloadToken((t) => t + 1), []);

  useEffect(() => {
    if (key === null) {
      setData(null);
      setLoading(false);
      setError("");
      return;
    }

    const controller = new AbortController();
    setLoading(true);
    setError("");

    void fetcherRef
      .current(controller.signal)
      .then((result) => {
        if (!controller.signal.aborted) {
          setData(result);
        }
      })
      .catch((e: unknown) => {
        if (!controller.signal.aborted) {
          setError(String(e));
          setData(null);
        }
      })
      .finally(() => {
        if (!controller.signal.aborted) {
          setLoading(false);
        }
      });

    return () => controller.abort();
  }, [key, reloadToken]);

  return { data, loading, error, reload };
}
