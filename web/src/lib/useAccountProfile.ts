import { useCallback, useEffect, useSyncExternalStore } from "react";
import type { AccountProfile } from "./account";
import { apiClient } from "./api";

/**
 * One shared copy of `GET /v1/account/profile`.
 *
 * This used to be a `useResource` with a constant key, which shares nothing —
 * `useResource` holds its state per hook instance, so every mounted caller
 * issued its own request for the same endpoint. The state lives at module scope
 * instead, with concurrent callers joining the in-flight request, matching the
 * cache-and-dedupe shape already used by `contactDetailCache` and
 * `contactGroups`.
 */

type ProfileState = {
  profile: AccountProfile | null;
  loading: boolean;
  error: string;
};

/** Loading until the first request settles, so guards do not act on a null profile. */
let state: ProfileState = { profile: null, loading: true, error: "" };
let inflight: Promise<AccountProfile | null> | null = null;
let loaded = false;

const listeners = new Set<() => void>();

function setState(next: ProfileState): void {
  state = next;
  for (const listener of listeners) listener();
}

function subscribe(onStoreChange: () => void): () => void {
  listeners.add(onStoreChange);
  return () => {
    listeners.delete(onStoreChange);
  };
}

function getSnapshot(): ProfileState {
  return state;
}

/**
 * Fetch the profile, sharing one request across callers. Returns the cached
 * value without a request unless `force` is set.
 */
export function loadAccountProfile(force = false): Promise<AccountProfile | null> {
  if (inflight) return inflight;
  if (loaded && !force) return Promise.resolve(state.profile);

  setState({ ...state, loading: true, error: "" });
  inflight = apiClient
    .get<AccountProfile>("/v1/account/profile")
    .then((profile) => {
      loaded = true;
      setState({ profile, loading: false, error: "" });
      return profile;
    })
    .catch((e: unknown) => {
      loaded = true;
      setState({ profile: null, loading: false, error: String(e) });
      return null;
    })
    .finally(() => {
      inflight = null;
    });
  return inflight;
}

/** Apply a POST response without requesting the profile again. */
export function setAccountProfile(profile: AccountProfile | null): void {
  loaded = true;
  setState({ profile, loading: false, error: "" });
}

/** Drop the cached profile. Call on sign-in and sign-out so it cannot outlive a session. */
export function clearAccountProfile(): void {
  loaded = false;
  inflight = null;
  setState({ profile: null, loading: true, error: "" });
}

export function useAccountProfile(): {
  profile: AccountProfile | null;
  setProfile: (profile: AccountProfile | null) => void;
  loading: boolean;
  error: string;
  reload: () => void;
} {
  const snapshot = useSyncExternalStore(subscribe, getSnapshot, getSnapshot);

  useEffect(() => {
    void loadAccountProfile();
  }, []);

  const reload = useCallback(() => {
    void loadAccountProfile(true);
  }, []);

  return {
    profile: snapshot.profile,
    setProfile: setAccountProfile,
    loading: snapshot.loading,
    error: snapshot.error,
    reload,
  };
}
