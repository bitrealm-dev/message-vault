/** Browser storage keys for Settings → System in the desktop app. */

import { invokeHomeDir } from "./tauri";
import { isTauri } from "./tauri-check";

/** localStorage key for the import staging parent (legacy name kept for saved paths). */
const VAULT_WORKING_DIR_KEY = "mv-vault-working-dir";
const REMEMBER_IMPORTER_PATHS_KEY = "mv-remember-importer-paths";
const IMPORTER_PATHS_KEY = "mv-importer-paths";
const IMPORTER_EXTRA_PATHS_KEY = "mv-importer-extra-paths";

let cachedHomeDir: string | null = null;
let homeDirPromise: Promise<string> | null = null;

/** Default folder name under the user home directory for import staging. */
const IMPORT_STAGING_PARENT = "message-vault";

/**
 * Strip trailing `/` or `\\` without turning a Unix root into an empty string.
 */
export function stripTrailingPathSeparators(path: string): string {
  const trimmed = path.trim();
  const stripped = trimmed.replace(/[/\\]+$/, "");
  if (!stripped && /^[/\\]+$/.test(trimmed)) return "/";
  return stripped;
}

/**
 * True for an absolute folder that is not the filesystem root.
 * Relative paths and `/` would write or open next to the process cwd, or anywhere on disk.
 */
export function isUsableImportStagingParent(path: string): boolean {
  const parent = stripTrailingPathSeparators(path);
  if (!parent || parent === "/") return false;
  if (/^[A-Za-z]:$/.test(parent)) return false;
  if (parent.startsWith("/")) return true;
  if (/^[A-Za-z]:[\\/]/.test(path.trim())) return true;
  if (parent.startsWith("\\\\")) return true;
  return false;
}

/**
 * Default import staging parent: `{home}/message-vault`.
 * When home is empty, returns the relative folder name `message-vault`.
 */
export function defaultImportStagingDir(homeDir: string): string {
  const home = stripTrailingPathSeparators(homeDir);
  if (!home) return IMPORT_STAGING_PARENT;
  if (home === "/") return `/${IMPORT_STAGING_PARENT}`;
  return `${home}/${IMPORT_STAGING_PARENT}`;
}

/** Folder chosen in Settings as the import staging parent. Empty when unset. */
export function getImportStagingDir(): string {
  try {
    return localStorage.getItem(VAULT_WORKING_DIR_KEY)?.trim() || "";
  } catch {
    return "";
  }
}

export function setImportStagingDir(dir: string): void {
  try {
    const trimmed = dir.trim();
    if (trimmed) localStorage.setItem(VAULT_WORKING_DIR_KEY, trimmed);
    else localStorage.removeItem(VAULT_WORKING_DIR_KEY);
  } catch {
    // Private browsing and full storage can throw. Keep the in-memory value.
  }
}

/**
 * Resolved parent folder for import staging (saved override or default).
 * Empty when neither a saved path nor a home directory is available.
 */
