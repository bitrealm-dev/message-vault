import Button from "../../components/Button";
import { formatBytes } from "../../lib/attachmentProgressCopy";
import type { StagingSummary } from "../../lib/tauri";
import type { AttachmentMediaMode } from "../../lib/types";
import type { GateDelta } from "./gateDelta";
import { mediaJobVerb } from "./gateForecast";

interface DeltaRow {
  key: string;
  count: number;
  text: string;
}

/**
 * The delta rows worth showing, worst first — mirrors Gate 1's own
 * "what needs attention first" ordering. Buckets with nothing in them are
 * dropped; a row reading "0 files" is noise.
 */
function deltaRows(delta: GateDelta, verb: "converting" | "compressing" | null): DeltaRow[] {
  const rows: DeltaRow[] = [
    {
      key: "worse",
      count: delta.worseThanForecast,
      // Decision 45: this bucket cannot say why — crossing the size limit
      // and a conversion failure land here the same way — so the copy
      // states the effect, not a cause the data cannot support.
      text: `${delta.worseThanForecast.toLocaleString()} files that looked fine at the last check will not be uploaded.`,
    },
    {
      key: "failed",
      count: delta.failed,
      text: `${delta.failed.toLocaleString()} files could not be processed and will not be uploaded.`,
    },
    {
      key: "better",
      count: delta.betterThanForecast,
      text: `${delta.betterThanForecast.toLocaleString()} files written off as too big came in under the limit after all.`,
    },
    {
      key: "held",
      count: delta.forecastHeld,
      text: verb
        ? `${delta.forecastHeld.toLocaleString()} files fit as expected after ${verb}.`
        : `${delta.forecastHeld.toLocaleString()} files fit as expected.`,
    },
  ];
  return rows.filter((row) => row.count > 0);
}

/**
 * Gate 2 — reviewed after the media step, before the upload itself
 * (decision 9). Decision 14: it leads with the delta against what was
 * approved at Gate 1, not a fresh summary — the final state follows
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
  const rows = deltaRows(delta, verb);

  return (
    <>
      <h1 className="m-0 mb-1 text-2xl font-bold">Ready to upload</h1>
      <p className="m-0 mb-5 text-[0.875rem] text-muted">
        The media step has finished, so this is where Gate 1's estimate turned out wrong.
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

      <section className="mt-5">
        <h2 className="m-0 text-base font-semibold">
          {verb ? `Final counts after ${verb}` : "Final counts"}
        </h2>
        <div className="mt-3 min-w-0 overflow-hidden rounded-lg border border-border">
          <table className="w-full table-fixed border-collapse text-[0.813rem]">
            <thead>
              <tr className="border-b border-border bg-elevated text-left text-muted">
                <th className="px-3 py-2 font-medium">Ready to upload</th>
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
                <td className="px-3 py-2 text-text">Size to upload</td>
                <td className="px-3 py-2 text-right tabular-nums text-text">
                  {formatBytes(actual.attachmentBytes)}
                </td>
              </tr>
            </tbody>
          </table>
        </div>
      </section>

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
