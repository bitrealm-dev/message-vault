import { useCallback } from "react";
import { Link, useSearchParams } from "react-router-dom";
import Button from "../components/Button";
import { apiErrorMessage } from "../lib/apiErrorMessage";
import { useRestoreConversation } from "../lib/trash";
import { getConversation, listConversations } from "../lib/vaultApi";
import { keys } from "../lib/vaultKeys";
import { useVaultQuery } from "../lib/vaultQuery";

/** Only `total` is read; the rows themselves are rendered by the list column. */

/**
 * Trashed conversations are listed in the left column by the shared
 * conversation list; this pane reports how much is in the trash and reflects
 * the header's trash search. Only the count is fetched — one row is enough to
 * read `total` off the page response.
 */
function trashQuery(search: string): string {
  const term = search.trim();
  return term ? `trashed:yes ${term}` : "trashed:yes";
}

/** The `tsel` param as a positive conversation id, or null when absent or malformed. */
function selectedIdFromParam(raw: string | null): number | null {
  if (raw === null || !/^\d+$/.test(raw)) return null;
  const n = Number(raw);
  return Number.isSafeInteger(n) && n > 0 ? n : null;
}

export default function TrashScreen() {
  const [searchParams, setSearchParams] = useSearchParams();
  const search = searchParams.get("tq") || "";
  const query = trashQuery(search);
  const selectedId = selectedIdFromParam(searchParams.get("tsel"));

  const fetchCount = useCallback(
    async (signal: AbortSignal) => {
      const res = await listConversations({ q: query, limit: 1, offset: 0 }, { signal });
      return res.total ?? 0;
    },
    [query],
  );

  const { data, isPending: loading, error } = useVaultQuery(keys.trash.count(query), fetchCount);

  // AppLayout's left column sets `tsel` when a trashed conversation is clicked;
  // it stays on `/trash` rather than navigating to the thread, so this pane can
  // show Restore for the row the person just selected.
  const {
    data: selected,
    isPending: selectedLoading,
    error: selectedError,
  } = useVaultQuery(
    keys.conversations.detail(selectedId ?? 0),
    (signal) => getConversation(selectedId ?? 0, { signal }),
    { enabled: selectedId !== null },
  );

  const restoreConversation = useRestoreConversation();

  // The row leaves the trashed list once restored, so drop the selection along
  // with it rather than pointing at a conversation this pane can no longer show.
  const clearSelection = useCallback(() => {
    const next = new URLSearchParams(searchParams);
    next.delete("tsel");
    setSearchParams(next, { replace: true });
  }, [searchParams, setSearchParams]);

  if (loading) return <div className="p-6 text-[0.875rem] text-muted">Loading…</div>;

  const total = data ?? 0;
  const searching = search.trim().length > 0;

  return (
    <div className="max-w-[700px] p-6">
      <h2 className="m-0 mb-6">Trash</h2>
      {error && (
        <div className="mb-4 rounded border border-danger-soft-border bg-danger-soft-bg px-3 py-2 text-[0.813rem] text-danger">
          {apiErrorMessage(error, "Could not load Trash.")}
        </div>
      )}
      {selectedId !== null ? (
        selectedLoading ? (
          <div className="text-[0.875rem] text-muted">Loading…</div>
        ) : selected ? (
          <div className="rounded border border-border bg-elevated p-4">
            <div className="mb-1 text-[0.938rem] font-semibold text-text">
              {selected.label ||
                (selected.is_group
                  ? `${selected.participants.length} participants`
                  : selected.participants[0]?.name)}
            </div>
            <div className="mb-3 text-[0.75rem] text-muted">
              {selected.message_count} message{selected.message_count !== 1 ? "s" : ""}
            </div>
            {restoreConversation.error && (
              <div className="mb-3 rounded border border-danger-soft-border bg-danger-soft-bg px-3 py-2 text-[0.813rem] text-danger">
                {apiErrorMessage(restoreConversation.error, "Could not restore this conversation.")}
              </div>
            )}
            <div className="flex items-center gap-4">
              <Button
                variant="secondary"
                disabled={restoreConversation.isPending}
                onClick={() =>
                  restoreConversation.mutate(selectedId, { onSuccess: clearSelection })
                }
              >
                {restoreConversation.isPending ? "Restoring…" : "Restore"}
              </Button>
              <Link
                to={`/messages/${selectedId}`}
                className="text-[0.875rem] text-accent underline-offset-2 hover:underline"
              >
                View conversation
              </Link>
            </div>
          </div>
        ) : (
          <div className="text-[0.875rem] text-danger">
            {apiErrorMessage(selectedError, "Could not load this conversation.")}
          </div>
        )
      ) : total === 0 ? (
        <div className="text-[0.875rem] text-muted">
          {searching ? "No trashed conversations match this search." : "Trash is empty."}
        </div>
      ) : (
        <div className="text-[0.875rem] text-muted">
          {total} conversation{total !== 1 ? "s" : ""}
          {searching ? " matching this search" : ""} in Trash. Select one on the left to view it.
        </div>
      )}
    </div>
  );
}
