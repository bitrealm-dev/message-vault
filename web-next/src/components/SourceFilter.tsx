"use client";

import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useState,
  type ReactNode,
} from "react";

type SourceFilterValue = {
  sources: string[];
  /** null = combined (all sources, soft-deduped) */
  source: string | null;
  setSource: (source: string | null) => void;
  sourceQuery: string;
};

const SourceFilterContext = createContext<SourceFilterValue | null>(null);

const STORAGE_KEY = "mv-source-filter";

export function SourceFilterProvider({ children }: { children: ReactNode }) {
  const [sources, setSources] = useState<string[]>([]);
  const [source, setSourceState] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    fetch("/api/sources")
      .then((r) => r.json())
      .then((data: { sources?: Array<{ id: string }> }) => {
        if (cancelled) return;
        const ids = (data.sources ?? []).map((s) => s.id);
        setSources(ids);
        const saved = localStorage.getItem(STORAGE_KEY);
        if (saved === null || saved === "" || saved === "all") {
          setSourceState(null);
        } else if (ids.includes(saved)) {
          setSourceState(saved);
        }
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, []);

  const setSource = useCallback((next: string | null) => {
    setSourceState(next);
    localStorage.setItem(STORAGE_KEY, next ?? "all");
  }, []);

  const value = useMemo(
    () => ({
      sources,
      source,
      setSource,
      sourceQuery: source ? `&source=${encodeURIComponent(source)}` : "",
    }),
    [sources, source, setSource],
  );

  return (
    <SourceFilterContext.Provider value={value}>
      {children}
    </SourceFilterContext.Provider>
  );
}

export function useSourceFilter(): SourceFilterValue {
  const ctx = useContext(SourceFilterContext);
  if (!ctx) {
    return {
      sources: [],
      source: null,
      setSource: () => {},
      sourceQuery: "",
    };
  }
  return ctx;
}
