/** Browser storage keys for Settings → System in the desktop app. */

import { invokeHomeDir } from "./tauri";
import { isTauri } from "./tauri-check";

const VAULT_WORKING_DIR_KEY = "mv-vault-working-dir";
const REMEMBER_IMPORTER_PATHS_KEY = "mv-remember-importer-paths";
const IMPORTER_PATHS_KEY = "mv-importer-paths";

let cachedHomeDir: string | null = null;
let homeDirPromise: Promise<string> | null = null;

/** Parent folder under the user home directory that holds import staging folders. */
const IMPORT_STAGING_PARENT = "message-vault";

/** Folder chosen in Settings as the vault working directory. Empty when unset. */
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
    // Private browsing and full storage can throw. Keep the in-memory value.
  }
}

/** User home folder from the desktop app. Empty in the browser or when lookup fails. */
export async function getHomeDir(): Promise<string> {
  if (cachedHomeDir != null) return cachedHomeDir;
  if (!isTauri()) {
    cachedHomeDir = "";
    return cachedHomeDir;
  }
  if (!homeDirPromise) {
    homeDirPromise = invokeHomeDir()
      .then((info) => {
        cachedHomeDir = info.path.trim();
        return cachedHomeDir;
      })
      .catch(() => {
        cachedHomeDir = "";
        return cachedHomeDir;
      });
  }
  return homeDirPromise;
}

/** True when Import should reuse the last backup folder for each source. */
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
    // Private browsing and full storage can throw.
  }
}

function readImporterPaths(): Record<string, string> {
  try {
    const raw = localStorage.getItem(IMPORTER_PATHS_KEY);
    if (!raw) return {};
    const parsed: unknown = JSON.parse(raw);
    if (!parsed || typeof parsed !== "object") return {};
    const out: Record<string, string> = {};
    for (const [k, v] of Object.entries(parsed)) {
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
    // Private browsing and full storage can throw.
  }
}

/** Last backup folder remembered for this import source. */
export function getImporterPath(sourceId: string): string {
  return readImporterPaths()[sourceId] ?? "";
}

export function setImporterPath(sourceId: string, path: string): void {
  const map = readImporterPaths();
  const trimmed = path.trim();
  if (trimmed) {
    writeImporterPaths({ ...map, [sourceId]: trimmed });
    return;
  }
  const next: Record<string, string> = {};
  for (const [key, value] of Object.entries(map)) {
    if (key !== sourceId) next[key] = value;
  }
  writeImporterPaths(next);
}

/**
 * Short name used in staging folder names.
 * Matches the desktop GUI in `crates/message-vault-io-gui/src/staging.rs`.
 */
function importerSlugForSource(sourceId: string): string {
  if (sourceId === "imessage-ios") return "iphone-ios";
  if (sourceId === "imessage-macos") return "macos";
  return sourceId;
}

/** Local date and time as `YYMMDD-HHMMSS`, matching the desktop GUI. */
function formatStagingTimestamp(now: Date = new Date()): string {
  const pad = (n: number) => String(n).padStart(2, "0");
  const yy = pad(now.getFullYear() % 100);
  const mm = pad(now.getMonth() + 1);
  const dd = pad(now.getDate());
  const hh = pad(now.getHours());
  const mi = pad(now.getMinutes());
  const ss = pad(now.getSeconds());
  return `${yy}${mm}${dd}-${hh}${mi}${ss}`;
}

/** Staging folder name: `staging-<importer>-YYMMDD-HHMMSS`. */
function stagingDirName(sourceId: string, now: Date = new Date()): string {
  return `staging-${importerSlugForSource(sourceId)}-${formatStagingTimestamp(now)}`;
}

/**
 * Join the user home folder with `message-vault/staging-<importer>-YYMMDD-HHMMSS`.
 * When home is empty (browser builds, failed lookup), the path is relative.
 */
export function joinImportStagingPath(
  homeDir: string,
  sourceId: string,
  now: Date = new Date(),
): string {
  const name = stagingDirName(sourceId, now);
  const home = homeDir.replace(/[/\\]+$/, "");
  if (!home) {
    return `${IMPORT_STAGING_PARENT}/${name}`;
  }
  return `${home}/${IMPORT_STAGING_PARENT}/${name}`;
}

/**
 * Full path for a new import staging folder.
 * Always `{home}/message-vault/staging-<importer>-YYMMDD-HHMMSS`.
 */
export async function resolveImportStagingDir(
  _backupPath: string,
  sourceId: string,
): Promise<string> {
  return joinImportStagingPath(await getHomeDir(), sourceId);
}
