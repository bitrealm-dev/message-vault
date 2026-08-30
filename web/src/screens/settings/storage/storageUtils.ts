import type { ImportIssue, ImportSummaryView } from "../../../components/import/ImportSummaryPanel";
import { formatDateTime } from "../../../lib/formatDate";

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
  attachments_ms: number | null;
  prepare_ms: number | null;
  upload_ms: number | null;
  summary: unknown;
  issues: ImportIssue[];
}

/** Human-readable file size (for example "1.2 MB"). */
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

/** Import start/finish time for table rows, or an em dash when missing. */
export function formatImportDate(iso: string | null | undefined): string {
  if (!iso) return "—";
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso;
  return formatDateTime(iso);
}

/** Finite number, or undefined for anything else. */
function toNumber(value: unknown): number | undefined {
  return typeof value === "number" && Number.isFinite(value) ? value : undefined;
}

/** Map a server import status onto the summary panel's five statuses. */
function toSummaryStatus(status: string): ImportSummaryView["status"] {
  switch (status) {
    case "completed":
      return "completed";
    case "completed_with_issues":
      return "completed_with_issues";
    case "canceled":
      return "canceled";
    case "running":
      return "running";
    default:
      return "failed";
  }
}

/** Human label for a raw server import status, for the import detail panel. */
export function importStatusLabel(status: string): string {
  switch (status) {
    case "running":
      return "Running";
    case "completed":
      return "Completed";
    case "completed_with_issues":
      return "Completed with issues";
    case "failed":
      return "Failed";
    case "canceled":
      return "Canceled";
    default:
      return status;
  }
}

/** Build the import summary panel model from a server import-detail response. */
export function toImportSummaryView(detail: ImportDetailResponse): ImportSummaryView {
  const summary =
    detail.summary && typeof detail.summary === "object"
      ? (detail.summary as Record<string, unknown>)
      : {};
  const hasAnyStageTiming =
    detail.parse_ms != null ||
    detail.attachments_ms != null ||
    detail.prepare_ms != null ||
    detail.upload_ms != null;
  const durationMs =
    detail.duration_ms ??
    (hasAnyStageTiming
      ? (detail.parse_ms ?? 0) +
        (detail.attachments_ms ?? 0) +
        (detail.prepare_ms ?? 0) +
        (detail.upload_ms ?? 0)
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
    attachmentsMs: detail.attachments_ms,
    prepareMs: detail.prepare_ms,
    uploadMs: detail.upload_ms,
    durationMs,
    issues: detail.issues,
  };
}
