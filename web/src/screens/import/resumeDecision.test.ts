import { describe, expect, it } from "vitest";
import type { ActiveImportSession } from "../../lib/importSession";
import { resumeDecisionFor } from "./resumeDecision";

function session(overrides: Partial<ActiveImportSession> = {}): ActiveImportSession {
  return {
    id: 7,
    source: "imessage",
    mode: "append",
    status: "running",
    started_at: "2026-08-30T00:00:00Z",
    stage: "pushing",
    staging_dir: "/home/u/message-vault/staging-260830",
    device_id: "this-device",
    form: { source: "imessage-ios" },
    source_fingerprint: null,
    ...overrides,
  };
}

describe("resumeDecisionFor", () => {
  it("has nothing to decide without a session", () => {
    expect(
      resumeDecisionFor({ session: null, deviceId: "this-device", folderExists: false }).kind,
    ).toBe("none");
  });

  it("says where a session belongs when another install owns it", () => {
    const decision = resumeDecisionFor({
      session: session({ device_id: "other-device" }),
      deviceId: "this-device",
      folderExists: true,
    });
    expect(decision.kind).toBe("other_device");
    expect(decision.canResume).toBe(false);
  });

  it("offers discard alone when the staging folder is gone", () => {
    const decision = resumeDecisionFor({
      session: session(),
      deviceId: "this-device",
      folderExists: false,
    });
    expect(decision.kind).toBe("folder_missing");
    expect(decision.canResume).toBe(false);
  });

  it("resumes the upload when a push was interrupted", () => {
    const decision = resumeDecisionFor({
      session: session({ stage: "pushing" }),
      deviceId: "this-device",
      folderExists: true,
    });
    expect(decision.kind).toBe("resume_push");
    expect(decision.canResume).toBe(true);
  });

  it("restarts when the run died before the folder was finished", () => {
    for (const stage of ["parse", "write"] as const) {
      expect(
        resumeDecisionFor({
          session: session({ stage }),
          deviceId: "this-device",
          folderExists: true,
        }).kind,
      ).toBe("restart");
    }
  });

  it("sends a session waiting at a gate back to its gate", () => {
    for (const stage of ["awaiting_gate_1", "awaiting_gate_2"] as const) {
      const decision = resumeDecisionFor({
        session: session({ stage }),
        deviceId: "this-device",
        folderExists: true,
      });
      expect(decision.kind).toBe("resume_gate");
      expect(decision.canResume).toBe(true);
    }
  });

  it("sends a session that died converting back to the media pass", () => {
    const decision = resumeDecisionFor({
      session: session({ stage: "transcode" }),
      deviceId: "this-device",
      folderExists: true,
    });
    expect(decision.kind).toBe("resume_media");
    expect(decision.canResume).toBe(true);
  });

  it("still offers discard only when the folder is gone at a gate", () => {
    // Decision 36: after approval, discard only. There is nothing to
    // recompute a summary from.
    for (const stage of ["awaiting_gate_1", "awaiting_gate_2", "transcode"] as const) {
      expect(
        resumeDecisionFor({
          session: session({ stage }),
          deviceId: "this-device",
          folderExists: false,
        }).kind,
      ).toBe("folder_missing");
    }
  });

  it("treats a missing device id as this install rather than locking the user out", () => {
    expect(
      resumeDecisionFor({
        session: session({ device_id: null }),
        deviceId: "this-device",
        folderExists: true,
      }).kind,
    ).toBe("resume_push");
  });
});
