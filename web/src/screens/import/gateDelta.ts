import type {
  AttachmentForecast,
  SizeVerdict,
  StagingSummary,
  TranscodeFinishedReport,
} from "../../lib/tauri";

/**
 * The stable identity of a staged attachment across the media pass.
 *
 * A committed derivative changes name: `attachments/2024-01-15-9f2a3b4c.heic`
 * becomes `attachments/2024-01-15-9f2a3b4c-mv.jpg`. The stem gains a literal
 * `-mv` suffix and the extension changes, but the `{date}-{digest16}` stem in
 * front of it is stable (`attachment_dest_name`,
 * `crates/core/message-vault-io-core/src/attachments.rs`). Match on that, or
 * a converted file reads as "one file vanished, an unrelated one appeared"
 * instead of the same file under its new name.
 */
export function stableStem(path: string): string {
  const base = path.split("/").pop() ?? path;
  const dot = base.lastIndexOf(".");
  const stem = dot > 0 ? base.slice(0, dot) : base;
  return stem.endsWith("-mv") ? stem.slice(0, -3) : stem;
}

/**
 * Ranks a verdict by how much trouble it predicts, worst last. Used only to
 * tell a genuine regression (a live row that got worse) from one that is
 * merely unresolved.
 */
const SEVERITY: Record<SizeVerdict, number> = {
  fits_as_is: 0,
  likely_fits: 1,
  may_grow: 2,
  probably_too_big: 3,
  cannot_process: 4,
};

/** One attachment still flagged in the recomputed summary. */
export interface StillFlaggedItem {
  name: string;
  verdict: SizeVerdict;
  /**
   * True when this file was flagged less severely — or not flagged at all,
   * i.e. approved as `fits_as_is` — at the last check. Decision 45's "was
   * under the limit, now over" framing belongs on these rows specifically:
   * the file *was* processed, it is just still over the limit. A row that
   * was already this severe (or worse) at the last check is not news.
   */
  regressed: boolean;
}

export interface GateDelta {
  /**
   * Files the media pass will definitively not upload: too large after
   * conversion, failed to convert outright, or gone missing. A pure count —
   * none of those sources name individual files, so there is nothing to
   * list.
   *
   * When `transcode` is supplied, this is a **pass-wide total** —
   * `too_large + failed + missing` across every attachment the pass
   * touched, not bucketed by which approved verdict a lost file came from.
   * Today's `convert`/`compress` pipeline only ever touches attachments
   * that were already flagged (`likely_fits`/`may_grow`/`probably_too_big`),
   * so every loss the report counts corresponds to a vanished named row and
   * `cameOutFine`'s subtraction below is exact. If a future media mode ever
   * processes `fits_as_is`-approved attachments too, a loss from that
   * bucket would inflate this total with no corresponding named row to
   * subtract, and `cameOutFine` would undercount genuine successes by the
   * same amount.
   */
  lostCount: number;
  /**
   * Every attachment the recomputed summary still flags, named and exact —
   * the same set Gate 1's own `forecastGroups` would render.
   */
  stillFlagged: StillFlaggedItem[];
  /**
   * Approved rows that vanished from the recomputed summary and are not
   * explained by `lostCount` — good news worth surfacing. Merged across
   * every approved verdict on purpose: once a row vanishes, the data cannot
   * say which specific file among several converted successfully and which
   * were lost, only how many of each, so the screen does not pretend to
   * attribute individual files either. See `lostCount`'s doc comment for
   * the one condition under which this undercounts.
   */
  cameOutFine: number;
  /** False only when nothing above is worth a word. */
  hasChanges: boolean;
}