export async function resolveImportStagingParent(): Promise<string> {
  const saved = getImportStagingDir();
  if (isUsableImportStagingParent(saved)) {
    return stripTrailingPathSeparators(saved);
  }
  const home = (await getHomeDir()).trim();
  if (!home) return "";
  const fallback = defaultImportStagingDir(home);
  return isUsableImportStagingParent(fallback) ? stripTrailingPathSeparators(fallback) : "";
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

type ImporterExtraRow = {
  attachmentRoot?: string;
  appleContacts?: string;
};

function readImporterExtraPaths(): Record<string, ImporterExtraRow> {
  try {
    const raw = localStorage.getItem(IMPORTER_EXTRA_PATHS_KEY);
    if (!raw) return {};
    const parsed: unknown = JSON.parse(raw);
    if (!parsed || typeof parsed !== "object") return {};
    const out: Record<string, ImporterExtraRow> = {};
    for (const [sourceId, row] of Object.entries(parsed)) {
      if (!row || typeof row !== "object") continue;
      const entry: ImporterExtraRow = {};
      const record = row as Record<string, unknown>;
      if (typeof record.attachmentRoot === "string" && record.attachmentRoot.trim()) {
        entry.attachmentRoot = record.attachmentRoot.trim();
      }
      if (typeof record.appleContacts === "string" && record.appleContacts.trim()) {
        entry.appleContacts = record.appleContacts.trim();
      }
      if (Object.keys(entry).length > 0) out[sourceId] = entry;
    }
    return out;
  } catch {
    return {};
  }
}

function writeImporterExtraPaths(map: Record<string, ImporterExtraRow>): void {
  try {
    if (Object.keys(map).length === 0) localStorage.removeItem(IMPORTER_EXTRA_PATHS_KEY);
    else localStorage.setItem(IMPORTER_EXTRA_PATHS_KEY, JSON.stringify(map));
  } catch {
    // Private browsing and full storage can throw.
  }
}

export type ImporterExtraField = "attachmentRoot" | "appleContacts";

export function getImporterExtraPaths(sourceId: string): {
  attachmentRoot: string;
  appleContacts: string;
} {
  const row = readImporterExtraPaths()[sourceId];
  return {
    attachmentRoot: row?.attachmentRoot ?? "",
    appleContacts: row?.appleContacts ?? "",
  };
}

/** Last paths to show after a source change. Empty when remembering is off. */
export function loadRememberedImportPaths(sourceId: string): {
  backupPath: string;
  attachmentRoot: string;
  appleContacts: string;
} {
  if (!getRememberImporterPaths()) {
    return { backupPath: "", attachmentRoot: "", appleContacts: "" };
  }
  const extras = getImporterExtraPaths(sourceId);
  return {
    backupPath: getImporterPath(sourceId),
    attachmentRoot: extras.attachmentRoot,
    appleContacts: extras.appleContacts,
  };
}

export function setImporterExtraPath(
  sourceId: string,
  field: ImporterExtraField,
  path: string,
): void {
  const map = readImporterExtraPaths();
  const trimmed = path.trim();
  if (trimmed) {
    const row = map[sourceId] ?? {};
    writeImporterExtraPaths({ ...map, [sourceId]: { ...row, [field]: trimmed } });
    return;
  }
  const row = map[sourceId];
  if (!row) return;
  const nextRow: ImporterExtraRow = {};
  for (const [key, value] of Object.entries(row)) {
    if (key !== field && value) nextRow[key as ImporterExtraField] = value;
  }
  const next: Record<string, ImporterExtraRow> = {};
  for (const [key, value] of Object.entries(map)) {
    if (key === sourceId) {
      if (Object.keys(nextRow).length > 0) next[key] = nextRow;
    } else {
      next[key] = value;
    }
  }
  writeImporterExtraPaths(next);
}

/**
 * Short name used in staging folder names.
 * Matches the desktop GUI in `crates/message-vault-io-gui/src/staging.rs`.
 */
function importerSlugForSource(sourceId: string): string {
  if (sourceId === "imessage-ios") return "iphone-ios";
  if (sourceId === "imessage-macos") return "macos";
  if (sourceId === "imessage-jailbreak") return "iphone-jailbreak";
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
 * Join a staging parent folder with `staging-<importer>-YYMMDD-HHMMSS`.
 * When the parent is empty, the path is only the staging folder name.
 */
export function joinImportStagingPath(
  parentDir: string,
  sourceId: string,
  now: Date = new Date(),
): string {
  const name = stagingDirName(sourceId, now);
  const parent = stripTrailingPathSeparators(parentDir);
  if (!parent) return name;
  if (parent === "/") return `/${name}`;
  return `${parent}/${name}`;
}

/**
 * Full path for a new import staging folder under the Settings parent
 * (default `{home}/message-vault`).
 *
 * @throws If neither a saved staging parent nor the user home directory is
 * available. A relative `message-vault/…` path would otherwise be created next
 * to the process working directory (for example the AppImage mount).
 */
export async function resolveImportStagingDir(
  _backupPath: string,
  sourceId: string,
): Promise<string> {
  const parent = await resolveImportStagingParent();
  if (!parent) {
    throw new Error(
      "Could not determine the user home directory. Import staging needs ~/message-vault/.",
    );
  }
  return joinImportStagingPath(parent, sourceId);
}
