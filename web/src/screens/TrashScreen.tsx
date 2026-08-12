import { useCallback, useState } from "react";
import { apiClient } from "../lib/api";
import { useResource } from "../lib/useResource";
import Button from "../components/Button";
import ConfirmDialog from "../components/ConfirmDialog";

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
  const [deleting, setDeleting] = useState(false);

  const fetchTrash = useCallback(async (signal: AbortSignal) => {
    const res = await apiClient.get<{ trash: TrashEntry[] }>("/v1/export/trash", {
      signal,
    });
    return res.trash;
  }, []);

  const { data, loading, error, reload } = useResource(
    TRASH_RESOURCE_KEY,
    fetchTrash,
  );

  const entries = data ?? [];

  const restore = async (id: string) => {
    await apiClient.post(`/v1/trash/${id}/restore`);
    setMessage("Conversation restored.");
    reload();
  };

  const emptyTrash = async () => {
    setDeleting(true);
    try {
      await apiClient.post("/v1/trash/empty");
      setMessage("Trash emptied.");
      reload();
    } finally {
      setDeleting(false);
      setConfirmOpen(false);
    }
  };

  if (loading) return <div className="p-6 text-[0.875rem] text-muted">Loading…</div>;

  return (
    <div className="max-w-[700px] p-6">
      <div className="mb-6 flex items-center justify-between">
        <h2 className="m-0">Trash</h2>
        {entries.length > 0 && (
          <Button variant="danger" disabled={deleting} onClick={() => setConfirmOpen(true)}
            className="!px-3 !py-1.5 !text-[0.813rem]">
            Empty trash
          </Button>
        )}
      </div>
      {error && (
        <div className="mb-4 rounded border border-danger-soft-border bg-danger-soft-bg px-3 py-2 text-[0.813rem] text-danger">
          {error}
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
          <div key={entry.id} className="flex items-center justify-between border-b border-border p-3">
            <div>
              <div className="text-[0.875rem] font-medium">{entry.label}</div>
              <div className="text-[0.75rem] text-muted">
                {entry.message_count} message{entry.message_count !== 1 ? "s" : ""} · deleted {new Date(entry.deleted_at).toLocaleDateString()}
              </div>
            </div>
            <Button onClick={() => restore(entry.id)} className="!px-3 !py-1 !text-[0.813rem]">
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
