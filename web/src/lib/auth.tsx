import {
  createContext,
  useContext,
  useState,
  useCallback,
  useEffect,
  type ReactNode,
} from "react";
import { setBaseUrl, setToken, apiClient } from "./api";

interface AuthState {
  serverUrl: string;
  token: string | null;
  accountId: string | null;
  isAuthenticated: boolean;
}

interface AuthContextValue extends AuthState {
  login: (serverUrl: string, token: string, accountId: string) => void;
  logout: () => void;
  setServer: (url: string) => void;
}

const AuthContext = createContext<AuthContextValue | null>(null);

const STORAGE_KEY = "message-vault-auth";

function loadPersisted(): Partial<AuthState> | null {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return null;
    return JSON.parse(raw) as Partial<AuthState>;
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
  const [state, setState] = useState<AuthState>(() => {
    const persisted = loadPersisted();
    if (persisted?.serverUrl && persisted?.token && persisted?.accountId) {
      return {
        serverUrl: persisted.serverUrl,
        token: persisted.token,
        accountId: persisted.accountId,
        isAuthenticated: true,
      };
    }
    return {
      serverUrl: persisted?.serverUrl || "",
      token: null,
      accountId: null,
      isAuthenticated: false,
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
        if (!cancelled) setRestored(true);
      } catch {
        // Token invalid — clear and show login
        if (!cancelled) {
          setToken(null);
          clearPersisted();
          setState((s) => ({
            ...s,
            token: null,
            accountId: null,
            isAuthenticated: false,
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
    (serverUrl: string, token: string, accountId: string) => {
      setBaseUrl(serverUrl);
      setToken(token);
      const newState: AuthState = {
        serverUrl,
        token,
        accountId,
        isAuthenticated: true,
      };
      persistState(newState);
      setState(newState);
      setRestored(true);
    },
    [],
  );

  const logout = useCallback(() => {
    setToken(null);
    clearPersisted();
    setState((s) => ({
      ...s,
      token: null,
      accountId: null,
      isAuthenticated: false,
    }));
  }, []);

  return (
    <AuthContext.Provider value={{ ...state, login, logout, setServer }}>
      {children}
    </AuthContext.Provider>
  );
}

export function useAuth(): AuthContextValue {
  const ctx = useContext(AuthContext);
  if (!ctx) throw new Error("useAuth must be used within AuthProvider");
  return ctx;
}
