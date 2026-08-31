import type { ReactNode } from "react";
import Button from "../../components/Button";
import { formatBytes } from "../../lib/attachmentProgressCopy";
import type { StagingSummary } from "../../lib/tauri";
import type { AttachmentMediaMode } from "../../lib/types";
import { forecastGroups, mediaJobVerb, pluralFiles } from "./gateForecast";

const PRIMARY_LABEL: Record<AttachmentMediaMode, string> = {
  convert: "Convert media",
  compress: "Compress media",
  copy: "Upload to vault",
  skip: "Upload to vault",
};

/**
 * Gate 1 — reviewed after staging, before the media step spends time
 * converting or compressing every file (decision 9). Presentational only:
 * every count comes in as a prop, and approving or declining is left to the
 * caller (Task 10 wires this up to the session and the contact-match call).
 */
export default function GateOneScreen({
  summary,
  unknownContacts,
  mode,
  onApprove,
  onDecline,
  busy,
  mediaToolsMissing,
  mediaPartiallyRan,
  identityPanel,
}: {
  summary: StagingSummary;
  /** Null while the contact-match lookup is in flight or failed — the "new
   * to your vault" clause is a nicety, not a blocker, so a failed lookup
   * just omits it rather than stalling the gate. */
  unknownContacts: number | null;
  mode: AttachmentMediaMode;
  onApprove: () => void;
  onDecline: () => void;
  busy?: boolean;
  /** True when convert/compress is selected and ffmpeg was not found —
   * disables approval rather than letting the media step fail later. */
  mediaToolsMissing?: boolean;
  /** True only when this landing is a resume that found the media step
   * partway through (ffmpeg went missing mid pass). The folder may already
   * hold a mix of originals and converted files, so the estimates line
   * below would be wrong — it claims the media step hasn't run yet. */
  mediaPartiallyRan?: boolean;
  /** The backup's identity list, composed by the caller (null-safe: omit to hide). */
  identityPanel?: ReactNode;
}) {
  const verb = mediaJobVerb(mode);
  // Decision 11 renders this breakdown unconditionally: copy/skip has no
  // media step, but the exact verdicts (over the limit, not audio or
  // video, …) are still worth surfacing before the user commits to an
  // upload that will drop some of these files.
  const groups = forecastGroups(summary.verdictCounts, mode);
  const toolsBlocked = verb != null && Boolean(mediaToolsMissing);

  return (
    <>
      <h1 className="m-0 mb-1 text-2xl font-bold">Review what was copied</h1>
      <p className="m-0 mb-5 text-[0.875rem] text-muted">
        These counts are read from the staged files, so they are exact.
      </p>

      <div className="min-w-0 overflow-hidden rounded-lg border border-border">
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
                {summary.conversations.toLocaleString()}
              </td>
            </tr>
            <tr className="border-b border-border">
              <td className="px-3 py-2 text-text">Messages</td>
              <td className="px-3 py-2 text-right tabular-nums text-text">
                {summary.messages.toLocaleString()}
              </td>
            </tr>
            <tr className="border-b border-border">
              <td className="px-3 py-2 text-text">Contacts</td>
              <td className="px-3 py-2 text-right tabular-nums text-text">
                {summary.contactIdentifiers.length.toLocaleString()}
                {unknownContacts != null
                  ? ` · ${unknownContacts.toLocaleString()} new to your vault`
                  : ""}
              </td>
            </tr>
            <tr className="border-b border-border">
              <td className="px-3 py-2 text-text">Attachments</td>
              <td className="px-3 py-2 text-right tabular-nums text-text">
                {summary.attachments.toLocaleString()}
              </td>
            </tr>
            <tr className="last:border-b-0">
              <td className="px-3 py-2 text-text">Size copied</td>
              <td className="px-3 py-2 text-right tabular-nums text-text">
                {formatBytes(summary.attachmentBytes)}
              </td>
            </tr>
          </tbody>
        </table>
      </div>

      {identityPanel ? (
        <section className="mt-5">
          <h2 className="m-0 text-base font-semibold">Addresses this backup sent from</h2>
          <div className="mt-3">{identityPanel}</div>
        </section>
      ) : null}

      {verb || groups.length > 0 ? (
        <section className="mt-5">
          <h2 className="m-0 text-base font-semibold">
            {verb ? `What to expect after ${verb}` : "Files against the upload limit"}
          </h2>
          <p className="m-0 mt-1 text-[0.813rem] text-muted">
            {verb
              ? mediaPartiallyRan
                ? "The media step needs its tools to finish. Approving here picks up where it left off, once they're available."
                : "The media step has not run yet, so these are estimates based on the files as staged."
              : "There is no media step in this mode, so these sizes are exact, read straight from the staged files."}
          </p>
          <div className="mt-3 flex flex-col gap-3">
            {groups.map((group) => (
              <div key={group.verdict} className="rounded-lg border border-border p-3">
                <p className="m-0 text-[0.875rem] font-semibold text-text">
                  {pluralFiles(group.count)} — {group.label}
                </p>
                <p className="m-0 mt-1 text-[0.813rem] text-muted">{group.hint}</p>
              </div>
            ))}
          </div>
        </section>
      ) : null}

      {toolsBlocked ? (
        <p className="m-0 mt-5 text-[0.813rem] text-muted">
          This step needs ffmpeg. Set its folder in Settings, then come back to Import.
        </p>
      ) : null}

      <div className="mt-5 flex items-center gap-3">
        <Button variant="primary" size="wide" onClick={onApprove} disabled={busy || toolsBlocked}>
          {PRIMARY_LABEL[mode]}
        </Button>
        <Button variant="ghost" onClick={onDecline} disabled={busy}>
          Cancel this import
        </Button>
      </div>
    </>
  );
}
