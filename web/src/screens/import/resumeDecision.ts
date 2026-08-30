import type { ActiveImportSession, SourceFingerprint } from "../../lib/importSession";
import type { PathStat } from "../../lib/tauri";

/** What entering Import should do about a session that already exists. */
export type ResumeDecision = {
  kind:
    | "none"
    | "other_device"
    | "folder_missing"
    | "resume_push"
    // A session waiting at either approval gate: the summary is recomputed
    // fresh from the folder (decision 39) and shown again, nothing restored.
    | "resume_gate"
    // A session that died mid media pass: the pass re-runs over whatever
    // originals it had not reached yet (Task 3 makes this safe), then
    // continues to Gate 2 exactly as the normal flow does.
    | "resume_media"
    // A session whose copy was interrupted: the exporter reads the backup
    // again and skips the conversations already written.
    | "resume_write"
    // The backup this session was reading is not the one on disk now, so
    // copying more of it into the same folder would mix two sources.
    | "source_changed"
    | "restart"
    // resumeDecisionFor never returns this: it has no way to know whether a
    // session's stored form snapshot is readable. The screen constructs it
    // itself when restoreFormFromSnapshot rejects the snapshot at the point
    // of trying to resume or restart.
    | "settings_unreadable";
  /** Whether staged work can be picked up rather than redone. */
  canResume: boolean;
  session: ActiveImportSession | null;
};

/**
 * Decide what to show when Import opens and the vault reports a session.
 *
 * Pure so the table can be read and tested on its own: the caller does the
 * network and filesystem work and hands the answers in.
 *
 * A session with no recorded device is treated as this install's. The
 * column is new, so an older session predates it, and locking someone out
 * of their own staged work over a missing field would be worse than the
 * rare case of two installs sharing a vault.
 */
export function resumeDecisionFor(args: {
  session: ActiveImportSession | null;
  deviceId: string;
  folderExists: boolean;
  fingerprint: FingerprintCheck;
}): ResumeDecision {
  const { session, deviceId, folderExists, fingerprint } = args;
  if (!session) return { kind: "none", canResume: false, session: null };
  if (session.device_id && session.device_id !== deviceId) {
    return { kind: "other_device", canResume: false, session };
  }
  if (!session.staging_dir || !folderExists) {
    return { kind: "folder_missing", canResume: false, session };
  }
  if (session.stage === "pushing") {
    return { kind: "resume_push", canResume: true, session };
  }
  if (session.stage === "awaiting_gate_1" || session.stage === "awaiting_gate_2") {
    return { kind: "resume_gate", canResume: true, session };
  }
  if (session.stage === "transcode") {
    return { kind: "resume_media", canResume: true, session };
  }
  // Only the copy cares whether the backup still matches: every later stage
  // works from the staged folder, not from the source.
  if (session.stage === "write") {
    if (fingerprint === "mismatch" || fingerprint === "source_missing") {
      return { kind: "source_changed", canResume: false, session };
    }
    return { kind: "resume_write", canResume: true, session };
  }
  return { kind: "restart", canResume: false, session };
}

/** How a session's stored backup fingerprint compares to the backup now. */
export type FingerprintCheck = "match" | "mismatch" | "source_missing" | "unknown";

/**
 * Compare the fingerprint a session recorded against the source on disk.
 *
 * Directory sources carry the blind spot `buildSourceFingerprint` documents:
 * a stat of the directory entry does not move when a file inside it grows.
 * A change that goes unseen resumes and re-reads the backup, and unchanged
 * conversation boundaries keep the skip correct; this fires on every
 * difference the stat can actually see.
 */
export function checkSourceFingerprint(
  stored: SourceFingerprint | null,
  stat: PathStat | null,
): FingerprintCheck {
  if (!stored) return "unknown";
  if (!stat?.exists) return "source_missing";
  return stored.size_bytes === stat.sizeBytes && stored.modified_unix_ms === stat.modifiedUnixMs
    ? "match"
    : "mismatch";
}
