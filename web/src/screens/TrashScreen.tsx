import { useCallback } from "react";
import { Link, useSearchParams } from "react-router-dom";
import Button from "../components/Button";
import { apiErrorMessage } from "../lib/apiErrorMessage";
import { trashed } from "../lib/searchQuery";
import { useRestoreContact, useRestoreConversation } from "../lib/trash";
import { getConversation, listContacts, listConversations } from "../lib/vaultApi";
import { keys } from "../lib/vaultKeys";
import { useVaultQuery } from "../lib/vaultQuery";

/**
 * Trash holds two kinds of thing, and this pane is where both come back.
 *
 * Trashed conversations are listed in the left column by the shared
 * conversation list, so this pane only reports how many there are and offers
 * Restore for the one the person selected. Trashed contacts have no left
 * column of their own — and a trashed contact cannot be opened at all, since
 * `GET /v1/contacts/{id}` filters them out — so they are listed here in full,
 * each row carrying its own Restore. That is why restoring a contact lives on
 * the row rather than in the contact drawer.
 *
 * Both lists read the same header search term, so narrowing Trash narrows both
 * kinds at once.
 */

/** How many trashed contacts this pane lists before it stops. */
const CONTACT_LIMIT = 100;

/** The `tsel` param as a positive conversation id, or null when absent or malformed. */
function selectedIdFromParam(raw: string | null): number | null {
  if (raw === null || !/^\d+$/.test(raw)) return null;
  const n = Number(raw);
  return Number.isSafeInteger(n) && n > 0 ? n : null;
}

const sectionHeading =
  "m-0 mb-2 text-[0.75rem] font-semibold uppercase tracking-[0.04em] text-muted";

const errorBox =
  "mb-3 rounded border border-danger-soft-border bg-danger-soft-bg px-3 py-2 text-[0.813rem] text-danger";

export default function TrashScreen() {
  const [searchParams, setSearchParams] = useSearchParams();
  const search = searchParams.get("tq") || "";
  const query = trashed(search);
  const selectedId = selectedIdFromParam(searchParams.get("tsel"));

  // Only `total` is read here; the rows themselves are rendered by the list
  // column, so one row is enough to read `total` off the page response.
  const fetchCount = useCallback(
    async (signal: AbortSignal) => {
      const res = await listConversations({ q: query, limit: 1, offset: 0 }, { signal });
      return res.total ?? 0;
    },
    [query],
  );

  const { data, isPending: loading, error } = useVaultQuery(keys.trash.count(query), fetchCount);

  const {
    data: contactPage,
    isPending: contactsLoading,
    error: contactsError,
  } = useVaultQuery(keys.contacts.trashed(query), (signal) =>
    listContacts({ q: query, limit: CONTACT_LIMIT, offset: 0 }, { signal }),
  );

  // AppLayout's left column sets `tsel` when a trashed conversation is clicked;
  // it stays on `/trash` rather than navigating to the thread, so this pane can
  // show Restore for the row the person just selected.
  const {
    data: selected,
    isPending: selectedLoading,
    error: selectedError,
  } = useVaultQuery(
    selectedId === null ? keys.trash.noSelection : keys.conversations.detail(selectedId),
    (signal) => getConversation(selectedId ?? 0, { signal }),
    { enabled: selectedId !== null },
  );

  const restoreConversation = useRestoreConversation();
  const restoreContact = useRestoreContact();

  // The row leaves the trashed list once restored, so drop the selection along
  // with it rather than pointing at a conversation this pane can no longer show.
  const clearSelection = useCallback(() => {
    const next = new URLSearchParams(searchParams);
    next.delete("tsel");
    setSearchParams(next, { replace: true });
  }, [searchParams, setSearchParams]);

  if (loading || contactsLoading)
    return <div className="p-6 text-[0.875rem] text-muted">Loading…</div>;

  const total = data ?? 0;
  const contacts = contactPage?.items ?? [];
  const searching = search.trim().length > 0;
  const nothingInTrash = total === 0 && contacts.length === 0 && selectedId === null;

  return (
    <div className="max-w-[700px] p-6">
      <h2 className="m-0 mb-6">Trash</h2>
      {error && <div className={errorBox}>{apiErrorMessage(error, "Could not load Trash.")}</div>}
      {nothingInTrash ? (
        <div className="text-[0.875rem] text-muted">
          {searching ? "Nothing in Trash matches this search." : "Trash is empty."}
        </div>
      ) : (
        <>
          <section className="mb-8">
            <h3 className={sectionHeading}>Conversations</h3>
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
                    <div className={errorBox}>
                      {apiErrorMessage(
                        restoreConversation.error,
                        "Could not restore this conversation.",
                      )}
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
                {searching ? "No conversations match this search." : "No conversations in Trash."}
              </div>
            ) : (
              <div className="text-[0.875rem] text-muted">
                {total} conversation{total !== 1 ? "s" : ""}
                {searching ? " matching this search" : ""} in Trash. Select one on the left to view
                it.
              </div>
            )}
          </section>

          <section>
            <h3 className={sectionHeading}>Contacts</h3>
            {contactsError && (
              <div className={errorBox}>
                {apiErrorMessage(contactsError, "Could not load trashed contacts.")}
              </div>
            )}
            {restoreContact.error && (
              <div className={errorBox}>
                {apiErrorMessage(restoreContact.error, "Could not restore this contact.")}
              </div>
            )}
            {contacts.length === 0 ? (
              <div className="text-[0.875rem] text-muted">
                {searching ? "No contacts match this search." : "No contacts in Trash."}
              </div>
            ) : (
              <ul className="m-0 list-none rounded border border-border bg-elevated p-0">
                {contacts.map((contact) => {
                  const restoring =
                    restoreContact.isPending && restoreContact.variables === contact.id;
                  return (
                    <li
                      key={contact.id}
                      className="flex items-center justify-between gap-4 border-0 border-b border-solid border-border px-4 py-3 last:border-b-0"
                    >
                      <div className="min-w-0">
                        <div className="truncate text-[0.875rem] text-text">{contact.name}</div>
                        <div className="text-[0.75rem] text-muted">
                          {contact.handle_count} handle{contact.handle_count !== 1 ? "s" : ""}
                        </div>
                      </div>
                      <Button
                        variant="secondary"
                        size="sm"
                        // Every row's button reads "Restore", so the name it
                        // answers to says which contact it restores.
                        aria-label={`Restore ${contact.name}`}
                        disabled={restoreContact.isPending}
                        onClick={() => restoreContact.mutate(contact.id)}
                      >
                        {restoring ? "Restoring…" : "Restore"}
                      </Button>
                    </li>
                  );
                })}
              </ul>
            )}
          </section>
        </>
      )}
    </div>
  );
}
