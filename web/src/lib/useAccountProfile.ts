import { useCallback, useState } from "react";
import { apiClient } from "./api";
import type { AccountProfile } from "./account";
import { useResource } from "./useResource";

/**
 * Shared loader for `GET /v1/account/profile`.
 * `setProfile` lets settings panels apply POST responses without a second fetch.
 */
export function useAccountProfile(): {
  profile: AccountProfile | null;
  setProfile: (profile: AccountProfile | null) => void;
  loading: boolean;
  error: string;
  reload: () => void;
} {
  const fetchProfile = useCallback(
    (signal: AbortSignal) =>
      apiClient.get<AccountProfile>("/v1/account/profile", { signal }),
    [],
  );

  const { data, loading, error, reload } = useResource(
    "account/profile",
    fetchProfile,
  );
  const [override, setOverride] = useState<AccountProfile | null | undefined>(
    undefined,
  );

  const setProfile = useCallback((profile: AccountProfile | null) => {
    setOverride(profile);
  }, []);

  return {
    profile: override !== undefined ? override : data,
    setProfile,
    loading: override !== undefined ? false : loading,
    error: override !== undefined ? "" : error,
    reload: () => {
      setOverride(undefined);
      reload();
    },
  };
}
