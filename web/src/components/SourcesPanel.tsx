import { useCallback } from "react";
import { apiClient } from "../lib/api";
import { useResource } from "../lib/useResource";
import ModalShell from "./ModalShell";

interface SourceInfo {
  backup_name: string;
  message_count: number;
  unique_count: number;
  percentage: number;
}

export default function SourcesPanel({
  conversationId,
  onClose,
}: {
  conversationId: string | null;
  onClose: () => void;
}) {
  const fetchSources = useCallback(
    async (signal: AbortSignal) => {
      const res = await apiClient.get<{ sources: SourceInfo[] }>(
        `/v1/export/conversations/${conversationId}/sources`,
        { signal },
      );
      return res.sources;
    },
    [conversationId],
  );

  const { data, loading, error } = useResource(conversationId, fetchSources);

  if (!conversationId) return null;

  const sources = data ?? [];
  const total = sources.reduce((sum, s) => sum + s.unique_count, 0);

  return (
    <ModalShell
      open={!!conversationId}
      onOpenChange={(o) => {
        if (o) return;
        onClose();
      }}
      label="Sources"
      variant="drawer"
    >
      <div className="mb-4 flex justify-between">
        <h2 className="m-0 text-[1.125rem]">Sources</h2>
        <button onClick={onClose} className="cursor-pointer border-none bg-none text-[1.25rem] text-muted">×</button>
      </div>

      {loading ? (
        <div className="text-[0.875rem] text-muted">Loading…</div>
      ) : error ? (
        <div className="rounded border border-danger-soft-border bg-danger-soft-bg p-2 text-[0.813rem] text-danger">
          {error}
        </div>
      ) : sources.length === 0 ? (
        <div className="text-[0.875rem] text-muted">No source data available.</div>
      ) : (
        <>
          {sources.map((s, i) => (
            <div key={i} className="mb-3 rounded bg-elevated p-2">
              <div className="text-[0.875rem] font-medium">{s.backup_name}</div>
              <div className="text-[0.75rem] text-muted">
                {s.message_count.toLocaleString()} messages ({s.percentage}% of total)
              </div>
              <div className="text-[0.75rem] text-muted">
                {s.unique_count.toLocaleString()} unique
              </div>
            </div>
          ))}
          <div className="mt-2 text-[0.813rem] text-muted">
            Net total: {total.toLocaleString()} unique messages
          </div>
        </>
      )}
    </ModalShell>
  );
}
