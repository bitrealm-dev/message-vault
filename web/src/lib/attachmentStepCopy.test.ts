import { describe, expect, it } from "vitest";
import { attachmentStepCopy } from "./attachmentStepCopy";

describe("attachmentStepCopy", () => {
  it("labels copy mode", () => {
    expect(attachmentStepCopy("copy")).toEqual({
      label: "Copy attachments",
      doneDetail: "Copied attachments",
    });
  });

  it("labels skip mode", () => {
    expect(attachmentStepCopy("skip")).toEqual({
      label: "Skip attachments",
      doneDetail: "Message attachments skipped",
    });
  });

  it("labels convert and compress the same", () => {
    expect(attachmentStepCopy("convert")).toEqual({
      label: "Convert attachments",
      doneDetail: "Attachments processed",
    });
    expect(attachmentStepCopy("compress")).toEqual(attachmentStepCopy("convert"));
  });
});
