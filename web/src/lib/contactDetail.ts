import { useQueryClient } from "@tanstack/react-query";
import { useCallback, useMemo } from "react";
import { useAuth } from "./auth";
import { getContact } from "./vaultApi";
import type { components } from "./vaultApi.types";
import { useVaultQuery } from "./vaultQuery";
import { ANONYMOUS_ACCOUNT, vaultQueryKey } from "./vaultQueryKey";

/**
 * One contact in full, as the contact drawer shows it.
 *
 * This replaces `contactDetailCache`, which held a `Map`, its own in-flight
 * guard, and a `mv-contact-detail-changed` browser event that the drawer
 * subscribed to so that group chips edited in the contact list would show. All
 * three are TanStack Query's now: the cache is keyed by account and contact,
 * and a group change is a write to that entry, which re-renders whoever reads
 * it.
 */

export type ContactDetail = components["schemas"]["ContactDetail"];
export type ContactHandle = components["schemas"]["ContactHandleInfo"];

/** Cache key parts for one contact, before the account is put in front. */
export function contactDetailKey(id: string | number): readonly unknown[] {
  return ["contact-detail", String(id)];
}

/** The contact behind an open drawer. Skipped entirely when no contact is open. */
export function useContactDetail(contactId: string | null): {
  detail: ContactDetail | null;
  loading: boolean;
} {
  const { data, isPending } = useVaultQuery(
    contactDetailKey(contactId ?? ""),
    (signal) => getContact(contactId ?? "", { signal }),
    { enabled: contactId !== null },
  );
  return { detail: contactId ? (data ?? null) : null, loading: isPending };
}

/**
 * Read and edit cached contacts outside a query.
 *
 * The contact list needs both: it reads a contact's groups while rendering a
 * row, and it writes new groups onto contacts the person just re-grouped so the
 * chips change before the vault has answered.
 */
export function useContactDetailCache(): {
  read: (id: string | number) => ContactDetail | null;
  setGroups: (id: string | number, groups: string[]) => void;
  invalidate: (id: string | number) => void;
} {
  const client = useQueryClient();
  const { accountId } = useAuth();
  const account = accountId ?? ANONYMOUS_ACCOUNT;

  const read = useCallback(
    (id: string | number) =>
      client.getQueryData<ContactDetail>(vaultQueryKey(account, contactDetailKey(id))) ?? null,
    [client, account],
  );

  const setGroups = useCallback(
    (id: string | number, groups: string[]) => {
      client.setQueryData<ContactDetail>(vaultQueryKey(account, contactDetailKey(id)), (current) =>
        current ? { ...current, groups } : current,
      );
    },
    [client, account],
  );

  const invalidate = useCallback(
    (id: string | number) => {
      void client.invalidateQueries({ queryKey: vaultQueryKey(account, contactDetailKey(id)) });
    },
    [client, account],
  );

  return useMemo(() => ({ read, setGroups, invalidate }), [read, setGroups, invalidate]);
}
