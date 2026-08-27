import { getCurrentWindow } from "@tauri-apps/api/window";
import {
  createContext,
  type ReactNode,
  useCallback,
  useContext,
  useEffect,
  useRef,
  useState,
} from "react";
import { apiClient, getToken, setBaseUrl, setToken } from "./api";
import { parsePersistedAuth } from "./authGuards";
import { clearContactDetailCache } from "./contactDetailCache";
import { invalidateContactGroups } from "./contactGroups";
import { isTauri } from "./tauri-check";
import { invalidateThreadTags } from "./threadTags";

interface AuthState {
  serverUrl: string;
  token: string | null;
  accountId: string | null;
  isAuthenticated: boolean;
  needsOnboarding: boolean;
}

interface Profile {
  preferred_name?: string | null;
  phones?: string[];
  emails?: string[];
}

/** True when the profile has no name, phone, or email yet — the user still needs setup. */
function profileNeedsOnboarding(profile: Profile): boolean {
  const hasName = !!profile.preferred_name?.trim();
  const hasPhone = (profile.phones?.length ?? 0) > 0;
  const hasEmail = (profile.emails?.length ?? 0) > 0;
  return !hasName && !hasPhone && !hasEmail;
}

interface AuthContextValue extends AuthState {
  login: (serverUrl: string, token: string, accountId: string) => Promise<void>;
  /** Save a new session token after the user changes their password. */
  updateToken: (token: string) => void;
  /** Revoke the vault session (best-effort) and clear the saved login. */
  logout: () => Promise<void>;
  setServer: (url: string) => void;
}

const AuthContext = createContext<AuthContextValue | null>(null);

const STORAGE_KEY = "message-vault-auth";

/** Max time to wait for the vault logout request before clearing local state. */
const LOGOUT_TIMEOUT_MS = 2000;

/** AbortSignal that fires after {@link LOGOUT_TIMEOUT_MS}. */
function logoutTimeoutSignal(): AbortSignal {
  if (typeof AbortSignal !== "undefined" && typeof AbortSignal.timeout === "function") {
    return AbortSignal.timeout(LOGOUT_TIMEOUT_MS);
  }
  const controller = new AbortController();
  setTimeout(() => controller.abort(), LOGOUT_TIMEOUT_MS);
  return controller.signal;
}

/** Read the last saved login from browser storage. */
function loadPersisted(): Partial<AuthState> | null {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return null;
    const parsed = parsePersistedAuth(raw);
    if (!parsed) return null;
    return {
      serverUrl: parsed.serverUrl,
      token: parsed.token,
      accountId: parsed.accountId,
      needsOnboarding: parsed.needsOnboarding,
    };
  } catch {
    return null;
  }
}

/** Write the current login to browser storage. Passwords are never stored. */
function persistState(state: AuthState) {
  try {
    localStorage.setItem(
      STORAGE_KEY,
      JSON.stringify({
        serverUrl: state.serverUrl,
        token: state.token,
        accountId: state.accountId,
        needsOnboarding: state.needsOnboarding,
      }),
    );
  } catch {
    // Full or blocked storage should not break login.
  }
}

/** Remove the saved login from browser storage. */
function clearPersisted() {
  try {
    localStorage.removeItem(STORAGE_KEY);
  } catch {
    // Full or blocked storage should not break logout.
  }
}

