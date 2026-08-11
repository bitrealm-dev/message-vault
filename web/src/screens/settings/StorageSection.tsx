import { useState, useEffect, useMemo } from "react";
import { apiClient } from "../../lib/api";
import Button from "../../components/Button";
import ImportSummaryPanel, {
  type ImportIssue,
  type ImportSummaryView,
} from "../../components/import/ImportSummaryPanel";

function formatBytes(bytes: number): string {
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

function formatImportDate(iso: string | null | undefined): string {
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

function formatDuration(milliseconds: number | null | undefined): string {
  if (milliseconds == null) return "—";

  const seconds = Math.max(0, Math.round(milliseconds / 1000));
  const minutes = Math.floor(seconds / 60);
  const remainingSeconds = seconds % 60;
  return minutes > 0 ? `${minutes}m ${remainingSeconds}s` : `${remainingSeconds}s`;
}

const ATTACHMENT_PAGE_SIZE = 20;

const sectionTitle = "m-0 text-[0.938rem] font-semibold text-text";

const sectionHint = "mt-1 text-[0.813rem] text-muted";

const tableWrap = "overflow-x-auto rounded-lg border border-border";

const thStyle =
  "border-b border-border bg-elevated p-2 px-3 text-left text-[0.813rem] font-medium text-muted";

const tdStyle = "border-b border-border p-2 px-3 text-[0.813rem] text-text";

interface ImportRow {
  id: number;
  source: string;
  started_at: string;
  finished_at: string | null;
  message_count: number;
  attachment_count: number;
  duration_ms: number | null;
}

interface TopAttachment {
  id: number;
  original_name: string | null;
  mime_type: string | null;
  size_bytes: number;
  conversation_title: string | null;
  chat_identifier: string;
}

interface ImportDetailResponse {
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

function toNumber(value: unknown): number | undefined {
  return typeof value === "number" && Number.isFinite(value) ? value : undefined;
}

function toString(value: unknown): string | undefined {
  return typeof value === "string" ? value : undefined;
}

function toImportSummaryView(detail: ImportDetailResponse): ImportSummaryView {
  const summary =
    detail.summary && typeof detail.summary === "object"
      ? (detail.summary as Record<string, unknown>)
      : {};
  const durationMs =
    detail.duration_ms ??
    (detail.parse_ms ?? 0) + (detail.convert_ms ?? 0) + (detail.upload_ms ?? 0);

  return {
    status: detail.status === "completed" ? "completed" : "failed",
    parseMessages: toNumber(summary.parse_messages ?? summary.parseMessages),
    convertDetail: toString(summary.convert_detail ?? summary.convertDetail),
    uploadFiles: toNumber(summary.upload_files ?? summary.uploadFiles),
    parseMs: detail.parse_ms,
    convertMs: detail.convert_ms,
    uploadMs: detail.upload_ms,
    durationMs,
    issues: detail.issues,
  };
}

export function StorageSection() {
  const [imports, setImports] = useState<ImportRow[]>([]);
  const [totalBytes, setTotalBytes] = useState(0);
  const [attachmentCount, setAttachmentCount] = useState(0);
  const [topAttachments, setTopAttachments] = useState<TopAttachment[]>([]);
  const [page, setPage] = useState(0);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");
  const [selectedImportId, setSelectedImportId] = useState<number | null>(null);
  const [selectedImport, setSelectedImport] = useState<ImportDetailResponse | null>(null);
  const [selectedImportLoading, setSelectedImportLoading] = useState(false);
  const [selectedImportError, setSelectedImportError] = useState("");

  useEffect(() => {
    setLoading(true);
    setError("");
    Promise.all([
      apiClient.get<{ imports: ImportRow[] }>("/v1/imports"),
      apiClient.get<{
        total_bytes: number;
        attachment_count: number;
        top_attachments: TopAttachment[];
      }>("/v1/account/storage"),
    ])
      .then(([importsRes, usageRes]) => {
        setImports(importsRes.imports ?? []);
        setTotalBytes(usageRes.total_bytes ?? 0);
        setAttachmentCount(usageRes.attachment_count ?? 0);
        setTopAttachments(usageRes.top_attachments ?? []);
        setPage(0);
      })
      .catch((e) => {
        setError(e instanceof Error ? e.message : String(e));
      })
      .finally(() => setLoading(false));
  }, []);

  useEffect(() => {
    if (selectedImportId == null) return;

    const controller = new AbortController();
    setSelectedImportLoading(true);
    setSelectedImportError("");

    apiClient
      .get<ImportDetailResponse>(`/v1/imports/${selectedImportId}`, {
        signal: controller.signal,
      })
      .then((detail) => {
        setSelectedImport(detail);
      })
      .catch((err) => {
        if (controller.signal.aborted) return;
        setSelectedImport(null);
        setSelectedImportError(
          err instanceof Error ? err.message : "Couldn’t load import details.",
        );
      })
      .finally(() => {
        if (!controller.signal.aborted) {
          setSelectedImportLoading(false);
        }
      });

    return () => controller.abort();
  }, [selectedImportId]);

  if (loading) {
    return <div className="text-[0.875rem] text-muted">Loading storage…</div>;
  }

  const pageCount = Math.max(1, Math.ceil(topAttachments.length / ATTACHMENT_PAGE_SIZE));
  const pageRows = topAttachments.slice(
    page * ATTACHMENT_PAGE_SIZE,
    page * ATTACHMENT_PAGE_SIZE + ATTACHMENT_PAGE_SIZE,
  );
  const showDurationColumn = imports.some((row) => row.duration_ms != null);
  const selectedImportSummary = selectedImport ? toImportSummaryView(selectedImport) : null;

  const openImportDetail = (importId: number) => {
    setSelectedImportId(importId);
    setSelectedImport(null);
    setSelectedImportError("");
  };

  return (
    <div className="flex flex-col gap-8">
      {error && (
        <div className="rounded-md border border-danger-soft-border bg-danger-soft-bg p-2 px-3 text-[0.813rem] text-danger">
          {error}
        </div>
      )}

      <section>
        <h3 className={sectionTitle}>Usage</h3>
        <p className={sectionHint}>Attachment storage for this account (original file sizes).</p>
        <div className="mt-3 rounded-lg border border-border bg-elevated p-3 px-4">
          <div className="text-[1.375rem] font-semibold text-text">
            {formatBytes(totalBytes)}
          </div>
          <div className="mt-1 text-[0.813rem] text-muted">
            {attachmentCount.toLocaleString()} attachment{attachmentCount === 1 ? "" : "s"}
          </div>
        </div>
      </section>

      <section>
        <h3 className={sectionTitle}>Import history</h3>
        <p className={sectionHint}>Each vault push or CLI import recorded for this account.</p>
        {imports.length === 0 ? (
          <p className={`${sectionHint} mt-3`}>No imports recorded yet.</p>
        ) : (
          <div className={`${tableWrap} mt-3`}>
            <table className="w-full border-collapse">
              <thead>
                <tr>
                  <th className={thStyle}>Date</th>
                  <th className={thStyle}>Import type</th>
                  <th className={`${thStyle} text-right`}>Messages</th>
                  <th className={`${thStyle} text-right`}>Attachments</th>
                  {showDurationColumn ? (
                    <th className={`${thStyle} text-right`}>Duration</th>
                  ) : null}
                </tr>
              </thead>
              <tbody>
                {imports.map((row) => (
                  <tr
                    key={row.id}
                    className={selectedImportId === row.id ? "bg-hover" : "hover:bg-hover"}
                  >
                    <td
                      className={`${tdStyle} cursor-pointer`}
                      onClick={() => openImportDetail(row.id)}
                      onKeyDown={(event) => {
                        if (event.key === "Enter" || event.key === " ") {
                          event.preventDefault();
                          openImportDetail(row.id);
                        }
                      }}
                      tabIndex={0}
                    >
                      {formatImportDate(row.finished_at ?? row.started_at)}
                    </td>
                    <td
                      className={`${tdStyle} cursor-pointer`}
                      onClick={() => openImportDetail(row.id)}
                      onKeyDown={(event) => {
                        if (event.key === "Enter" || event.key === " ") {
                          event.preventDefault();
                          openImportDetail(row.id);
                        }
                      }}
                      tabIndex={0}
                    >
                      {row.source}
                    </td>
                    <td
                      className={`${tdStyle} cursor-pointer text-right tabular-nums`}
                      onClick={() => openImportDetail(row.id)}
                      onKeyDown={(event) => {
                        if (event.key === "Enter" || event.key === " ") {
                          event.preventDefault();
                          openImportDetail(row.id);
                        }
                      }}
                      tabIndex={0}
                    >
                      {row.message_count.toLocaleString()}
                    </td>
                    <td
                      className={`${tdStyle} cursor-pointer text-right tabular-nums`}
                      onClick={() => openImportDetail(row.id)}
                      onKeyDown={(event) => {
                        if (event.key === "Enter" || event.key === " ") {
                          event.preventDefault();
                          openImportDetail(row.id);
                        }
                      }}
                      tabIndex={0}
                    >
                      {row.attachment_count.toLocaleString()}
                    </td>
                    {showDurationColumn ? (
                      <td
                        className={`${tdStyle} cursor-pointer text-right tabular-nums`}
                        onClick={() => openImportDetail(row.id)}
                        onKeyDown={(event) => {
                          if (event.key === "Enter" || event.key === " ") {
                            event.preventDefault();
                            openImportDetail(row.id);
                          }
                        }}
                        tabIndex={0}
                      >
                        {formatDuration(row.duration_ms)}
                      </td>
                    ) : null}
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
        {selectedImportId != null && (
          <div className="rounded-lg border border-border bg-elevated p-4">
            <div className="flex flex-wrap items-start justify-between gap-3">
              <div>
                <h3 className={sectionTitle}>Import details</h3>
                <p className={sectionHint}>
                  {selectedImport
                    ? `${selectedImport.source} · ${selectedImport.mode} · ${selectedImport.status}`
                    : "Loading import details…"}
                </p>
              </div>
              <Button
                variant="ghost"
                onClick={() => {
                  setSelectedImportId(null);
                  setSelectedImport(null);
                  setSelectedImportLoading(false);
                  setSelectedImportError("");
                }}
                className="!px-3 !py-1.5 !text-[0.813rem]"
              >
                Back to history
              </Button>
            </div>

            {selectedImportLoading && (
              <p className="mt-4 text-[0.813rem] text-muted">Loading import summary…</p>
            )}

            {selectedImportError && (
              <div className="mt-4 rounded-md border border-danger-soft-border bg-danger-soft-bg p-2 px-3 text-[0.813rem] text-danger">
                {selectedImportError}
              </div>
            )}

            {selectedImport && selectedImportSummary ? (
              <>
                <dl className="mt-4 grid gap-3 text-[0.813rem] text-text sm:grid-cols-2 lg:grid-cols-3">
                  <div>
                    <dt className="text-muted">Started</dt>
                    <dd className="mt-1">{formatImportDate(selectedImport.started_at)}</dd>
                  </div>
                  <div>
                    <dt className="text-muted">Finished</dt>
                    <dd className="mt-1">
                      {formatImportDate(selectedImport.finished_at ?? selectedImport.started_at)}
                    </dd>
                  </div>
                  <div>
                    <dt className="text-muted">Duration</dt>
                    <dd className="mt-1">{formatDuration(selectedImportSummary.durationMs)}</dd>
                  </div>
                  <div>
                    <dt className="text-muted">Messages</dt>
                    <dd className="mt-1">{selectedImport.message_count.toLocaleString()}</dd>
                  </div>
                  <div>
                    <dt className="text-muted">Attachments</dt>
                    <dd className="mt-1">{selectedImport.attachment_count.toLocaleString()}</dd>
                  </div>
                  <div>
                    <dt className="text-muted">Bytes uploaded</dt>
                    <dd className="mt-1">{formatBytes(selectedImport.bytes_uploaded)}</dd>
                  </div>
                </dl>
                <ImportSummaryPanel summary={selectedImportSummary} />
              </>
            ) : null}
          </div>
        )}
      </section>

      <section>
        <h3 className={sectionTitle}>Largest attachments</h3>
        <p className={sectionHint}>
          Top {topAttachments.length || 100} attachments by file size
          {topAttachments.length > ATTACHMENT_PAGE_SIZE
            ? ` · ${ATTACHMENT_PAGE_SIZE} per page`
            : ""}
          .
        </p>
        {topAttachments.length === 0 ? (
          <p className={`${sectionHint} mt-3`}>No attachments with sizes yet.</p>
        ) : (
          <div className="mt-3 flex flex-col gap-3">
            <div className={tableWrap}>
              <table className="w-full border-collapse">
                <thead>
                  <tr>
                    <th className={thStyle}>Name</th>
                    <th className={thStyle}>Conversation</th>
                    <th className={`${thStyle} text-right`}>Size</th>
                  </tr>
                </thead>
                <tbody>
                  {pageRows.map((row) => (
                    <tr key={row.id}>
                      <td className={`${tdStyle} max-w-[14rem] truncate`}>
                        {row.original_name || row.mime_type || `Attachment ${row.id}`}
                      </td>
                      <td className={tdStyle}>
                        {row.conversation_title || row.chat_identifier}
                      </td>
                      <td className={`${tdStyle} text-right tabular-nums`}>
                        {formatBytes(row.size_bytes)}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
            {topAttachments.length > ATTACHMENT_PAGE_SIZE && (
              <div className="flex items-center justify-between gap-3">
                <span className="text-[0.75rem] text-muted">
                  Page {page + 1} of {pageCount}
                </span>
                <div className="flex gap-2">
                  <Button
                    disabled={page <= 0}
                    onClick={() => setPage((p) => Math.max(0, p - 1))}
                    className="!px-3 !py-1.5 !text-[0.813rem]"
                  >
                    Back
                  </Button>
                  <Button
                    disabled={page >= pageCount - 1}
                    onClick={() => setPage((p) => Math.min(pageCount - 1, p + 1))}
                    className="!px-3 !py-1.5 !text-[0.813rem]"
                  >
                    Next
                  </Button>
                </div>
              </div>
            )}
          </div>
        )}
      </section>
    </div>
  );
}
