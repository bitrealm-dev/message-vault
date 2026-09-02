import { useQueryClient } from "@tanstack/react-query";
import { useCallback } from "react";
import type { AccountProfile } from "./account";
import { useAuth } from "./auth";
import { getAccountProfile } from "./vaultApi";
import { useVaultQuery, useVaultSetCached } from "./vaultQuery";
import { ANONYMOUS_ACCOUNT, vaultQueryKey } from "./vaultQueryKey";
import { keys } from "./vaultKeys";

/**
 * The signed-in account's profile.
 *
 * This used to be a module-level store with its own in-flight guard, its own
 * subscriber list, and a `clearAccountProfile` that `auth.tsx` had to remember
 * to call on both sign-in and sign-out. All of that is TanStack Query's now,
 * and the entry is named with the account, so nothing has to be cleared for
 * one account to stop seeing another's profile.
 */

export function useAccountProfile(): {
  profile: AccountProfile | null;
  setProfile: (profile: AccountProfile | null) => void;
  loading: boolean;
  error: string;
  reload: () => void;
} {
  const setCached = useVaultSetCached();
  const { data, isPending, error, refetch } = useVaultQuery(keys.accountProfile.all, (signal) =>
    getAccountProfile({ signal }),
  );

  const setProfile = useCallback(
    (profile: AccountProfile | null) => {
      setCached(keys.accountProfile.all, profile);
    },
    [setCached],
  );

  const reload = useCallback(() => {
    void refetch();
  }, [refetch]);

  return {
    profile: data ?? null,
    setProfile,
    loading: isPending,
    error: error ? error.message : "",
    reload,
  };
}

/**
 * Read the profile outside a render — during sign-in, and before an import
 * decides whether the backup belongs to this person.
 *
 * `accountId` is passed explicitly because sign-in knows the account before the
 * auth state carries it, and the entry has to land under the key the hook above
 * will read.
 */
export function fetchAccountProfileFor(
  client: ReturnType<typeof useQueryClient>,
  accountId: string | null,
  force = false,
): Promise<AccountProfile | null> {
  const key = vaultQueryKey(accountId ?? ANONYMOUS_ACCOUNT, keys.accountProfile.all);
  if (force) client.removeQueries({ queryKey: key });
  return client.fetchQuery({ queryKey: key, queryFn: () => getAccountProfile() }).catch(() => null);
}

/** The same, for a caller that is already inside the signed-in tree. */
export function useFetchAccountProfile(): (force?: boolean) => Promise<AccountProfile | null> {
  const client = useQueryClient();
  const { accountId } = useAuth();
  return useCallback(
    (force = false) => fetchAccountProfileFor(client, accountId, force),
    [client, accountId],
  );
}