/** Holds login state for the app and restores a saved session on startup. */
export function AuthProvider({ children }: { children: ReactNode }) {
  const [restored, setRestored] = useState(false);
  // Incremented on login and logout so an older profile request is ignored.
  const authEpoch = useRef(0);
  const [state, setState] = useState<AuthState>(() => {
    const persisted = loadPersisted();
    // An empty server URL is allowed: it means "same host as this page".
    if (persisted?.token && persisted?.accountId && typeof persisted.serverUrl === "string") {
      // Apply before children mount. Otherwise Contact Groups loads without
      // a token, fails, and the sidebar stays on "No group" only.
      setBaseUrl(persisted.serverUrl);
      setToken(persisted.token);
      return {
        serverUrl: persisted.serverUrl,
        token: persisted.token,
        accountId: persisted.accountId,
        isAuthenticated: true,
        needsOnboarding: persisted.needsOnboarding ?? false,
      };
    }
    return {
      serverUrl: typeof persisted?.serverUrl === "string" ? persisted.serverUrl : "",
      token: null,
      accountId: null,
      isAuthenticated: false,
      needsOnboarding: false,
    };
  });

  useEffect(() => {
    setBaseUrl(state.serverUrl);
    setToken(state.token);
  }, [state.serverUrl, state.token]);

  // Check that the restored token still works.
  useEffect(() => {
    if (!state.isAuthenticated || restored) return;

    let cancelled = false;
    const validate = async () => {
      try {
        setBaseUrl(state.serverUrl);
        setToken(state.token);
        await apiClient.get("/v1/auth/check");
        if (cancelled) return;

        // Re-read the profile so a stale "needs setup" flag can correct itself.
        try {
          const profile = await apiClient.get<Profile>("/v1/account/profile");
          if (!cancelled) {
            const needsOnboarding = profileNeedsOnboarding(profile);
            setState((s) => {
              if (s.needsOnboarding === needsOnboarding) return s;
              const next: AuthState = { ...s, needsOnboarding };
              persistState(next);
              return next;
            });
          }
        } catch {
          // Profile request failed. Keep the saved "needs setup" flag.
        }

        if (!cancelled) setRestored(true);
      } catch {
        // Token is no longer valid. Clear it and show the login screen.
        if (!cancelled) {
          authEpoch.current++;
          setToken(null);
          clearPersisted();
          setState((s) => ({
            ...s,
            token: null,
            accountId: null,
            isAuthenticated: false,
            needsOnboarding: false,
          }));
          setRestored(true);
        }
      }
    };
    validate();
    return () => {
      cancelled = true;
    };
  }, [state.isAuthenticated, restored, state.serverUrl, state.token]);

  const setServer = useCallback((url: string) => {
    setBaseUrl(url);
    setState((s) => ({ ...s, serverUrl: url }));
  }, []);

  const login = useCallback(async (serverUrl: string, token: string, accountId: string) => {
    const epoch = ++authEpoch.current;
    clearContactDetailCache();
    invalidateContactGroups();
    invalidateThreadTags();
    setBaseUrl(serverUrl);
    setToken(token);

    // New accounts have no profile yet, so send them through setup.
    let needsOnboarding = false;
    try {
      const profile = await apiClient.get<Profile>("/v1/account/profile");
      needsOnboarding = profileNeedsOnboarding(profile);
    } catch {
      // Profile request failed. Assume a profile exists so the user is not locked out.
    }

    if (authEpoch.current !== epoch) return; // A later login or logout replaced this one.

    const newState: AuthState = {
      serverUrl,
      token,
      accountId,
      isAuthenticated: true,
      needsOnboarding,
    };
    persistState(newState);
    setState(newState);
    setRestored(true);
  }, []);

  const updateToken = useCallback((token: string) => {
    setToken(token);
    setState((s) => {
      if (!s.isAuthenticated) return s;
      const next: AuthState = { ...s, token };
      persistState(next);
      return next;
    });
  }, []);

  const logout = useCallback(async () => {
    authEpoch.current++;
    // Tell the server to end the session while the token is still set on the API client.
    // Await so close-to-quit can finish (or time out) before the WebView dies.
    if (getToken()) {
      try {
        await apiClient.post("/v1/auth/logout", {}, { signal: logoutTimeoutSignal() });
      } catch {
        // Vault unreachable, 401, or timeout — still clear the local session.
      }
    }
    setToken(null);
    clearContactDetailCache();
    invalidateContactGroups();
    invalidateThreadTags();
    clearPersisted();
    setState((s) => ({
      ...s,
      token: null,
      accountId: null,
      isAuthenticated: false,
      needsOnboarding: false,
    }));
  }, []);

  // Desktop only: on window close, revoke the session then quit.
  const closingRef = useRef(false);
  useEffect(() => {
    if (!isTauri()) return;

    let unlisten: (() => void) | undefined;
    let cancelled = false;

    void (async () => {
      try {
        const win = getCurrentWindow();
        unlisten = await win.onCloseRequested(async (event) => {
          event.preventDefault();
          if (closingRef.current) return;
          closingRef.current = true;
          try {
            await logout();
            await win.destroy();
          } catch {
            // Destroy failed or window already gone — allow another close attempt.
            closingRef.current = false;
          }
        });
        if (cancelled) {
          unlisten();
          unlisten = undefined;
        }
      } catch {
        // Missing window permissions or not a real Tauri window — leave close alone.
      }
    })();

    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [logout]);

  return (
    <AuthContext.Provider value={{ ...state, login, logout, updateToken, setServer }}>
      {children}
    </AuthContext.Provider>
  );
}

/** Current login state. Must be called under AuthProvider. */
export function useAuth(): AuthContextValue {
  const ctx = useContext(AuthContext);
  if (!ctx) throw new Error("useAuth must be used within AuthProvider");
  return ctx;
}
