import { useCallback } from "react";
import { useSearchParams } from "react-router-dom";
import { apiErrorMessage } from "../lib/apiErrorMessage";
import { listConversations } from "../lib/vaultApi";
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
  return term ? `is:trash ${term}` : "is:trash";
}

export default function TrashScreen() {
  const [searchParams] = useSearchParams();
  const search = searchParams.get("tq") || "";
  const query = trashQuery(search);

  const fetchCount = useCallback(
    async (signal: AbortSignal) => {
      const res = await listConversations({ q: query, limit: 1, offset: 0 }, { signal });
      return res.total ?? 0;
    },
    [query],
  );

  const { data, isPending: loading, error } = useVaultQuery(["trash-count", query], fetchCount);

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
      {total === 0 ? (
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
