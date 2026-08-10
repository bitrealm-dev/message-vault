/** localStorage keys for Settings → System (Tauri desktop). */

export const VAULT_WORKING_DIR_KEY = "mv-vault-working-dir";
export const REMEMBER_IMPORTER_PATHS_KEY = "mv-remember-importer-paths";
export const IMPORTER_PATHS_KEY = "mv-importer-paths";

export function getVaultWorkingDir(): string {
  try {
    return localStorage.getItem(VAULT_WORKING_DIR_KEY)?.trim() || "";
  } catch {
    return "";
  }
}

export function setVaultWorkingDir(dir: string): void {
  try {
    const trimmed = dir.trim();
    if (trimmed) localStorage.setItem(VAULT_WORKING_DIR_KEY, trimmed);
    else localStorage.removeItem(VAULT_WORKING_DIR_KEY);
  } catch {
    // ignore quota / private mode
  }
}

export function getRememberImporterPaths(): boolean {
  try {
    return localStorage.getItem(REMEMBER_IMPORTER_PATHS_KEY) === "1";
  } catch {
    return false;
  }
}

export function setRememberImporterPaths(on: boolean): void {
  try {
    if (on) localStorage.setItem(REMEMBER_IMPORTER_PATHS_KEY, "1");
    else localStorage.removeItem(REMEMBER_IMPORTER_PATHS_KEY);
  } catch {
    // ignore
  }
}

function readImporterPaths(): Record<string, string> {
  try {
    const raw = localStorage.getItem(IMPORTER_PATHS_KEY);
    if (!raw) return {};
    const parsed = JSON.parse(raw) as unknown;
    if (!parsed || typeof parsed !== "object") return {};
    const out: Record<string, string> = {};
    for (const [k, v] of Object.entries(parsed as Record<string, unknown>)) {
      if (typeof v === "string" && v.trim()) out[k] = v.trim();
    }
    return out;
  } catch {
    return {};
  }
}

function writeImporterPaths(map: Record<string, string>): void {
  try {
    if (Object.keys(map).length === 0) localStorage.removeItem(IMPORTER_PATHS_KEY);
    else localStorage.setItem(IMPORTER_PATHS_KEY, JSON.stringify(map));
  } catch {
    // ignore
  }
}

export function getImporterPath(sourceId: string): string {
  return readImporterPaths()[sourceId] ?? "";
}

export function setImporterPath(sourceId: string, path: string): void {
  const map = readImporterPaths();
  const trimmed = path.trim();
  if (trimmed) map[sourceId] = trimmed;
  else delete map[sourceId];
  writeImporterPaths(map);
}

/**
 * Importer slug in staging folder names — matches Slint GUI
 * (`crates/message-vault-io-gui/src/staging.rs`).
 */
export function importerSlugForSource(sourceId: string): string {
  if (sourceId === "imessage-ios") return "iphone-ios";
  if (sourceId === "imessage-macos") return "macos";
  return sourceId;
}

/** Local `YYMMDD-HHMMSS` — same shape as Slint `chrono` `%y%m%d-%H%M%S`. */
export function formatStagingTimestamp(now: Date = new Date()): string {
  const pad = (n: number) => String(n).padStart(2, "0");
  const yy = pad(now.getFullYear() % 100);
  const mm = pad(now.getMonth() + 1);
  const dd = pad(now.getDate());
  const hh = pad(now.getHours());
  const mi = pad(now.getMinutes());
  const ss = pad(now.getSeconds());
  return `${yy}${mm}${dd}-${hh}${mi}${ss}`;
}

/** `staging-<importer>-YYMMDD-HHMMSS` — matches Slint `staging_dir_name`. */
export function stagingDirName(sourceId: string, now: Date = new Date()): string {
  return `staging-${importerSlugForSource(sourceId)}-${formatStagingTimestamp(now)}`;
}

/**
 * When Vault Working Directory is set: `{workingDir}/staging-<importer>-YYMMDD-HHMMSS`
 * (Slint import staging rules). Otherwise: `{backupPath}/../extract-output`.
 */
export function resolveImportStagingDir(backupPath: string, sourceId: string): string {
  const working = getVaultWorkingDir();
  if (working) {
    return working.replace(/[/\\]+$/, "") + "/" + stagingDirName(sourceId);
  }
  return `${backupPath}/../extract-output`;
}
