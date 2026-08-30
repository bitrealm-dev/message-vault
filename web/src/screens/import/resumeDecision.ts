import type { ActiveImportSession } from "../../lib/importSession";

/** What entering Import should do about a session that already exists. */
export type ResumeDecision = {
  kind: "none" | "other_device" | "folder_missing" | "resume_push" | "restart";
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
  return { kind: "restart", canResume: false, session };
}
