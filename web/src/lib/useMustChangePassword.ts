import { useAccountProfile } from "./useAccountProfile";

/**
 * Whether this account still carries the password the vault owner chose for it.
 *
 * A server fact, read from the profile, not a guess made in the browser and
 * cached there. `loading` matters to the caller: a guard that treated "not
 * loaded yet" as "nothing owed" would let the account through for one render
 * and then yank it back.
 */
export function useMustChangePassword(): { mustChange: boolean; loading: boolean } {
  const { profile, loading } = useAccountProfile();
  return { mustChange: profile?.must_change_password === true, loading };
}
