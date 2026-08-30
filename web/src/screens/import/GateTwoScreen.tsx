import Button from "../../components/Button";
import { formatBytes } from "../../lib/attachmentProgressCopy";
import type { SizeVerdict, StagingSummary } from "../../lib/tauri";
import type { AttachmentMediaMode } from "../../lib/types";
import type { GateDelta } from "./gateDelta";
import { mediaJobVerb, pluralFiles, verdictCopy } from "./gateForecast";

interface DeltaRow {
  key: string;
  text: string;
}

/**
 * The delta rows worth showing, worst first — mirrors Gate 1's own
 * "what needs attention first" ordering. A bucket with nothing in it is
 * dropped; a row reading "0 files" is noise. Does not include still-pending
 * rows — those render unconditionally alongside this, not as part of it
 * (see the component body).
 */
function deltaRows(delta: GateDelta): DeltaRow[] {
  const regressed = delta.stillFlagged.filter((item) => item.regressed);
  return [
    {
      key: "lost",
      text:
        // Decision 45: this count cannot say why — crossing the size limit
        // and a conversion failure land here the same way — so the copy
        // states the effect, not a cause the data cannot support.
        delta.lostCount > 0 ? `${pluralFiles(delta.lostCount)} will not be uploaded.` : "",
    },
    {
      key: "regressed",
      text:
        regressed.length > 0
          ? // Decision 45's other half: these were under the limit (or not
            // flagged at all) at the last check, and are now over. They
            // were processed — this is not "could not be processed".
            `${pluralFiles(regressed.length)} that were fine at the last check are now over the limit.`
          : "",
    },
    {
      key: "cameOutFine",
      text:
        delta.cameOutFine > 0
          ? `${pluralFiles(delta.cameOutFine)} written off as too big came in under the limit after all.`
          : "",
    },
  ].filter((row) => row.text.length > 0);
}

/**
 * Groups the not-yet-resolved rows (still flagged, not regressed — e.g. a
 * `cannot_process` file every mode leaves alone) by verdict, using Gate 1's
 * own wording. Rendered unconditionally, never folded into `hasChanges`: an
 * import holding nothing but an unconvertible file has no "delta" to
 * report, but "will not upload" is still true and must not be hidden behind
 * "everything came out as expected".
 */
function stillPendingRows(delta: GateDelta, mode: AttachmentMediaMode): DeltaRow[] {
  const counts = new Map<SizeVerdict, number>();
  for (const item of delta.stillFlagged) {
    if (item.regressed) continue;
    counts.set(item.verdict, (counts.get(item.verdict) ?? 0) + 1);
  }
  return [...counts.entries()].map(([verdict, count]) => {
    const copy = verdictCopy(verdict, mode);
    return {
      key: `pending-${verdict}`,
      text: `${pluralFiles(count)} — ${copy.label}`,
    };
  });
}

/**
 * Gate 2 — reviewed after the media step, before the upload itself
 * (decision 9). Decision 14: it leads with the delta against what was
 * approved at the last check, not a fresh summary — the final state follows
 * underneath. Presentational only: `delta` and `actual` come in as props
 * (Task 10 wires this to `gateDelta` and the recomputed `StagingSummary`),
 * and approving or declining is left to the caller.
 */
export default function GateTwoScreen({
  delta,
  actual,
  mode,
  onApprove,
  onDecline,
  busy,
}: {
  delta: GateDelta;
  actual: StagingSummary;
  mode: AttachmentMediaMode;
  onApprove: () => void;
  onDecline: () => void;
  busy?: boolean;
}) {
  const verb = mediaJobVerb(mode);
  const rows = deltaRows(delta);
  const pendingRows = stillPendingRows(delta, mode);

  return (
    <>
      <h1 className="m-0 mb-1 text-2xl font-bold">Ready to upload</h1>
      <p className="m-0 mb-5 text-[0.875rem] text-muted">
        {verb ? `The ${verb} step has finished` : "The media step has finished"}, so this is where
        the last check's estimate turned out wrong.
      </p>

      <section>
        <h2 className="m-0 text-base font-semibold">What changed since you approved</h2>
        <div className="mt-3 flex flex-col gap-3">
          {delta.hasChanges ? (
            rows.map((row) => (
              <div key={row.key} className="rounded-lg border border-border p-3">
                <p className="m-0 text-[0.875rem] font-semibold text-text">{row.text}</p>
              </div>
            ))
          ) : (
            <p className="m-0 text-[0.813rem] text-muted">
              Everything came out as expected — no surprises since you approved.
            </p>
          )}
          {/* Still-pending rows sit alongside the delta, not inside its
              conditional — an import holding only an unconvertible file has
              no delta to report, but the file still will not upload. */}
          {pendingRows.map((row) => (
            <div key={row.key} className="rounded-lg border border-border p-3">
              <p className="m-0 text-[0.875rem] font-semibold text-text">{row.text}</p>
            </div>
          ))}
        </div>
      </section>

      <div className="mt-5 min-w-0 overflow-hidden rounded-lg border border-border">
        <table className="w-full table-fixed border-collapse text-[0.813rem]">
          <thead>
            <tr className="border-b border-border bg-elevated text-left text-muted">
              <th className="px-3 py-2 font-medium">What was copied</th>
              <th className="w-40 px-3 py-2 text-right font-medium">Count</th>
            </tr>
          </thead>
          <tbody>
            <tr className="border-b border-border">
              <td className="px-3 py-2 text-text">Conversations</td>
              <td className="px-3 py-2 text-right tabular-nums text-text">
                {actual.conversations.toLocaleString()}
              </td>
            </tr>
            <tr className="border-b border-border">
              <td className="px-3 py-2 text-text">Messages</td>
              <td className="px-3 py-2 text-right tabular-nums text-text">
                {actual.messages.toLocaleString()}
              </td>
            </tr>
            <tr className="border-b border-border">
              <td className="px-3 py-2 text-text">Attachments</td>
              <td className="px-3 py-2 text-right tabular-nums text-text">
                {actual.attachments.toLocaleString()}
              </td>
            </tr>
            <tr className="last:border-b-0">
              <td className="px-3 py-2 text-text">Size copied</td>
              <td className="px-3 py-2 text-right tabular-nums text-text">
                {formatBytes(actual.attachmentBytes)}
              </td>
            </tr>
          </tbody>
        </table>
      </div>

      <p className="m-0 mt-5 text-[0.813rem] text-muted">
        Messages are always uploaded. A skipped attachment leaves a placeholder in the conversation,
        and the message text is kept. Imported conversations can later be removed from your vault in
        the messages area.
      </p>

      <div className="mt-5 flex items-center gap-3">
        <Button variant="primary" size="wide" onClick={onApprove} disabled={busy}>
          Upload to vault
        </Button>
        <Button variant="ghost" onClick={onDecline} disabled={busy}>
          Cancel this import
        </Button>
      </div>
    </>
  );
}
