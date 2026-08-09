"use client";

import { useEffect, useState } from "react";

export function useVaultReadOnly(): boolean | null {
  const [readOnly, setReadOnly] = useState<boolean | null>(null);

  useEffect(() => {
    let cancelled = false;
    fetch("/api/settings/account")
      .then((r) => r.json())
      .then((data: { readOnly?: boolean }) => {
        if (!cancelled) setReadOnly(data.readOnly === true);
      })
      .catch(() => {
        if (!cancelled) setReadOnly(false);
      });
    return () => {
      cancelled = true;
    };
  }, []);

  return readOnly;
}
