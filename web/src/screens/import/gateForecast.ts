import type { SizeVerdict, VerdictCounts } from "../../lib/tauri";
import type { AttachmentMediaMode } from "../../lib/types";

/** The job the media step is doing, in the user's words (decisions 18, 19). */
export function mediaJobVerb(mode: AttachmentMediaMode): "converting" | "compressing" | null {
  if (mode === "convert") return "converting";
  if (mode === "compress") return "compressing";
  return null;
}

function capitalize(word: string): string {
  return word.charAt(0).toUpperCase() + word.slice(1);
}

/** "1 file" or "12 files" — every caller here folds the count into a
 * sentence, so this spells out the plural rather than leaving each call
 * site to remember the `=== 1` check. */
export function pluralFiles(count: number): string {
  return `${count.toLocaleString()} file${count === 1 ? "" : "s"}`;
}

export interface VerdictCopy {
  label: string;
  hint: string;
}

/**
 * Wording for one verdict, in the mode it was forecast under. Labels that
 * name the job route through `mediaJobVerb` so `convert` and `compress`
 * copy can never drift apart from each other.
 */
export function verdictCopy(verdict: SizeVerdict, mode: AttachmentMediaMode): VerdictCopy {
  const verb = mediaJobVerb(mode);
  switch (verdict) {
    case "fits_as_is":
      return {
        label: "Fits as-is",
        hint: "Already under the size limit, so the media step leaves it alone.",
      };
    case "likely_fits":
      return {
        label: verb ? `Likely to fit after ${verb}` : "Likely to fit",
        hint: "Close to the limit today; the estimate expects it to land under it.",
      };
    case "may_grow":
      return {
        label: "May grow past the limit",
        hint: verb
          ? `${capitalize(verb)} can add size, so a file under the limit today can land over it.`
          : "A file under the limit today can land over it after the media step.",
      };
    case "probably_too_big":
      return {
        label: "Probably still too big",
        hint: "Expected to stay over the limit even after the media step.",
      };
    case "cannot_process": {
      const doneForm = mode === "convert" ? "converted" : mode === "compress" ? "compressed" : null;
      return {
        label: doneForm ? `Cannot be ${doneForm} — not audio or video` : "Not audio or video",
        hint: "This file type is not audio or video, so the media step does not touch it.",
      };
    }
    default:
      return verdict satisfies never;
  }
}

export interface ForecastGroup extends VerdictCopy {
  verdict: SizeVerdict;
  count: number;
}

/** What needs attention first, what is fine last (decision 11). */
const GROUP_ORDER: SizeVerdict[] = [
  "probably_too_big",
  "may_grow",
  "cannot_process",
  "likely_fits",
  "fits_as_is",
];

const COUNT_KEY: Record<SizeVerdict, keyof VerdictCounts> = {
  fits_as_is: "fitsAsIs",
  likely_fits: "likelyFits",
  may_grow: "mayGrow",
  probably_too_big: "probablyTooBig",
  cannot_process: "cannotProcess",
};

/**
 * The verdict groups worth showing, in priority order, with the empty ones
 * dropped — a row reading "0 files may grow past the limit" is noise on a
 * screen whose job is to be read quickly.
 */
export function forecastGroups(counts: VerdictCounts, mode: AttachmentMediaMode): ForecastGroup[] {
  return GROUP_ORDER.map((verdict) => ({ verdict, count: counts[COUNT_KEY[verdict]] }))
    .filter((group) => group.count > 0)
    .map((group) => ({ ...group, ...verdictCopy(group.verdict, mode) }));
}
