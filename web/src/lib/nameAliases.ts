/** Appearance preference: show per-identity aliases instead of preferred names. */

export const USE_NAME_ALIASES_KEY = "mv-use-name-aliases";

/** Read the "use name aliases" toggle from browser storage. */
export function readUseNameAliases(): boolean {
  try {
    return window.localStorage.getItem(USE_NAME_ALIASES_KEY) === "1";
  } catch {
    return false;
  }
}

/** Save the "use name aliases" toggle. */
export function writeUseNameAliases(on: boolean): void {
  try {
    if (on) window.localStorage.setItem(USE_NAME_ALIASES_KEY, "1");
    else window.localStorage.removeItem(USE_NAME_ALIASES_KEY);
  } catch {
    // Private browsing and full storage can throw.
  }
}

/** Pick the name shown for a person: preferred name, alias, or the raw handle. */
export function personDisplayLabel(
  opts: {
    preferredName?: string | null;
    nameAlias?: string | null;
    handle: string;
  },
  useAliases: boolean,
): string {
  const preferred = opts.preferredName?.trim() || null;
  const alias = opts.nameAlias?.trim() || null;
  if (useAliases) {
    return alias || preferred || opts.handle;
  }
  return preferred || alias || opts.handle;
}
