import { createContext, useContext, useState, useCallback, type ReactNode } from "react";
import { setBaseUrl, setToken } from "./api";

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

export function AuthProvider({ children }: { children: ReactNode }) {
  const [state, setState] = useState<AuthState>({
    serverUrl: "",
    token: null,
    accountId: null,
    isAuthenticated: false,
  });

  const setServer = useCallback((url: string) => {
    setBaseUrl(url);
    setState((s) => ({ ...s, serverUrl: url }));
  }, []);

  const login = useCallback((serverUrl: string, token: string, accountId: string) => {
    setBaseUrl(serverUrl);
    setToken(token);
    setState({
      serverUrl,
      token,
      accountId,
      isAuthenticated: true,
    });
  }, []);

  const logout = useCallback(() => {
    setToken(null);
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
