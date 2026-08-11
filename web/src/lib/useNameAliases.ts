import { useSyncExternalStore } from "react";
import {
  USE_NAME_ALIASES_KEY,
  readUseNameAliases,
  writeUseNameAliases,
} from "./nameAliases";

const listeners = new Set<() => void>();

function subscribe(onStoreChange: () => void): () => void {
  listeners.add(onStoreChange);
  const onStorage = (e: StorageEvent) => {
    if (e.key === USE_NAME_ALIASES_KEY || e.key === null) onStoreChange();
  };
  window.addEventListener("storage", onStorage);
  return () => {
    listeners.delete(onStoreChange);
    window.removeEventListener("storage", onStorage);
  };
}

function emit(): void {
  for (const l of listeners) l();
}

export function setUseNameAliases(on: boolean): void {
  writeUseNameAliases(on);
  emit();
}

/** Reactive read of the Appearance “Use name aliases” toggle (default off). */
export function useNameAliases(): boolean {
  return useSyncExternalStore(subscribe, readUseNameAliases, () => false);
}
