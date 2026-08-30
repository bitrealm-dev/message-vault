import Button from "../../components/Button";
import { formatBytes } from "../../lib/attachmentProgressCopy";
import type { StagingSummary } from "../../lib/tauri";
import type { AttachmentMediaMode } from "../../lib/types";
import { forecastGroups, mediaJobVerb } from "./gateForecast";

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
}: {
  summary: StagingSummary;
  unknownContacts: number;
  mode: AttachmentMediaMode;
  onApprove: () => void;
  onDecline: () => void;
  busy?: boolean;
}) {
  const verb = mediaJobVerb(mode);
  const groups = verb ? forecastGroups(summary.verdictCounts, mode) : [];

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
                {summary.contactIdentifiers.length.toLocaleString()} ·{" "}
                {unknownContacts.toLocaleString()} new to your vault
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

      {verb ? (
        <section className="mt-5">
          <h2 className="m-0 text-base font-semibold">What to expect after {verb}</h2>
          <p className="m-0 mt-1 text-[0.813rem] text-muted">
            The media step has not run yet, so these are estimates based on the files as staged.
          </p>
          <div className="mt-3 flex flex-col gap-3">
            {groups.map((group) => (
              <div key={group.verdict} className="rounded-lg border border-border p-3">
                <p className="m-0 text-[0.875rem] font-semibold text-text">
                  {group.count.toLocaleString()} files — {group.label}
                </p>
                <p className="m-0 mt-1 text-[0.813rem] text-muted">{group.hint}</p>
              </div>
            ))}
          </div>
        </section>
      ) : null}

      <div className="mt-5 flex items-center gap-3">
        <Button variant="primary" size="wide" onClick={onApprove} disabled={busy}>
          {PRIMARY_LABEL[mode]}
        </Button>
        <Button variant="ghost" onClick={onDecline} disabled={busy}>
          Cancel this import
        </Button>
      </div>
    </>
  );
}
