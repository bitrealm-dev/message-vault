import {
  createContext,
  useContext,
  useState,
  useCallback,
  useEffect,
  useRef,
  type ReactNode,
} from "react";
import { setBaseUrl, setToken, apiClient } from "./api";
import { clearContactDetailCache } from "./contactDetailCache";
import { parsePersistedAuth } from "./authGuards";

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

function profileNeedsOnboarding(profile: Profile): boolean {
  const hasName = !!profile.preferred_name?.trim();
  const hasPhone = (profile.phones?.length ?? 0) > 0;
  const hasEmail = (profile.emails?.length ?? 0) > 0;
  return !hasName && !hasPhone && !hasEmail;
}

interface AuthContextValue extends AuthState {
  login: (serverUrl: string, token: string, accountId: string) => Promise<void>;
  /** Persist a rotated session token (e.g. after change-password). */
  updateToken: (token: string) => void;
  logout: () => void;
  setServer: (url: string) => void;
}

const AuthContext = createContext<AuthContextValue | null>(null);

const STORAGE_KEY = "message-vault-auth";

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
    // Storage full or unavailable — not critical
  }
}

function clearPersisted() {
  try {
    localStorage.removeItem(STORAGE_KEY);
  } catch {
    // ignore
  }
}

export function AuthProvider({ children }: { children: ReactNode }) {
  const [restored, setRestored] = useState(false);
  // Bumped on every login/logout so stale async profile checks are discarded
  const authEpoch = useRef(0);
  const [state, setState] = useState<AuthState>(() => {
    const persisted = loadPersisted();
    // Allow empty serverUrl (same-origin) when token + accountId are present.
    if (
      persisted?.token &&
      persisted?.accountId &&
      typeof persisted.serverUrl === "string"
    ) {
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

  // Validate restored token on mount
  useEffect(() => {
    if (!state.isAuthenticated || restored) return;

    let cancelled = false;
    const validate = async () => {
      try {
        setBaseUrl(state.serverUrl);
        setToken(state.token);
        await apiClient.get("/v1/auth/check");
        if (cancelled) return;

        // Refresh onboarding need from the profile — self-heals a stale flag
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
          // Profile fetch failed — keep the persisted flag
        }

        if (!cancelled) setRestored(true);
      } catch {
        // Token invalid — clear and show login
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

  const login = useCallback(
    async (serverUrl: string, token: string, accountId: string) => {
      const epoch = ++authEpoch.current;
      clearContactDetailCache();
      setBaseUrl(serverUrl);
      setToken(token);

      // New accounts have no profile yet — flag them for onboarding
      let needsOnboarding = false;
      try {
        const profile = await apiClient.get<Profile>("/v1/account/profile");
        needsOnboarding = profileNeedsOnboarding(profile);
      } catch {
        // Profile check failed — assume a profile exists so access is never blocked
      }

      if (authEpoch.current !== epoch) return; // superseded by logout/login

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
    },
    [],
  );

  const updateToken = useCallback((token: string) => {
    setToken(token);
    setState((s) => {
      if (!s.isAuthenticated) return s;
      const next: AuthState = { ...s, token };
      persistState(next);
      return next;
    });
  }, []);

  const logout = useCallback(() => {
    authEpoch.current++;
    // Revoke while the bearer token is still configured on the API client.
    void apiClient.post("/v1/auth/logout", {}).catch(() => {});
    setToken(null);
    clearContactDetailCache();
    clearPersisted();
    setState((s) => ({
      ...s,
      token: null,
      accountId: null,
      isAuthenticated: false,
      needsOnboarding: false,
    }));
  }, []);

  return (
    <AuthContext.Provider value={{ ...state, login, logout, updateToken, setServer }}>
      {children}
    </AuthContext.Provider>
  );
}

export function useAuth(): AuthContextValue {
  const ctx = useContext(AuthContext);
  if (!ctx) throw new Error("useAuth must be used within AuthProvider");
  return ctx;
}
