import Database from "better-sqlite3";
import fs from "fs";
import path from "path";
import { parse } from "smol-toml";

import { currentAccountId } from "./accountScope";

const DEFAULT_DB = "data/vault.db";
const DEFAULT_DATA_DIR = "data";
const DEFAULT_ASSETS_DIR = "assets";
const DEFAULT_ASSETS_CONVERTED_DIR = "assets_converted";

/** Repo root (parent of web/), detected via config/config.toml. */
export function repoRoot(): string {
  const cwd = process.cwd();
  if (fs.existsSync(path.join(cwd, "config", "config.toml"))) {
    return cwd;
  }
  const parent = path.resolve(cwd, "..");
  if (fs.existsSync(path.join(parent, "config", "config.toml"))) {
    return parent;
  }
  return parent;
}

export function configTomlPath(): string {
  return path.join(repoRoot(), "config", "config.toml");
}

function resolveConfiguredPath(
  configured: string | undefined,
  fallback: string,
): string {
  const rel = configured?.trim() || fallback;
  if (path.isAbsolute(rel)) return rel;
  return path.join(repoRoot(), rel);
}

export type SourcePaths = {
  id: string;
  /** Staging is no longer configured; kept for callers that expect a path. */
  exportDir: string;
  assetsDir: string;
  assetsConvertedDir: string;
};

type RawConfig = {
  paths?: {
    db?: string;
    data_dir?: string;
    assets_dir?: string;
    assets_converted_dir?: string;
  };
};

function loadRawConfig(): RawConfig {
  const configPath = configTomlPath();
  if (!fs.existsSync(configPath)) {
    return {};
  }
  try {
    const text = fs.readFileSync(configPath, "utf8");
    return parse(text) as RawConfig;
  } catch {
    return {};
  }
}

export function dbPath(): string {
  const fromEnv = process.env.VAULT_DB?.trim();
  if (fromEnv) {
    return path.isAbsolute(fromEnv) ? fromEnv : path.join(repoRoot(), fromEnv);
  }
  const cfg = loadRawConfig();
  return resolveConfiguredPath(cfg.paths?.db, DEFAULT_DB);
}

export function dataDir(): string {
  const fromEnv = process.env.VAULT_DATA_DIR?.trim();
  if (fromEnv) {
    return path.isAbsolute(fromEnv) ? fromEnv : path.join(repoRoot(), fromEnv);
  }
  const cfg = loadRawConfig();
  return resolveConfiguredPath(cfg.paths?.data_dir, DEFAULT_DATA_DIR);
}

export function accountDataDir(accountId: string): string {
  return path.join(dataDir(), accountId);
}

export function assetsDirName(): string {
  return loadRawConfig().paths?.assets_dir?.trim() || DEFAULT_ASSETS_DIR;
}

export function assetsConvertedDirName(): string {
  return (
    loadRawConfig().paths?.assets_converted_dir?.trim() ||
    DEFAULT_ASSETS_CONVERTED_DIR
  );
}

function sourceIdsForAccount(accountId: string): string[] {
  const ids = new Set<string>();
  const dbFile = dbPath();
  if (fs.existsSync(dbFile)) {
    try {
      const db = new Database(dbFile, { readonly: true });
      try {
        const rows = db
          .prepare(
            `SELECT DISTINCT m.source AS source
             FROM messages m
             JOIN conversations c ON c.id = m.conversation_id
             WHERE c.account_id = ?
               AND m.source IS NOT NULL
               AND TRIM(m.source) != ''
             ORDER BY m.source`,
          )
          .all(accountId) as Array<{ source: string }>;
        for (const row of rows) {
          if (row.source?.trim()) ids.add(row.source.trim());
        }
      } finally {
        db.close();
      }
    } catch {
      // Fall through to filesystem discovery.
    }
  }

  const accountRoot = accountDataDir(accountId);
  if (fs.existsSync(accountRoot)) {
    for (const entry of fs.readdirSync(accountRoot, { withFileTypes: true })) {
      if (!entry.isDirectory()) continue;
      const id = entry.name;
      if (id === "." || id === ".." || id.includes("/") || id.includes("\\")) {
        continue;
      }
      const assets = path.join(accountRoot, id, assetsDirName());
      if (fs.existsSync(assets)) {
        ids.add(id);
      }
    }
  }

  return [...ids].sort();
}

/** Per-account import sources with resolved asset roots (from DB + on-disk folders). */
export function loadSources(accountId = currentAccountId()): SourcePaths[] {
  const data = dataDir();
  const assetsName = assetsDirName();
  const convertedName = assetsConvertedDirName();

  return sourceIdsForAccount(accountId).map((id) => ({
    id,
    exportDir: path.join(data, accountId, id, "staging"),
    assetsDir: path.join(data, accountId, id, assetsName),
    assetsConvertedDir: path.join(data, accountId, id, convertedName),
  }));
}

export function sourceById(id: string): SourcePaths | undefined {
  return loadSources().find((s) => s.id === id);
}
