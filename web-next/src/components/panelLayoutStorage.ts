"use client";

import { useSyncExternalStore } from "react";
import type { LayoutStorage } from "react-resizable-panels";

const noopStorage: LayoutStorage = {
  getItem: () => null,
  setItem: () => {},
};

const browserStorage: LayoutStorage = {
  getItem: (key) => localStorage.getItem(key),
  setItem: (key, value) => {
    localStorage.setItem(key, value);
  },
};

/** localStorage does not emit to this subscribe; server/client snapshot switch is enough. */
function subscribe(_onStoreChange: () => void) {
  void _onStoreChange;
  return () => {};
}

/**
 * SSR-safe storage for `useDefaultLayout`.
 * Noop on the server and during hydration so markup matches, then localStorage
 * after hydrate (restores saved sizes without a hydration mismatch).
 */
export function usePanelLayoutStorage(): LayoutStorage {
  return useSyncExternalStore(
    subscribe,
    () => browserStorage,
    () => noopStorage,
  );
}
