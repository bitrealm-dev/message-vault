import { useCallback, useState } from "react";
import Button from "../components/Button";
import ConfirmDialog from "../components/ConfirmDialog";
import { apiClient } from "../lib/api";
import { formatLocaleDate } from "../lib/formatDate";
import { useAsyncAction } from "../lib/useAsyncAction";
import { useResource } from "../lib/useResource";

interface TrashEntry {
  id: string;
  label: string;
  message_count: number;
  deleted_at: string;
  conversation_exists: boolean;
}

const TRASH_RESOURCE_KEY = "trash";

export default function TrashScreen() {
  const [message, setMessage] = useState("");
  const [confirmOpen, setConfirmOpen] = useState(false);
  // Restore and empty are fire-and-forget from a click handler, so their
  // failures need somewhere to land — otherwise a failed restore is
  // indistinguishable from a click that never registered.
  const { busy: deleting, error: actionError, run } = useAsyncAction();

  const fetchTrash = useCallback(async (signal: AbortSignal) => {
    const res = await apiClient.get<{ trash: TrashEntry[] }>("/v1/export/trash", {
      signal,
    });
    return res.trash;
  }, []);

  const { data, loading, error, reload } = useResource(TRASH_RESOURCE_KEY, fetchTrash);

  const entries = data ?? [];

  const restore = (id: string) =>
    run(async () => {
      setMessage("");
      await apiClient.post(`/v1/trash/${id}/restore`);
      setMessage("Conversation restored.");
      reload();
    });

  const emptyTrash = () =>
    run(async () => {
      setMessage("");
      try {
        await apiClient.post("/v1/trash/empty");
        setMessage("Trash emptied.");
        reload();
      } finally {
        setConfirmOpen(false);
      }
    });

  if (loading) return <div className="p-6 text-[0.875rem] text-muted">Loading…</div>;

  return (
    <div className="max-w-[700px] p-6">
      <div className="mb-6 flex items-center justify-between">
        <h2 className="m-0">Trash</h2>
        {entries.length > 0 && (
          <Button
            variant="danger"
            disabled={deleting}
            onClick={() => setConfirmOpen(true)}
            size="sm"
          >
            Empty trash
          </Button>
        )}
      </div>
      {(error || actionError) && (
        <div className="mb-4 rounded border border-danger-soft-border bg-danger-soft-bg px-3 py-2 text-[0.813rem] text-danger">
          {error || actionError}
        </div>
      )}
      {message && (
        <div className="mb-4 rounded bg-ok-soft-bg px-3 py-2 text-[0.813rem] text-ok-soft-text">
          {message}
        </div>
      )}
      {entries.length === 0 ? (
        <div className="text-[0.875rem] text-muted">Trash is empty.</div>
      ) : (
        entries.map((entry) => (
          <div
            key={entry.id}
            className="flex items-center justify-between border-b border-border p-3"
          >
            <div>
              <div className="text-[0.875rem] font-medium">{entry.label}</div>
              <div className="text-[0.75rem] text-muted">
                {entry.message_count} message{entry.message_count !== 1 ? "s" : ""} · deleted{" "}
                {formatLocaleDate(entry.deleted_at)}
              </div>
            </div>
            <Button onClick={() => void restore(entry.id)} size="xs">
              Restore
            </Button>
          </div>
        ))
      )}

      <ConfirmDialog
        open={confirmOpen}
        title="Empty trash?"
        body="Permanently delete all trashed messages? This cannot be undone."
        confirmLabel="Empty trash"
        danger
        busy={deleting}
        onClose={() => setConfirmOpen(false)}
        onConfirm={() => void emptyTrash()}
      />
    </div>
  );
}
