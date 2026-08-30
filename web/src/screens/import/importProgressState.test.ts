import { describe, expect, it } from "vitest";
import {
  attachmentDoneDetail,
  isProgressStepComplete,
  progressHeading,
  stepIndexFor,
  stepsFor,
} from "./importProgressState";

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

describe("stepsFor", () => {
  it("shows a media step under convert and compress", () => {
    expect(stepsFor("convert").map((s) => s.label)).toEqual([
      "Read backup",
      "Copy to staging",
      "Convert media",
      "Upload to vault",
    ]);
    expect(stepsFor("compress")[2].label).toBe("Compress media");
  });

  it("has no media step under copy or skip", () => {
    // There is no media step in these modes, so a greyed-out row would be
    // promising work that will never run.
    expect(stepsFor("copy").map((s) => s.label)).toEqual([
      "Read backup",
      "Copy to staging",
      "Upload to vault",
    ]);
    expect(stepsFor("skip").map((s) => s.label)).toEqual([
      "Read backup",
      "Copy to staging",
      "Upload to vault",
    ]);
  });

  it("never says transcode", () => {
    for (const mode of ["copy", "convert", "compress", "skip"] as const) {
      for (const step of stepsFor(mode)) {
        expect(step.label.toLowerCase()).not.toContain("transcode");
      }
    }
  });
});

describe("stepIndexFor", () => {
  it("puts writing conversation files on the staging step", () => {
    // "prepare" is the pipeline's name for writing conversation files. From
    // the user's side that is part of staging, not a step of its own.
    expect(stepIndexFor("attachments", "convert")).toBe(1);
    expect(stepIndexFor("prepare", "convert")).toBe(1);
  });

  it("maps the media step to its own row, and upload after it", () => {
    expect(stepIndexFor("media", "convert")).toBe(2);
    expect(stepIndexFor("upload", "convert")).toBe(3);
  });

  it("shifts upload down when there is no media step", () => {
    expect(stepIndexFor("upload", "copy")).toBe(2);
  });

  it("never lands an unmapped step on upload by accident", () => {
    // The old mapping ended in `return 3`, so a step nobody had wired drew
    // its progress on the upload bar.
    expect(stepIndexFor("media", "copy")).toBe(-1);
  });
});

describe("progressHeading", () => {
  it("names the stage the import is actually on", () => {
    const steps = stepsFor("convert");
    steps[0].status = "done";
    steps[1].status = "active";
    expect(progressHeading(steps, "progress")).toBe("Copying to staging");
    steps[1].status = "done";
    steps[2].status = "active";
    expect(progressHeading(steps, "progress")).toBe("Converting media");
  });

  it("says compressing when that is the job", () => {
    const steps = stepsFor("compress");
    steps[2].status = "active";
    expect(progressHeading(steps, "progress")).toBe("Compressing media");
  });

  it("never says transcode", () => {
    // Decision 18: it is a stage name, and the user never sees it.
    const steps = stepsFor("convert");
    for (let i = 0; i < steps.length; i += 1) {
      const marked = steps.map(
        (s, j) => ({ ...s, status: j === i ? "active" : "pending" }) as const,
      );
      expect(progressHeading(marked, "progress").toLowerCase()).not.toContain("transcode");
    }
  });

  it("titles the finished screen by its outcome, not by a step", () => {
    expect(progressHeading(stepsFor("convert"), "done")).toBe("Import finished");
  });

  it("falls back to the first step rather than an empty heading", () => {
    // Nothing active yet, one render frame before the first event arrives.
    expect(progressHeading(stepsFor("convert"), "progress")).toBe("Reading your backup");
  });
});
