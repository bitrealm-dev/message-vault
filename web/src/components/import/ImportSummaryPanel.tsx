export type ImportIssue = {
  kind: string;
  step: string;
  item: string;
  reason: string;
};

export type ImportSummaryView = {
  status: "completed" | "failed" | "running";
  parseMessages?: number;
  convertDetail?: string;
  uploadFiles?: number;
  parseMs?: number | null;
  convertMs?: number | null;
  uploadMs?: number | null;
  durationMs: number | null;
  issues: ImportIssue[];
};

function formatIssueKind(kind: string): string {
  if (!kind) return "Issue";
  return kind.charAt(0).toUpperCase() + kind.slice(1).toLowerCase();
}

function formatDuration(milliseconds: number | null | undefined): string {
  if (milliseconds == null) return "—";

  const seconds = Math.max(0, Math.round(milliseconds / 1000));
  const minutes = Math.floor(seconds / 60);
  const remainingSeconds = seconds % 60;
  return minutes > 0 ? `${minutes}m ${remainingSeconds}s` : `${remainingSeconds}s`;
}

export default function ImportSummaryPanel({ summary }: { summary: ImportSummaryView }) {
  const succeeded = summary.status === "completed";
  const running = summary.status === "running";

  return (
    <section className="mt-5">
      <div
        className={`rounded-md p-4 text-[0.875rem] ${
          succeeded ? "bg-ok-soft-bg" : running ? "bg-hover" : "bg-danger-soft-bg"
        }`}
      >
        <h2 className="m-0 text-base font-semibold">
          {succeeded ? "Import complete" : running ? "Import in progress" : "Import failed"}
        </h2>
        <ol className="mb-0 mt-3 space-y-1 pl-5">
          <li>Parse backup{summary.parseMessages != null ? ` · ${summary.parseMessages} messages` : ""}</li>
          <li>Convert attachments{summary.convertDetail ? ` · ${summary.convertDetail}` : ""}</li>
          <li>Upload to vault{summary.uploadFiles != null ? ` · ${summary.uploadFiles} files` : ""}</li>
        </ol>
        <p className="mb-0 mt-3 text-[0.813rem] text-muted">
          Parse {formatDuration(summary.parseMs)} · Convert {formatDuration(summary.convertMs)} ·
          {" "}Upload {formatDuration(summary.uploadMs)} · Total {formatDuration(summary.durationMs)}
        </p>
      </div>

      {summary.issues.length > 0 ? (
        <section className="mt-4">
          <h2 className="m-0 text-base font-semibold">Errors &amp; skips</h2>
          <ul className="mt-2 space-y-2 pl-5 text-[0.813rem]">
            {summary.issues.map((issue, index) => (
              <li key={`${issue.kind}-${issue.step}-${issue.item}-${index}`}>
                {formatIssueKind(issue.kind)} · {issue.step} · {issue.item} — {issue.reason}
              </li>
            ))}
          </ul>
        </section>
      ) : null}
    </section>
  );
}
