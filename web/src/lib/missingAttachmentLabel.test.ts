import { describe, expect, it } from "vitest";
import { missingAttachmentChipLabel } from "./missingAttachmentLabel.ts";
import type { MessageAttachment } from "./types.ts";

function att(partial: Partial<MessageAttachment>): MessageAttachment {
  return {
    path: null,
    original_name: null,
    mime_type: null,
    sha256: null,
    is_sticker: false,
    transcription: null,
    missing_reason: null,
    ...partial,
  };
}

describe("missingAttachmentChipLabel", () => {
  it("formats too_large with name and mime", () => {
    expect(
      missingAttachmentChipLabel(
        att({
          original_name: "video.mp4",
          mime_type: "video/mp4",
          missing_reason: "too_large",
        }),
      ),
    ).toBe("video.mp4 · video/mp4 (missing — too large)");
  });

  it("formats file_missing from path basename when name missing", () => {
    expect(
      missingAttachmentChipLabel(
        att({
          path: "attachments/gone.bin",
          missing_reason: "file_missing",
        }),
      ),
    ).toBe("gone.bin (missing — file not found)");
  });

  it("labels a deliberately skipped attachment as skipped, not missing", () => {
    expect(
      missingAttachmentChipLabel(
        att({
          original_name: "IMG_0421.HEIC",
          mime_type: "image/heic",
          missing_reason: "skipped",
        }),
      ),
    ).toBe("IMG_0421.HEIC · image/heic (skipped)");
  });

  it("labels the iMessage embed_disabled reason as skipped too", () => {
    expect(
      missingAttachmentChipLabel(
        att({
          original_name: "clip.mov",
          missing_reason: "embed_disabled",
        }),
      ),
    ).toBe("clip.mov (skipped)");
  });
});
