import type { ImportIssue, ImportSummaryView } from "../../../components/import/ImportSummaryPanel";

export const ATTACHMENT_PAGE_SIZE = 20;

export const sectionTitle = "m-0 text-[0.938rem] font-semibold text-text";
export const sectionHint = "mt-1 text-[0.813rem] text-muted";
export const tableWrap = "overflow-x-auto rounded-lg border border-border";
export const thStyle =
  "border-b border-border bg-elevated p-2 px-3 text-left text-[0.813rem] font-medium text-muted";
export const tdStyle = "border-b border-border p-2 px-3 text-[0.813rem] text-text";

export interface ImportRow {
  id: number;
  source: string;
  started_at: string;
  finished_at: string | null;
  message_count: number;
  attachment_count: number;
  bytes_uploaded: number;
}

export interface TopAttachment {
  id: number;
  original_name: string | null;
  mime_type: string | null;
  size_bytes: number;
  conversation_title: string | null;
  chat_identifier: string;
}

export interface ImportDetailResponse {
  id: number;
  source: string;
  tool: string | null;
  mode: string;
  status: string;
  started_at: string;
  finished_at: string | null;
  message_count: number;
  attachment_count: number;
  bytes_uploaded: number;
  duration_ms: number | null;
  parse_ms: number | null;
  convert_ms: number | null;
  upload_ms: number | null;
  summary: unknown;
  issues: ImportIssue[];
}

export function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes <= 0) return "0 B";
  const units = ["B", "KB", "MB", "GB", "TB"];
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  const digits = value >= 10 || unit === 0 ? 0 : 1;
  return `${value.toFixed(digits)} ${units[unit]}`;
}

export function formatImportDate(iso: string | null | undefined): string {
  if (!iso) return "—";
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso;
  return d.toLocaleString(undefined, {
    year: "numeric",
    month: "short",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  });
}

function toNumber(value: unknown): number | undefined {
  return typeof value === "number" && Number.isFinite(value) ? value : undefined;
}

/** Any status the server does not report as in-flight or successful counts as failed. */
function toSummaryStatus(status: string): ImportSummaryView["status"] {
  switch (status) {
    case "completed":
      return "completed";
    case "canceled":
      return "canceled";
    case "running":
      return "running";
    default:
      return "failed";
  }
}

export function toImportSummaryView(detail: ImportDetailResponse): ImportSummaryView {
  const summary =
    detail.summary && typeof detail.summary === "object"
      ? (detail.summary as Record<string, unknown>)
      : {};
  const hasAnyStageTiming =
    detail.parse_ms != null || detail.convert_ms != null || detail.upload_ms != null;
  const durationMs = detail.duration_ms ?? (hasAnyStageTiming
    ? (detail.parse_ms ?? 0) + (detail.convert_ms ?? 0) + (detail.upload_ms ?? 0)
    : null);

  return {
    status: toSummaryStatus(detail.status),
    filesTotal: toNumber(summary.files_total ?? summary.filesTotal),
    filesSucceeded: toNumber(summary.files_succeeded ?? summary.filesSucceeded),
    filesFailed: toNumber(summary.files_failed ?? summary.filesFailed),
    filesSkipped: toNumber(summary.files_skipped ?? summary.filesSkipped),
    messagesParsed: toNumber(
      summary.messages_parsed ??
        summary.messagesParsed ??
        summary.parse_messages ??
        summary.parseMessages,
    ),
    messagesAttempted: toNumber(summary.messages_attempted ?? summary.messagesAttempted),
    messagesInserted:
      toNumber(summary.messages_inserted ?? summary.messagesInserted) ?? detail.message_count,
    messagesDeduped: toNumber(summary.messages_deduped ?? summary.messagesDeduped),
    messagesFailed: toNumber(summary.messages_failed ?? summary.messagesFailed),
    parseMs: detail.parse_ms,
    convertMs: detail.convert_ms,
    uploadMs: detail.upload_ms,
    durationMs,
    issues: detail.issues,
  };
}
