"use client";

import { useCallback, useEffect, useState } from "react";

/**
 * SSR-safe enum persisted in localStorage.
 * First paint uses `fallback`; client hydrates from storage in an effect.
 */
export function usePersistedEnum<T extends string>(
  key: string,
  allowed: readonly T[],
  fallback: T,
): [T, (next: T) => void] {
  const [value, setValueState] = useState<T>(fallback);
  const allowedKey = allowed.join("\0");

  useEffect(() => {
    const raw = window.localStorage.getItem(key);
    if (raw != null && (allowed as readonly string[]).includes(raw)) {
      setValueState(raw as T);
    }
    // allowed identity varies; allowedKey is the stable membership signal
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [key, allowedKey]);

  const setValue = useCallback(
    (next: T) => {
      setValueState(next);
      window.localStorage.setItem(key, next);
    },
    [key],
  );

  return [value, setValue];
}
