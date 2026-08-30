import type { AttachmentForecast, SizeVerdict, StagingSummary } from "../../lib/tauri";

/**
 * The stable identity of a staged attachment across the media pass.
 *
 * A committed derivative changes name: `attachments/2024-01-15-9f2a3b4c.heic`
 * becomes `attachments/2024-01-15-9f2a3b4c-mv.jpg`. The stem gains a literal
 * `-mv` suffix and the extension changes, but the `{date}-{digest}` stem in
 * front of it is stable. Match on that, or a converted file reads as "one
 * file vanished, an unrelated one appeared" instead of the same file under
 * its new name.
 */
function stableStem(path: string): string {
  const base = path.split("/").pop() ?? path;
  const dot = base.lastIndexOf(".");
  const stem = dot > 0 ? base.slice(0, dot) : base;
  return stem.endsWith("-mv") ? stem.slice(0, -3) : stem;
}

/**
 * Ranks a verdict by how much trouble it predicts, worst last. Only used to
 * tell a genuine regression (a live row that got worse) from a row that is
 * merely unresolved — `cannot_process` never legitimately competes with the
 * size verdicts, since a file's media type does not change between passes.
 */
const SEVERITY: Record<SizeVerdict, number> = {
  fits_as_is: 0,
  likely_fits: 1,
  may_grow: 2,
  probably_too_big: 3,
  cannot_process: 4,
};

export interface GateDelta {
  /** Forecast said it would probably fit, and it did. */
  forecastHeld: number;
  forecastHeldFiles: string[];
  /** Written off as too big at Gate 1, came in under the limit after all. */
  betterThanForecast: number;
  betterThanForecastFiles: string[];
  /**
   * Was under the limit (or flagged as merely at risk) when approved, and is
   * not accounted for now — decision 45's "what failed that nobody
   * flagged". This bucket cannot say *why*: the recomputed summary drops a
   * settled attachment's row whether the cause was crossing the size limit
   * or a conversion failure, so the two are indistinguishable from here.
   */
  worseThanForecast: number;
  worseThanForecastFiles: string[];
  /**
   * A traceable loss unrelated to the size story: a file that could never
   * be sized (`cannot_process`) disappeared outright, or a live row came
   * back with a strictly worse verdict than it was approved under.
   */
  failed: number;
  failedFiles: string[];
  /** False only when every bucket above is empty. */
  hasChanges: boolean;
}

/**
 * Compares the `StagingSummary` approved at Gate 1 against the one
 * recomputed after the media pass, per attachment — not per count, because
 * counts alone cannot tell "3 fell out and 3 different ones came in" from
 * "nothing changed".
 *
 * The two summaries are asymmetric in what they can say. `forecasts` never
 * carries a row for `fits_as_is` (`classify_one` in `staging_summary.rs`
 * records the count and stops there), and once the media pass sets an
 * attachment's `missing_reason` — because its derivative crossed the size
 * limit, or the conversion itself failed — `summarize_staging` skips it
 * entirely: no bytes, no verdict, no forecast row, regardless of what is
 * still on disk. So a name that vanishes from the recomputed forecasts is
 * either a success (it is now `fits_as_is`, which is never named) or one of
 * those two settled failures — the data cannot say which. Only a live row
 * that comes back with a strictly worse verdict is a traceable regression.
 */
export function gateDelta(approved: StagingSummary, actual: StagingSummary): GateDelta {
  const actualByStem = new Map<string, AttachmentForecast>();
  for (const row of actual.forecasts) {
    actualByStem.set(stableStem(row.path), row);
  }

  const held: string[] = [];
  const better: string[] = [];
  const worse: string[] = [];
  const failed: string[] = [];

  for (const row of approved.forecasts) {
    const match = actualByStem.get(stableStem(row.path));
    if (match) {
      // Still present with a live verdict. An unchanged or improved (but
      // not yet confirmed fitting) row is still pending, not news; only a
      // verdict that got strictly worse is worth a word here.
      if (SEVERITY[match.verdict] > SEVERITY[row.verdict]) {
        failed.push(row.name);
      }
      continue;
    }
    // No row at all in the recomputed summary — fully resolved, one way or
    // the other, and which way depends on what was approved.
    switch (row.verdict) {
      case "likely_fits":
        held.push(row.name);
        break;
      case "probably_too_big":
        better.push(row.name);
        break;
      case "may_grow":
        // Decision 45: a file that was under the limit and is now over
        // gets its own row. `may_grow` already flagged the risk, but
        // nobody confirmed which way it broke, so a silent disappearance
        // is read as the bad outcome, not the good one.
        worse.push(row.name);
        break;
      case "cannot_process":
        // Never touched by the media step, so its disappearance is not a
        // size story — something else removed it.
        failed.push(row.name);
        break;
      case "fits_as_is":
        // fits_as_is never gets a named row (see classify_one), so this
        // branch is unreachable; kept so the switch stays exhaustive.
        break;
      default:
        row.verdict satisfies never;
    }
  }

  // fits_as_is never gets a named row on either side, so the only signal
  // for "a file we were confident about is now gone" is the raw count: how
  // many fewer files the recomputed summary still calls fits_as_is. This is
  // the other half of decision 45 — nobody flagged it, and here there is
  // not even a name to show, only a count.
  const settledFitsAsIs = Math.max(
    0,
    approved.verdictCounts.fitsAsIs - actual.verdictCounts.fitsAsIs,
  );

  const forecastHeld = held.length;
  const betterThanForecast = better.length;
  const worseThanForecast = worse.length + settledFitsAsIs;
  const failedCount = failed.length;

  return {
    forecastHeld,
    forecastHeldFiles: held,
    betterThanForecast,
    betterThanForecastFiles: better,
    worseThanForecast,
    worseThanForecastFiles: worse,
    failed: failedCount,
    failedFiles: failed,
    hasChanges: forecastHeld + betterThanForecast + worseThanForecast + failedCount > 0,
  };
}
