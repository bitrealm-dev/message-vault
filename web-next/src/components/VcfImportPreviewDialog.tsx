"use client";

import { useEffect, useMemo, useState } from "react";
import { isReservedLabelName, reservedLabelError } from "@/lib/reservedLabels";
import type {
  VcfCategoryMapping,
  VcfImportPreview,
} from "@/lib/contactsVcfImport";

export type VcfCategoryRow = {
  source: string;
  target: string;
  enabled: boolean;
  matchedCount: number;
};

function rowsFromPreview(preview: VcfImportPreview): VcfCategoryRow[] {
  return preview.categories.map((c) => ({
    source: c.source,
    target: c.source,
    enabled: true,
    matchedCount: c.matchedCount,
  }));
}

export function VcfImportPreviewDialog({
  fileName,
  preview,
  busy,
  onDismiss,
  onConfirm,
}: {
  fileName: string;
  preview: VcfImportPreview;
  busy: boolean;
  onDismiss: () => void;
  onConfirm: (mappings: VcfCategoryMapping[]) => void;
}) {
  const [rows, setRows] = useState<VcfCategoryRow[]>(() =>
    rowsFromPreview(preview),
  );

  useEffect(() => {
    setRows(rowsFromPreview(preview));
  }, [preview]);

  const validationError = useMemo(() => {
    const enabledTargets = new Map<string, string>();
    for (const row of rows) {
      if (!row.enabled) continue;
      const target = row.target.trim();
      if (!target) {
        return `Destination label required for “${row.source}”`;
      }
      if (isReservedLabelName(target)) {
        return reservedLabelError(target);
      }
      const key = target.toLowerCase();
      const prev = enabledTargets.get(key);
      if (prev && prev.toLowerCase() !== row.source.toLowerCase()) {
        return `Multiple categories map to “${target}”`;
      }
      enabledTargets.set(key, row.source);
    }
    return null;
  }, [rows]);

  const enabledCount = rows.filter((r) => r.enabled).length;

  return (
    <div
      className="fixed inset-0 z-[200] flex items-center justify-center bg-scrim px-4"
      role="presentation"
      onClick={() => {
        if (!busy) onDismiss();
      }}
    >
      <div
        role="dialog"
        aria-modal="true"
        aria-labelledby="vcf-import-preview-title"
        className="flex max-h-[min(40rem,calc(100vh-2rem))] w-full max-w-xl flex-col rounded-xl border border-border bg-popover p-5 shadow-[0_16px_48px_rgba(0,0,0,0.5)]"
        onClick={(e) => e.stopPropagation()}
      >
        <h2
          id="vcf-import-preview-title"
          className="text-[16px] font-semibold text-text"
        >
          Import VCF
        </h2>
        <p className="mt-1 truncate text-[12px] text-muted" title={fileName}>
          {fileName}
        </p>

        <div className="mt-4 space-y-2 text-[13px] text-text">
          <p>
            <span className="font-semibold tabular-nums">{preview.matched}</span>{" "}
            of{" "}
            <span className="tabular-nums">{preview.cardsTotal}</span> address-book
            cards match phone numbers in this vault’s messages.
          </p>
          {(preview.unmatched > 0 || preview.skippedNoPhone > 0) && (
            <p className="text-muted">
              {preview.unmatched > 0 && (
                <>
                  {preview.unmatched} unmatched
                  {preview.skippedNoPhone > 0 ? ", " : ""}
                </>
              )}
              {preview.skippedNoPhone > 0 && (
                <>{preview.skippedNoPhone} with no usable phone</>
              )}{" "}
              will be ignored.
            </p>
          )}
          <p className="text-muted">
            Notes, photos, emails, and other VCF fields are not stored. Selected
            categories are copied once into vault labels — this is not a sync.
          </p>
        </div>

        <div className="mt-4 min-h-0 flex-1 overflow-auto rounded-lg border border-border">
          {rows.length === 0 ? (
            <p className="px-3 py-4 text-[13px] text-muted">
              No categories found on matched contacts. Import will still create
              or update names and phones for matched cards.
            </p>
          ) : (
            <table className="w-full text-left text-[13px]">
              <thead className="sticky top-0 bg-elevated text-[11px] font-semibold uppercase tracking-wider text-muted">
                <tr>
                  <th className="w-10 px-3 py-2">Use</th>
                  <th className="px-3 py-2">VCF category</th>
                  <th className="px-3 py-2">Vault label</th>
                  <th className="w-16 px-3 py-2 text-right">#</th>
                </tr>
              </thead>
              <tbody>
                {rows.map((row, i) => (
                  <tr key={row.source} className="border-t border-border">
                    <td className="px-3 py-2 align-middle">
                      <input
                        type="checkbox"
                        className="accent-accent"
                        checked={row.enabled}
                        disabled={busy}
                        aria-label={`Import category ${row.source}`}
                        onChange={(e) => {
                          const enabled = e.target.checked;
                          setRows((prev) =>
                            prev.map((r, idx) =>
                              idx === i ? { ...r, enabled } : r,
                            ),
                          );
                        }}
                      />
                    </td>
                    <td className="px-3 py-2 align-middle text-text">
                      {row.source}
                    </td>
                    <td className="px-3 py-2 align-middle">
                      <input
                        type="text"
                        className="w-full rounded-md border border-border bg-panel px-2 py-1 text-text outline-none focus:border-accent disabled:opacity-50"
                        value={row.target}
                        disabled={busy || !row.enabled}
                        onChange={(e) => {
                          const target = e.target.value;
                          setRows((prev) =>
                            prev.map((r, idx) =>
                              idx === i ? { ...r, target } : r,
                            ),
                          );
                        }}
                      />
                    </td>
                    <td className="px-3 py-2 text-right tabular-nums text-muted align-middle">
                      {row.matchedCount}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
        </div>

        {validationError && (
          <p className="mt-3 text-[13px] text-danger">{validationError}</p>
        )}

        <div className="mt-4 flex items-center justify-between gap-3">
          <p className="text-[12px] text-muted">
            {rows.length === 0
              ? "No categories to copy"
              : `${enabledCount} categor${enabledCount === 1 ? "y" : "ies"} selected`}
          </p>
          <div className="flex justify-end gap-2">
            <button
              type="button"
              className="rounded-md px-3 py-1.5 text-[13px] text-muted hover:bg-hover disabled:opacity-50"
              disabled={busy}
              onClick={onDismiss}
            >
              Cancel
            </button>
            <button
              type="button"
              className="rounded-md bg-accent px-3 py-1.5 text-[13px] font-medium text-sent-text disabled:opacity-50"
              disabled={busy || !!validationError || preview.matched === 0}
              onClick={() => {
                onConfirm(
                  rows.map((r) => ({
                    source: r.source,
                    target: r.target.trim() || r.source,
                    enabled: r.enabled,
                  })),
                );
              }}
            >
              {busy ? "Importing…" : "Import matched"}
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
