import assert from "node:assert/strict";
import { describe, it } from "node:test";
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
    assert.equal(
      missingAttachmentChipLabel(
        att({
          original_name: "video.mp4",
          mime_type: "video/mp4",
          missing_reason: "too_large",
        }),
      ),
      "video.mp4 · video/mp4 (missing — too large)",
    );
  });

  it("formats file_missing from path basename when name missing", () => {
    assert.equal(
      missingAttachmentChipLabel(
        att({
          path: "attachments/gone.bin",
          missing_reason: "file_missing",
        }),
      ),
      "gone.bin (missing — file not found)",
    );
  });
});
