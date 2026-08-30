import { describe, expect, it } from "vitest";
import type { AttachmentForecast, SizeVerdict, StagingSummary, VerdictCounts } from "../../lib/tauri";
import { gateDelta } from "./gateDelta";

/**
 * Builds a `StagingSummary` from verdict counts, mirroring the real
 * `summarize_staging` output: every non-`fits_as_is` count gets that many
 * named, uniquely-pathed forecast rows, and `fits_as_is` gets none — the
 * real classifier never emits a row for it (see `classify_one` in
 * `staging_summary.rs`). `tooLarge` is not a `StagingSummary` field; it is
 * this helper's way of simulating an attachment the media pass settled away
 * (`missing_reason` set) — present in neither `verdictCounts` nor
 * `forecasts`, exactly like the real thing.
 */
function summary(
  overrides: Partial<Record<keyof VerdictCounts | "tooLarge", number>> = {},
): StagingSummary {
  const verdictCounts: VerdictCounts = {
    fitsAsIs: overrides.fitsAsIs ?? 0,
    likelyFits: overrides.likelyFits ?? 0,
    mayGrow: overrides.mayGrow ?? 0,
    probablyTooBig: overrides.probablyTooBig ?? 0,
    cannotProcess: overrides.cannotProcess ?? 0,
  };

  const forecasts: AttachmentForecast[] = [];
  const addRows = (verdict: SizeVerdict, count: number) => {
    for (let i = 0; i < count; i++) {
      forecasts.push({
        path: `attachments/synthetic-${verdict}-${i}.bin`,
        name: `synthetic-${verdict}-${i}.bin`,
        sizeBytes: 1,
        estimateBytes: 1,
        verdict,
      });
    }
  };
  addRows("likely_fits", verdictCounts.likelyFits);
  addRows("may_grow", verdictCounts.mayGrow);
  addRows("probably_too_big", verdictCounts.probablyTooBig);
  addRows("cannot_process", verdictCounts.cannotProcess);

  const settled = overrides.tooLarge ?? 0;
  const attachments =
    verdictCounts.fitsAsIs +
    verdictCounts.likelyFits +
    verdictCounts.mayGrow +
    verdictCounts.probablyTooBig +
    verdictCounts.cannotProcess +
    settled;

  return {
    conversations: 1,
    messages: 1,
    contactIdentifiers: [],
    attachments,
    attachmentBytes: 0,
    verdictCounts,
    forecasts,
  };
}

describe("gateDelta", () => {
  it("counts the forecasts that came true", () => {
    const delta = gateDelta(summary({ likelyFits: 4 }), summary({ fitsAsIs: 4 }));
    expect(delta.forecastHeld).toBe(4);
  });

  it("counts files written off that came in under after all", () => {
    // Good news, and worth saying: the user approved on the assumption these
    // were lost.
    const delta = gateDelta(summary({ probablyTooBig: 3 }), summary({ fitsAsIs: 3 }));
    expect(delta.betterThanForecast).toBe(3);
  });

  it("counts files that crossed the limit nobody flagged", () => {
    const delta = gateDelta(summary({ fitsAsIs: 10 }), summary({ fitsAsIs: 9, tooLarge: 1 }));
    expect(delta.worseThanForecast).toBe(1);
  });

  it("is empty when the forecast was exactly right", () => {
    const same = summary({ fitsAsIs: 10 });
    expect(gateDelta(same, same).hasChanges).toBe(false);
  });

  it("matches a converted file by its stable stem, not its full name", () => {
    // The media pass renames a committed derivative: the date-digest stem
    // survives, but the suffix and extension do not. Without stripping
    // `-mv` and the extension, this reads as "the approved file vanished
    // AND an unrelated new file showed up" instead of one file, still too
    // big under its new name.
    const approved = summary();
    approved.verdictCounts.likelyFits = 1;
    approved.forecasts.push({
      path: "attachments/2024-01-15-9f2a3b4c.heic",
      name: "IMG_1234.HEIC",
      sizeBytes: 40_000_000,
      estimateBytes: 45_000_000,
      verdict: "likely_fits",
    });

    const actual = summary();
    actual.verdictCounts.probablyTooBig = 1;
    actual.forecasts.push({
      path: "attachments/2024-01-15-9f2a3b4c-mv.jpg",
      name: "IMG_1234.HEIC",
      sizeBytes: 60_000_000,
      estimateBytes: 60_000_000,
      verdict: "probably_too_big",
    });

    const delta = gateDelta(approved, actual);
    expect(delta.failed).toBe(1);
    expect(delta.failedFiles).toEqual(["IMG_1234.HEIC"]);
    // Must not also read as a plain vanished likely_fits row, which would
    // count as a held forecast instead of a regression.
    expect(delta.forecastHeld).toBe(0);
  });

  it("counts an unprocessable file that disappeared as failed, not as a size story", () => {
    const delta = gateDelta(summary({ cannotProcess: 2 }), summary());
    expect(delta.failed).toBe(2);
    expect(delta.failedFiles).toHaveLength(2);
    expect(delta.worseThanForecast).toBe(0);
  });

  it("leaves an unresolved but improved row uncounted rather than guessing", () => {
    // Still present, still not confirmed fitting — not pending confusion,
    // not news yet either.
    const approved = summary();
    approved.verdictCounts.mayGrow = 1;
    approved.forecasts.push({
      path: "attachments/2024-02-01-aabbcc.mov",
      name: "clip.mov",
      sizeBytes: 30_000_000,
      estimateBytes: 32_000_000,
      verdict: "may_grow",
    });

    const actual = summary();
    actual.verdictCounts.likelyFits = 1;
    actual.forecasts.push({
      path: "attachments/2024-02-01-aabbcc-mv.mp4",
      name: "clip.mov",
      sizeBytes: 28_000_000,
      estimateBytes: 29_000_000,
      verdict: "likely_fits",
    });

    const delta = gateDelta(approved, actual);
    expect(delta.hasChanges).toBe(false);
  });

  it("counts a live regression toward failed even when hasChanges gates on the total", () => {
    const approved = summary();
    approved.verdictCounts.cannotProcess = 1;
    approved.forecasts.push({
      path: "attachments/2024-03-01-ffeedd.zip",
      name: "archive.zip",
      sizeBytes: 500,
      estimateBytes: 500,
      verdict: "cannot_process",
    });
    const actual = summary();

    const delta = gateDelta(approved, actual);
    expect(delta.hasChanges).toBe(true);
    expect(delta.failed).toBe(1);
  });
});
