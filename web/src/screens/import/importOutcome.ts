import type { PushFinishedReport } from "../../lib/tauri";

export type ImportOutcome = "completed" | "completed_with_issues" | "failed";

/**
 * Three-way verdict for a finished import, read from the push report rather
 * than from whether the push call returned (spec decisions 21–22).
 *
 * `failed` has a zero floor: interrupted, threw, or nothing landed at all.
 * A re-push where every conversation dedupes to a skip is a no-op, not a
 * failure. Item-level problems make it `completed_with_issues`.
 */
export function importOutcome(args: {
  report: PushFinishedReport | undefined;
  threw: boolean;
  issues: readonly { kind: string }[];
}): ImportOutcome {
  const { report, threw, issues } = args;
  if (threw || !report) return "failed";
  const nothingLanded =
    report.conversations_total > 0 &&
    report.conversations_ok === 0 &&
    report.conversations_skipped === 0;
  if (nothingLanded) return "failed";
  if (report.conversations_failed > 0 || report.messages_failed > 0 || issues.length > 0) {
    return "completed_with_issues";
  }
  return "completed";
}
