import { useCallback, useState } from "react";
import type { AccountProfile } from "./account";
import { apiClient } from "./api";
import { useResource } from "./useResource";

/**
 * Load `GET /v1/account/profile` once and share it across settings screens.
 * `setProfile` applies a POST response without requesting the profile again.
 */
export function useAccountProfile(): {
  profile: AccountProfile | null;
  setProfile: (profile: AccountProfile | null) => void;
  loading: boolean;
  error: string;
  reload: () => void;
} {
  const fetchProfile = useCallback(
    (signal: AbortSignal) => apiClient.get<AccountProfile>("/v1/account/profile", { signal }),
    [],
  );

  const { data, loading, error, reload } = useResource("account/profile", fetchProfile);
  const [override, setOverride] = useState<AccountProfile | null | undefined>(undefined);

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
