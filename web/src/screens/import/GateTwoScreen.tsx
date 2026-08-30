import Button from "../../components/Button";
import { formatBytes } from "../../lib/attachmentProgressCopy";
import type { StagingSummary } from "../../lib/tauri";
import type { AttachmentMediaMode } from "../../lib/types";
import type { GateDelta, StillFlaggedItem } from "./gateDelta";
import { mediaJobVerb, verdictCopy } from "./gateForecast";

interface DeltaRow {
  key: string;
  text: string;
}

/**
 * The delta rows worth showing, worst first — mirrors Gate 1's own
 * "what needs attention first" ordering. A bucket with nothing in it is
 * dropped; a row reading "0 files" is noise.
 */
function deltaRows(delta: GateDelta, mode: AttachmentMediaMode): DeltaRow[] {
  const regressed = delta.stillFlagged.filter((item) => item.regressed);
  const rows: DeltaRow[] = [
    {
      key: "lost",
      text:
        // Decision 45: this count cannot say why — crossing the size limit
        // and a conversion failure land here the same way — so the copy
        // states the effect, not a cause the data cannot support.
        delta.lostCount > 0
          ? `${delta.lostCount.toLocaleString()} files will not be uploaded.`
          : "",
    },
    {
      key: "regressed",
      text:
        regressed.length > 0
          ? // Decision 45's other half: these were under the limit (or not
            // flagged at all) at the last check, and are now over. They
            // were processed — this is not "could not be processed".
            `${regressed.length.toLocaleString()} files that were fine at the last check are now over the limit.`
          : "",
    },
    {
      key: "cameOutFine",
      text:
        delta.cameOutFine > 0
          ? `${delta.cameOutFine.toLocaleString()} files written off as too big came in under the limit after all.`
          : "",
    },
  ].filter((row) => row.text.length > 0);

  const stillPending = delta.stillFlagged.filter((item) => !item.regressed);
  if (stillPending.length > 0) {
    rows.push(...stillPendingRows(stillPending, mode));
  }
  return rows;
}

/** Groups the not-yet-resolved rows by verdict, using Gate 1's own wording. */
function stillPendingRows(items: StillFlaggedItem[], mode: AttachmentMediaMode): DeltaRow[] {
  const counts = new Map<string, number>();
  for (const item of items) {
    counts.set(item.verdict, (counts.get(item.verdict) ?? 0) + 1);
  }
  return [...counts.entries()].map(([verdict, count]) => {
    const copy = verdictCopy(verdict as StillFlaggedItem["verdict"], mode);
    return {
      key: `pending-${verdict}`,
      text: `${count.toLocaleString()} files — ${copy.label}`,
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
  const rows = deltaRows(delta, mode);

  return (
    <>
      <h1 className="m-0 mb-1 text-2xl font-bold">Ready to upload</h1>
      <p className="m-0 mb-5 text-[0.875rem] text-muted">
        {verb ? `The ${verb} step has finished` : "The media step has finished"}, so this is
        where the last check's estimate turned out wrong.
      </p>

      <section>
        <h2 className="m-0 text-base font-semibold">What changed since you approved</h2>
        {delta.hasChanges ? (
          <div className="mt-3 flex flex-col gap-3">
            {rows.map((row) => (
              <div key={row.key} className="rounded-lg border border-border p-3">
                <p className="m-0 text-[0.875rem] font-semibold text-text">{row.text}</p>
              </div>
            ))}
          </div>
        ) : (
          <p className="m-0 mt-1 text-[0.813rem] text-muted">
            Everything came out as expected — no surprises since you approved.
          </p>
        )}
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
        Messages are always uploaded. A skipped attachment leaves a placeholder in the
        conversation, and the message text is kept. Imported conversations can later be removed
        from your vault in the messages area.
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
