"use client";

import {
  formatBytes,
  type TopAttachmentItem,
  type VaultImportListItem,
} from "@/lib/storageTypes";
import { useCallback, useEffect, useState } from "react";

function formatImportDate(iso: string | null): string {
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

export function SettingsStorageForm() {
  const [imports, setImports] = useState<VaultImportListItem[]>([]);
  const [totalBytes, setTotalBytes] = useState(0);
  const [attachmentCount, setAttachmentCount] = useState(0);
  const [topAttachments, setTopAttachments] = useState<TopAttachmentItem[]>(
    [],
  );
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const [importsRes, usageRes] = await Promise.all([
        fetch("/api/settings/imports"),
        fetch("/api/settings/usage"),
      ]);
      const importsJson = (await importsRes.json()) as {
        imports?: VaultImportListItem[];
        error?: string;
      };
      const usageJson = (await usageRes.json()) as {
        totalBytes?: number;
        attachmentCount?: number;
        topAttachments?: TopAttachmentItem[];
        error?: string;
      };
      if (!importsRes.ok) {
        throw new Error(importsJson.error ?? "Couldn’t load import history.");
      }
      if (!usageRes.ok) {
        throw new Error(usageJson.error ?? "Couldn’t load storage usage.");
      }
      setImports(importsJson.imports ?? []);
      setTotalBytes(usageJson.totalBytes ?? 0);
      setAttachmentCount(usageJson.attachmentCount ?? 0);
      setTopAttachments(usageJson.topAttachments ?? []);
    } catch (err) {
      setError(
        err instanceof Error ? err.message : "Couldn’t load storage details.",
      );
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  if (loading) {
    return <p className="text-[14px] text-muted">Loading storage…</p>;
  }

  return (
    <div className="space-y-8">
      {error && (
        <p className="rounded-md border border-red-200 bg-red-50 px-3 py-2 text-[13px] text-red-800">
          {error}
        </p>
      )}

      <section className="space-y-3">
        <div>
          <h2 className="text-[15px] font-semibold text-text">Usage</h2>
          <p className="mt-1 text-[13px] text-muted">
            Attachment storage for this account (original file sizes).
          </p>
        </div>
        <div className="rounded-lg border border-border bg-surface px-4 py-3">
          <p className="text-[22px] font-semibold tracking-tight text-text">
            {formatBytes(totalBytes)}
          </p>
          <p className="mt-1 text-[13px] text-muted">
            {attachmentCount.toLocaleString()} attachment
            {attachmentCount === 1 ? "" : "s"}
          </p>
        </div>
      </section>

      <section className="space-y-3">
        <div>
          <h2 className="text-[15px] font-semibold text-text">Import history</h2>
          <p className="mt-1 text-[13px] text-muted">
            Each vault push or CLI import recorded for this account.
          </p>
        </div>
        {imports.length === 0 ? (
          <p className="text-[13px] text-muted">No imports recorded yet.</p>
        ) : (
          <div className="overflow-x-auto rounded-lg border border-border">
            <table className="min-w-full text-left text-[13px]">
              <thead className="border-b border-border bg-surface text-muted">
                <tr>
                  <th className="px-3 py-2 font-medium">Date</th>
                  <th className="px-3 py-2 font-medium">Import type</th>
                  <th className="px-3 py-2 font-medium text-right">Messages</th>
                  <th className="px-3 py-2 font-medium text-right">
                    Attachments
                  </th>
                </tr>
              </thead>
              <tbody>
                {imports.map((row) => (
                  <tr
                    key={row.id}
                    className="border-b border-border last:border-b-0"
                  >
                    <td className="px-3 py-2 text-text">
                      {formatImportDate(row.finishedAt ?? row.startedAt)}
                    </td>
                    <td className="px-3 py-2 text-text">{row.source}</td>
                    <td className="px-3 py-2 text-right tabular-nums text-text">
                      {row.messageCount.toLocaleString()}
                    </td>
                    <td className="px-3 py-2 text-right tabular-nums text-text">
                      {row.attachmentCount.toLocaleString()}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </section>

      <section className="space-y-3">
        <div>
          <h2 className="text-[15px] font-semibold text-text">
            Largest attachments
          </h2>
          <p className="mt-1 text-[13px] text-muted">
            Top attachments by file size.
          </p>
        </div>
        {topAttachments.length === 0 ? (
          <p className="text-[13px] text-muted">No attachments with sizes yet.</p>
        ) : (
          <div className="overflow-x-auto rounded-lg border border-border">
            <table className="min-w-full text-left text-[13px]">
              <thead className="border-b border-border bg-surface text-muted">
                <tr>
                  <th className="px-3 py-2 font-medium">Name</th>
                  <th className="px-3 py-2 font-medium">Conversation</th>
                  <th className="px-3 py-2 font-medium text-right">Size</th>
                </tr>
              </thead>
              <tbody>
                {topAttachments.map((row) => (
                  <tr
                    key={row.id}
                    className="border-b border-border last:border-b-0"
                  >
                    <td className="max-w-[14rem] truncate px-3 py-2 text-text">
                      {row.originalName || row.mimeType || `Attachment ${row.id}`}
                    </td>
                    <td className="px-3 py-2 text-text">
                      {row.conversationTitle || row.chatIdentifier}
                    </td>
                    <td className="px-3 py-2 text-right tabular-nums text-text">
                      {formatBytes(row.sizeBytes)}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </section>
    </div>
  );
}
