import { apiClient } from "./api";
import type { PathStat } from "./tauri";

/** Where a live import session is. Mirrors the vault's `ImportStage`. */
export type ImportStage =
  | "parse"
  | "write"
  | "awaiting_gate_1"
  | "transcode"
  | "awaiting_gate_2"
  | "pushing";

/** Identity of the backup a session was started from. */
export type SourceFingerprint = {
  path: string;
  size_bytes: number;
  modified_unix_ms: number | null;
  /** Filled in after parse; null until then. */
  message_count: number | null;
};

/** The account's live import session, as the vault reports it. */
export type ActiveImportSession = {
  id: number;
  source: string;
  mode: string;
  status: string;
  started_at: string;
  stage: ImportStage | null;
  staging_dir: string | null;
  device_id: string | null;
  form: unknown;
  source_fingerprint: SourceFingerprint | null;
  /** Addresses the backup's device sent from (JSON array), or null. */
  source_identities: unknown;
  /** What was approved at the last gate passed, or null. Mirrors what
   * `setImportStage`'s `approvedPlan` argument last wrote. */
  summary: unknown;
};

/** The account's live session, or null when there is none. */
export async function getActiveImportSession(): Promise<ActiveImportSession | null> {
  const res = await apiClient.get<{ session: ActiveImportSession | null }>("/v1/imports/active");
  return res.session ?? null;
}

/**
 * Move a live session to another stage.
 *
 * `approvedPlan`, when given, is recorded as the session's `summary_json` —
 * what the user approved at the gate they just passed. Omitting it leaves
 * whatever plan is already stored untouched; it is never nulled out.
 */
export async function setImportStage(
  id: number,
  stage: ImportStage,
  approvedPlan?: unknown,
): Promise<void> {
  await apiClient.post(`/v1/imports/${String(id)}/stage`, { stage, summary: approvedPlan });
}

/** Close a session the user gave up on, freeing the account's slot. */
export async function discardImportSession(id: number): Promise<void> {
  await apiClient.post(`/v1/imports/${String(id)}/discard`, {});
}

/**
 * Identity of the backup this session reads.
 *
 * The message count is unknown until parse finishes, so it starts null
 * and is filled in afterwards.
 *
 * The size and mtime come from a stat of the path itself, so for a
 * directory source -- an iOS backup folder, a WhatsApp folder -- they
 * describe the directory entry rather than its contents, and neither moves
 * when a file inside it grows. Nothing reads this fingerprint back yet;
 * whatever does will need its own answer for directories.
 */
export function buildSourceFingerprint(path: string, stat: PathStat): SourceFingerprint {
  return {
    path,
    size_bytes: stat.sizeBytes,
    modified_unix_ms: stat.modifiedUnixMs,
    message_count: null,
  };
}
