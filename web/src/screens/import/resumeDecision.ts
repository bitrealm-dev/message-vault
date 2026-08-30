import type { ActiveImportSession } from "../../lib/importSession";

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
}): ResumeDecision {
  const { session, deviceId, folderExists } = args;
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
  return { kind: "restart", canResume: false, session };
}
