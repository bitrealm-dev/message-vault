import { describe, expect, it } from "vitest";
import type {
  AttachmentForecast,
  SizeVerdict,
  StagingSummary,
  TranscodeFinishedReport,
  VerdictCounts,
} from "../../lib/tauri";
import { gateDelta } from "./gateDelta";

/**
 * Builds a `StagingSummary` from verdict counts, mirroring the real
 * `summarize_staging` output: every non-`fits_as_is` count gets that many
 * named, uniquely-pathed forecast rows, and `fits_as_is` gets none — the
 * real classifier never emits a row for it (see `classify_one` in
 * `staging_summary.rs`).
 */
function summary(overrides: Partial<VerdictCounts> = {}): StagingSummary {
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

  const attachments =
    verdictCounts.fitsAsIs +
    verdictCounts.likelyFits +
    verdictCounts.mayGrow +
    verdictCounts.probablyTooBig +
    verdictCounts.cannotProcess;

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

/** Fills in the `TranscodeFinishedReport` fields a test doesn't care about. */
function report(overrides: Partial<TranscodeFinishedReport> = {}): TranscodeFinishedReport {
  return {
    converted: 0,
    skipped: 0,
    too_large: 0,
    failed: 0,
    missing: 0,
    repointed: 0,
    bytes_before: 0,
    bytes_after: 0,
    ...overrides,
  };
}

describe("gateDelta", () => {
  it("reads the loss count from the pass instead of inferring it — the fixture the old subtraction design got wrong", () => {
    // Approved 10 fitsAsIs + 5 likelyFits (named). All 5 convert; 3 of the
    // original 10 fitsAsIs are separately lost. The recomputed summary shows
    // fitsAsIs:12 either way — conversion inflow (+2) and settled outflow
    // (-3) on the *other* bucket net out to +2 on top of the surviving 10.
    // A design that subtracts approved.fitsAsIs from actual.fitsAsIs reads
    // this as zero losses, because the inflow hides the outflow.
    const approved = summary({ fitsAsIs: 10, likelyFits: 5 });
    const actual = summary({ fitsAsIs: 12 });
    const transcode = report({ converted: 5, too_large: 2, failed: 1, missing: 0 });

    const delta = gateDelta(approved, actual, transcode);

    expect(delta.lostCount).toBe(3);
    expect(delta.cameOutFine).toBe(2);
  });

  it("does not report a clean conversion as a loss", () => {
    const approved = summary({ mayGrow: 3 });
    const actual = summary({ fitsAsIs: 3 });
    const transcode = report({ converted: 3, too_large: 0, failed: 0 });

    const delta = gateDelta(approved, actual, transcode);

    expect(delta.lostCount).toBe(0);
    expect(delta.cameOutFine).toBe(3);
    expect(delta.stillFlagged).toEqual([]);
  });

  describe("without a transcode report (resumed session)", () => {
    it("recovers the same loss count via conservation math — the loss direction", () => {
      const approved = summary({ fitsAsIs: 10, likelyFits: 5 });
      const actual = summary({ fitsAsIs: 12 });

      const delta = gateDelta(approved, actual);

      expect(delta.lostCount).toBe(3);
      expect(delta.cameOutFine).toBe(2);
    });

    it("recovers the same zero loss count via conservation math — the success direction", () => {
      const approved = summary({ mayGrow: 3 });
      const actual = summary({ fitsAsIs: 3 });

      const delta = gateDelta(approved, actual);

      expect(delta.lostCount).toBe(0);
      expect(delta.cameOutFine).toBe(3);
    });
  });

  it("is empty when the forecast was exactly right", () => {
    const same = summary({ fitsAsIs: 10 });
    expect(gateDelta(same, same).hasChanges).toBe(false);
  });

  it("matches a converted file by its stable stem, not its full name", () => {
    // The media pass renames a committed derivative: the date-digest16 stem
    // survives (`attachment_dest_name`,
    // `crates/core/message-vault-io-core/src/attachments.rs`), but the
    // suffix and extension do not. The digest is hex (0-9a-f) and the date
    // prefix is digits and underscores (`%Y%m%d_%H%M%S`), so neither `m` nor
    // `v` can appear in an original stem — no staged file can already be
    // wearing the `-mv` suffix `COMMITTED_SUFFIX` adds (`transcode.rs`).
    // Without stripping it and the extension, this reads as "the approved
    // file vanished AND an unrelated new file showed up" instead of one
    // file, still too big under its new name.
    const approved = summary({ likelyFits: 1 });
    approved.forecasts[0] = {
      path: "attachments/2024-01-15-9f2a3b4c.heic",
      name: "IMG_1234.HEIC",
      sizeBytes: 40_000_000,
      estimateBytes: 45_000_000,
      verdict: "likely_fits",
    };

    const actual = summary({ probablyTooBig: 1 });
    actual.forecasts[0] = {
      path: "attachments/2024-01-15-9f2a3b4c-mv.jpg",
      name: "IMG_1234.HEIC",
      sizeBytes: 60_000_000,
      estimateBytes: 60_000_000,
      verdict: "probably_too_big",
    };

    const delta = gateDelta(approved, actual);

    expect(delta.stillFlagged).toEqual([
      { name: "IMG_1234.HEIC", verdict: "probably_too_big", regressed: true },
    ]);
    // Not read as a plain vanished likely_fits row (which would count
    // toward cameOutFine instead of surfacing as a live regression).
    expect(delta.cameOutFine).toBe(0);
  });

  it("gives a regressed row decision 45's framing, not a plain still-flagged one", () => {
    const approved = summary({ mayGrow: 1 });
    approved.forecasts[0] = {
      path: "attachments/2024-02-01-aabbcc.mov",
      name: "clip.mov",
      sizeBytes: 30_000_000,
      estimateBytes: 32_000_000,
      verdict: "may_grow",
    };
    const actual = summary({ probablyTooBig: 1 });
    actual.forecasts[0] = {
      path: "attachments/2024-02-01-aabbcc-mv.mp4",
      name: "clip.mov",
      sizeBytes: 55_000_000,
      estimateBytes: 55_000_000,
      verdict: "probably_too_big",
    };

    const delta = gateDelta(approved, actual);
    expect(delta.stillFlagged[0]?.regressed).toBe(true);
    expect(delta.hasChanges).toBe(true);
  });

  it("leaves a row that is still flagged the same way as before out of hasChanges", () => {
    const approved = summary({ cannotProcess: 1 });
    approved.forecasts[0] = {
      path: "attachments/2024-03-01-ffeedd.zip",
      name: "archive.zip",
      sizeBytes: 500,
      estimateBytes: 500,
      verdict: "cannot_process",
    };
    const actual = summary({ cannotProcess: 1 });
    actual.forecasts[0] = {
      path: "attachments/2024-03-01-ffeedd.zip",
      name: "archive.zip",
      sizeBytes: 500,
      estimateBytes: 500,
      verdict: "cannot_process",
    };

    const delta = gateDelta(approved, actual);
    expect(delta.stillFlagged[0]?.regressed).toBe(false);
    expect(delta.hasChanges).toBe(false);
  });

  it("leaves an unresolved but improved row out of hasChanges rather than guessing", () => {
    // Still flagged, still not confirmed fitting — not a regression, and
    // not news yet either.
    const approved = summary({ mayGrow: 1 });
    approved.forecasts[0] = {
      path: "attachments/2024-02-01-aabbcc.mov",
      name: "clip.mov",
      sizeBytes: 30_000_000,
      estimateBytes: 32_000_000,
      verdict: "may_grow",
    };
    const actual = summary({ likelyFits: 1 });
    actual.forecasts[0] = {
      path: "attachments/2024-02-01-aabbcc-mv.mp4",
      name: "clip.mov",
      sizeBytes: 28_000_000,
      estimateBytes: 29_000_000,
      verdict: "likely_fits",
    };

    const delta = gateDelta(approved, actual);
    expect(delta.stillFlagged[0]?.regressed).toBe(false);
    expect(delta.hasChanges).toBe(false);
  });

  it("does not throw with no approved baseline — a resumed session whose stored plan failed to parse", () => {
    const actual = summary({ likelyFits: 1 });
    actual.forecasts[0] = {
      path: "attachments/2024-05-01-ab12cd.mov",
      name: "clip.mov",
      sizeBytes: 10_000_000,
      estimateBytes: 9_000_000,
      verdict: "likely_fits",
    };

    const delta = gateDelta(undefined, actual);
    expect(delta.lostCount).toBe(0);
    // No baseline to diff against -- an unknown history reads as the
    // mildest severity, so anything actually flagged now is new
    // information rather than a silently assumed non-issue.
    expect(delta.stillFlagged[0]?.regressed).toBe(true);
  });
});
