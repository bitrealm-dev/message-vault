import { type UseMutationResult, useMutation } from "@tanstack/react-query";
import {
  restoreContact as restoreVaultContact,
  restoreConversation as restoreVaultConversation,
  trashContact as trashVaultContact,
  trashConversation as trashVaultConversation,
} from "./vaultApi";
import { keys } from "./vaultKeys";
import { useVaultCache } from "./vaultQuery";

/**
 * Trash is a soft marker on two different nouns — conversations and contacts
 * — set and cleared by four idempotent routes. Permanent delete and Empty
 * Trash are separate and unbuilt (issue #314); nothing here destroys
 * anything.
 *
 * Unlike Contact Groups and Message Tags this is not a `nameCollection`:
 * there is no name, no membership, and nothing to look an id up by — the
 * caller already has the conversation or contact id. Each pair is a plain
 * mutation over one vault route, with `onSettled` marking the prefixes the
 * trash state actually touches.
 */

/** Every list and count that shows conversation trash state. */
function useConversationTrashWrite(
  write: (id: number) => Promise<void>,
): UseMutationResult<void, Error, number> {
  const cache = useVaultCache();
  return useMutation<void, Error, number>({
    mutationFn: write,
    // - conversations.lists: the row leaves or rejoins the (non-)trashed list.
    // - trash.all: the Trash screen's count is `listConversations` under its
    //   own key, not a child of conversations.lists, so it needs naming here
    //   too.
    // - contacts.details: an open contact's per-handle conversation and
    //   message counts exclude trashed conversations (see
    //   `crates/vault/server/src/db/participant_names.rs` and
    //   `get_contact_detail`'s comment), and the 204 response names no
    //   participant to narrow this to, so every open detail is marked.
    // Nothing under conversations beyond the list needs marking: GET
    // /v1/conversations/{id} answers the same ConversationSummary whether or
    // not it is trashed ("trash is a property the list applies, not a gate
    // on reading"), and trashing a conversation does not touch its own
    // messages. contacts.lists is also left alone: ContactSummary (name,
    // handles, groups) carries no conversation or message counts.
    onSettled: () =>
      cache.invalidate(
        keys.conversations.lists,
        keys.conversations.finds,
        keys.trash.all,
        keys.contacts.details,
      ),
  });
}

export function useTrashConversation(): UseMutationResult<void, Error, number> {
  return useConversationTrashWrite(trashVaultConversation);
}

export function useRestoreConversation(): UseMutationResult<void, Error, number> {
  return useConversationTrashWrite(restoreVaultConversation);
}

/** Every list and detail that shows contact trash state. */
function useContactTrashWrite(
  write: (id: string | number) => Promise<void>,
): UseMutationResult<void, Error, string | number> {
  const cache = useVaultCache();
  return useMutation<void, Error, string | number>({
    mutationFn: write,
    // - contacts.lists: the row leaves or rejoins the contacts list.
    // - contacts.detail(id): unlike a trashed conversation, GET
    //   /v1/contacts/{id} is itself gated by trash — a trashed contact 404s
    //   — so an open drawer for exactly this contact goes stale. The id is
    //   known here, so this is targeted rather than the whole
    //   contacts.details prefix.
    // Not invalidated: anything under conversations, or trash.all. Trashing
    // a contact never touches the trashed_conversations table, and
    // participant-name resolution does not filter on a contact's trash
    // state, so no conversation row, detail, or message changes. trash.all
    // only backs the Trash screen's conversation count (`listConversations`
    // with `trashed:yes`); nothing today counts trashed contacts under it.
    onSettled: (_data, _error, id) =>
      cache.invalidate(keys.contacts.lists, keys.contacts.detail(id)),
  });
}

export function useTrashContact(): UseMutationResult<void, Error, string | number> {
  return useContactTrashWrite(trashVaultContact);
}

export function useRestoreContact(): UseMutationResult<void, Error, string | number> {
  return useContactTrashWrite(restoreVaultContact);
}
