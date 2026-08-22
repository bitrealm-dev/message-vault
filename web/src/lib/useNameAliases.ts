import { useSyncExternalStore } from "react";
import { readUseNameAliases, USE_NAME_ALIASES_KEY, writeUseNameAliases } from "./nameAliases";

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

/** Write the "use name aliases" toggle and tell listeners it changed. */
export function setUseNameAliases(on: boolean): void {
  writeUseNameAliases(on);
  emit();
}

/** Current value of the Appearance "Use name aliases" toggle (off by default). */
export function useNameAliases(): boolean {
  return useSyncExternalStore(subscribe, readUseNameAliases, () => false);
}