/**
 * Compares the `StagingSummary` approved at the last check against the one
 * recomputed after the media pass, plus — when available — the pass's own
 * `TranscodeFinishedReport`.
 *
 * The two summaries alone cannot resolve where a vanished file went. A
 * successfully converted file lands at `fits_as_is`, which never gets a
 * named forecast row (`classify_one` in `staging_summary.rs` records the
 * count and stops there); a file the pass lost — its derivative crossed the
 * size limit, or the conversion itself failed — gets `missing_reason` set,
 * and `summarize_staging` then skips it entirely: no bytes, no verdict, no
 * forecast row, regardless of what is still on disk. Both cases vanish
 * identically. Counting a bucket's vanished rows as "held" or "better" the
 * way an earlier version of this function did is wrong whenever conversion
 * inflow and settled outflow happen in the same run — inflow can mask a
 * real loss, and a clean run can misread as a loss.
 *
 * The fix is to stop inferring per-file outcomes from the recomputed
 * summary alone and instead read the loss count straight from the pass:
 * `transcode`'s `too_large + failed + missing` is exact, taken from the run
 * itself rather than guessed at. `cameOutFine` is then just "how many named
 * rows vanished, minus how many the pass says it lost" — still a count, not
 * an attribution, because the data still cannot say *which* vanished file
 * is which.
 *
 * `transcode` is absent on a resumed session (the pass already ran in an
 * earlier one). Without it, `lostCount` falls back to conservation math:
 * the total attachment count is conserved across the pass even though
 * individual attribution is not, so `approved.fitsAsIs + vanishedNamedRows
 * - actual.fitsAsIs` recovers the same total the report would have given,
 * as long as nothing outside that arithmetic moved (see the module's tests
 * for the two directions this covers and does not).
 *
 * `approved` itself is absent when a session resumes at Gate 2 (or through
 * a re-run of the media pass) with a stored plan that failed to parse — a
 * crash mid-write, or data from before this field existed. There is
 * nothing to diff against in that case, so every named row in `actual` is
 * treated as new information (an unknown baseline is the mildest severity,
 * `fits_as_is`, so anything actually flagged now reads as a regression)
 * and the conservation math above simply sees zero approved counts and
 * zero vanished rows — never blocks the resume, just under-informs it.
 */
export function gateDelta(
  approved: StagingSummary | undefined,
  actual: StagingSummary,
  transcode?: TranscodeFinishedReport,
): GateDelta {
  const approvedForecasts = approved?.forecasts ?? [];
  const approvedByStem = new Map<string, AttachmentForecast>();
  for (const row of approvedForecasts) {
    approvedByStem.set(stableStem(row.path), row);
  }
  const actualByStem = new Map<string, AttachmentForecast>();
  for (const row of actual.forecasts) {
    actualByStem.set(stableStem(row.path), row);
  }

  let vanishedNamedRowCount = 0;
  for (const row of approvedForecasts) {
    if (!actualByStem.has(stableStem(row.path))) {
      vanishedNamedRowCount += 1;
    }
  }

  const lostCount = transcode
    ? transcode.too_large + transcode.failed + transcode.missing
    : Math.max(
        0,
        (approved?.verdictCounts.fitsAsIs ?? 0) +
          vanishedNamedRowCount -
          actual.verdictCounts.fitsAsIs,
      );

  const cameOutFine = Math.max(0, vanishedNamedRowCount - lostCount);

  const stillFlagged: StillFlaggedItem[] = actual.forecasts.map((row) => {
    const approvedMatch = approvedByStem.get(stableStem(row.path));
    // No approved row for this stem means it was `fits_as_is` at the last
    // check (every attachment already existed at approval time — nothing
    // appears out of nowhere between the two summaries), the mildest
    // severity there is.
    const approvedSeverity = approvedMatch ? SEVERITY[approvedMatch.verdict] : SEVERITY.fits_as_is;
    return {
      name: row.name,
      verdict: row.verdict,
      regressed: SEVERITY[row.verdict] > approvedSeverity,
    };
  });

  const hasChanges =
    lostCount > 0 || cameOutFine > 0 || stillFlagged.some((item) => item.regressed);

  return {
    lostCount,
    stillFlagged,
    cameOutFine,
    hasChanges,
  };
}
