import { describe, expect, it } from "vitest";
import { attachmentDoneDetail, isProgressStepComplete } from "./importProgressState";

describe("isProgressStepComplete", () => {
  it("does not complete attachments when the last clone is reported", () => {
    expect(isProgressStepComplete("attachments", 5, 5)).toBe(false);
    expect(isProgressStepComplete("attachments", 0, 0)).toBe(false);
  });

  it("completes parse, prepare, and upload when done reaches total", () => {
    expect(isProgressStepComplete("parse", 10, 10)).toBe(true);
    expect(isProgressStepComplete("prepare", 1, 1)).toBe(true);
    expect(isProgressStepComplete("upload", 2, 2)).toBe(true);
    expect(isProgressStepComplete("parse", 3, 10)).toBe(false);
  });
});

describe("attachmentDoneDetail", () => {
  it("uses zero counts when no attachments event arrived", () => {
    expect(attachmentDoneDetail("skip", null, "Message attachments skipped")).toBe(
      "Skipped 0/0 attachments (0 B / 0 B)",
    );
    expect(attachmentDoneDetail("copy", null, "Copied attachments")).toBe(
      "Copied 0/0 attachments (0 B / 0 B)",
    );
  });

  it("formats the last live counts when present", () => {
    expect(
      attachmentDoneDetail(
        "copy",
        { done: 2, total: 4, bytesDone: 10, bytesTotal: 20 },
        "Copied attachments",
      ),
    ).toBe("Copied 2/4 attachments (10 B / 20 B)");
  });
});
