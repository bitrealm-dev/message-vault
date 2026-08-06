# Unified GUI — Plan 2: App Shell and Navigation

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the layout shell, routing, auth layer, and API client that every screen plugs into. After this plan the app boots to a login screen, authenticates against the vault server, and renders an empty layout with a left panel and main content area.

**Architecture:** React Router for navigation (`/login`, `/`, `/import`, `/export`, `/settings`). Auth state held in React context — stores server URL, token, and account info. API client module wraps `fetch()` with automatic auth header injection. Left panel and main content area are persistent layout components. `isTauri()` gates desktop-only UI elements.

**Tech Stack:** React 19, React Router v7, TypeScript, Vite (existing stack in `web/`)

## Global Constraints

- All screen components go under `web/src/screens/`
- Shared components under `web/src/components/`
- API client under `web/src/lib/api.ts` (new file alongside `tauri.ts`)
- Auth context under `web/src/lib/auth.tsx` (new file)
- Existing Tauri screens (Extract, Format, etc.) are preserved until replaced by new import/export screens
- Desktop-only elements use `isTauri()` from `@tauri-apps/api/core` or a window property check
- The web deployment has no Tauri APIs available — `isTauri()` returns false

---

## File Structure

| File | Responsibility |
|------|---------------|
| `web/src/App.tsx` | Router setup, auth guard, layout |
| `web/src/components/AppLayout.tsx` | Left panel + main area shell |
| `web/src/components/LeftPanel.tsx` | Global search, saved groups, nav links, Import/Export buttons, Profile/Settings |
| `web/src/lib/auth.tsx` | AuthContext, AuthProvider, useAuth hook |
| `web/src/lib/api.ts` | Typed fetch() wrapper, base URL + auth header management |
| `web/src/screens/LoginScreen.tsx` | Server URL input, auth mode detection, login form |

---

### Task 1: API client module

**Files:**
- Create: `web/src/lib/api.ts`

**Interfaces:**
- Produces: `apiClient` object with `get<T>()`, `post<T>()`, `setBaseUrl()`, `setToken()`
- Consumes: none (standalone module)

- [ ] **Step 1: Write the API client**

```typescript
// web/src/lib/api.ts

let baseUrl = "";
let authToken: string | null = null;

export function setBaseUrl(url: string) {
  // Strip trailing slash
  baseUrl = url.replace(/\/+$/, "");
}

export function setToken(token: string | null) {
  authToken = token;
}

export function getBaseUrl(): string {
  return baseUrl;
}

async function request<T>(
  method: string,
  path: string,
  body?: unknown,
): Promise<T> {
  if (!baseUrl) throw new Error("Server URL not configured");

  const headers: Record<string, string> = {
    "Content-Type": "application/json",
  };
  if (authToken) {
    headers["Authorization"] = `Bearer ${authToken}`;
  }

  const res = await fetch(`${baseUrl}${path}`, {
    method,
    headers,
    body: body ? JSON.stringify(body) : undefined,
  });

  if (!res.ok) {
    const text = await res.text();
    throw new Error(`${res.status}: ${text}`);
  }

  return res.json() as Promise<T>;
}

export const apiClient = {
  get<T>(path: string): Promise<T> {
    return request<T>("GET", path);
  },
  post<T>(path: string, body?: unknown): Promise<T> {
    return request<T>("POST", path, body);
  },
};
```

- [ ] **Step 2: Build frontend**

```bash
cd web && npm run build
```

Expected: compiles cleanly, no errors.

- [ ] **Step 3: Commit**

```bash
git add web/src/lib/api.ts
git commit -m "feat(web): add API client module

Typed fetch() wrapper with automatic auth header injection.
Supports GET/POST with JSON serialization.

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

### Task 2: Auth context

**Files:**
- Create: `web/src/lib/auth.tsx`

**Interfaces:**
- Produces: `AuthProvider` component, `useAuth()` hook returning `{ serverUrl, token, accountId, isAuthenticated, login(), logout() }`
- Consumes: `api.ts` (setBaseUrl, setToken, apiClient)

- [ ] **Step 1: Write the auth context**

```typescript
// web/src/lib/auth.tsx

import { createContext, useContext, useState, useCallback, type ReactNode } from "react";
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
    setState({
      serverUrl: state.serverUrl,
      token: null,
      accountId: null,
      isAuthenticated: false,
    });
  }, [state.serverUrl]);

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
```

- [ ] **Step 2: Build and verify**

```bash
cd web && npm run build
```

Expected: compiles cleanly.

- [ ] **Step 3: Commit**

```bash
git add web/src/lib/auth.tsx
git commit -m "feat(web): add auth context provider

AuthProvider wraps the app, stores server URL + token + accountId.
useAuth hook exposes login/logout/setServer. Integrates with api.ts.

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

### Task 3: Login screen

**Files:**
- Create: `web/src/screens/LoginScreen.tsx`
- Create: `web/src/lib/tauri-check.ts`

**Interfaces:**
- Produces: `LoginScreen` component — full-page login with server URL, auth mode detection, login form
- Consumes: `useAuth()`, `apiClient`, `isTauri()`

- [ ] **Step 1: Create isTauri helper**

```typescript
// web/src/lib/tauri-check.ts

/** True when running inside the Tauri desktop app. */
export function isTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}
```

This avoids importing the full `@tauri-apps/api` package in the web deployment where it doesn't exist.

- [ ] **Step 2: Write LoginScreen**

```typescript
// web/src/screens/LoginScreen.tsx

import { useState, useEffect } from "react";
import { useAuth } from "../lib/auth";
import { apiClient, setBaseUrl } from "../lib/api";
import { isTauri } from "../lib/tauri-check";

type AuthMode = "hanko" | "local" | null;

export default function LoginScreen() {
  const { login, setServer: setAuthServer } = useAuth();
  const [serverUrl, setServerUrl] = useState("http://localhost:5556");
  const [authMode, setAuthMode] = useState<AuthMode>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");

  // Local auth fields
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");

  // Detect auth mode when server URL is entered
  const detectMode = async () => {
    if (!serverUrl.trim()) return;
    setLoading(true);
    setError("");
    try {
      setBaseUrl(serverUrl);
      const res = await apiClient.get<{ mode: string }>("/v1/auth/mode");
      setAuthMode(res.mode as AuthMode);
      setAuthServer(serverUrl);
    } catch {
      setError("Could not reach server. Check the URL and try again.");
    } finally {
      setLoading(false);
    }
  };

  const handleLocalLogin = async () => {
    setLoading(true);
    setError("");
    try {
      const res = await apiClient.post<{ token: string; account_id: string }>(
        "/v1/auth/login",
        { username, password },
      );
      login(serverUrl, res.token, res.account_id);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  };

  return (
    <div style={{
      display: "flex", alignItems: "center", justifyContent: "center",
      minHeight: "100vh", background: "#f3f4f6", fontFamily: "system-ui",
    }}>
      <div style={{
        background: "#fff", padding: "2rem", borderRadius: "8px",
        width: "100%", maxWidth: "400px", boxShadow: "0 1px 3px rgba(0,0,0,0.1)",
      }}>
        <h1 style={{ margin: "0 0 1.5rem", fontSize: "1.5rem", textAlign: "center" }}>
          Message Vault
        </h1>

        <label style={{ fontSize: "0.875rem", fontWeight: 500, display: "block", marginBottom: "0.25rem" }}>
          Server URL
        </label>
        <div style={{ display: "flex", gap: "0.5rem", marginBottom: "1rem" }}>
          <input
            type="text"
            value={serverUrl}
            onChange={(e) => setServerUrl(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && detectMode()}
            placeholder="https://vault.example.com"
            style={{ flex: 1, padding: "0.5rem", fontSize: "0.875rem", border: "1px solid #d1d5db", borderRadius: "4px" }}
          />
          <button
            onClick={detectMode}
            disabled={loading}
            style={{ padding: "0.5rem 1rem", fontSize: "0.875rem", fontWeight: 600 }}
          >
            Connect
          </button>
        </div>

        {error && (
          <div style={{ padding: "0.5rem 0.75rem", background: "#fef2f2", border: "1px solid #fecaca", borderRadius: "4px", color: "#991b1b", fontSize: "0.813rem", marginBottom: "1rem" }}>
            {error}
          </div>
        )}

        {authMode === "local" && (
          <>
            <label style={{ fontSize: "0.875rem", fontWeight: 500, display: "block", marginBottom: "0.25rem" }}>Username</label>
            <input type="text" value={username} onChange={(e) => setUsername(e.target.value)}
              style={{ width: "100%", padding: "0.5rem", fontSize: "0.875rem", border: "1px solid #d1d5db", borderRadius: "4px", marginBottom: "0.75rem" }} />

            <label style={{ fontSize: "0.875rem", fontWeight: 500, display: "block", marginBottom: "0.25rem" }}>Password</label>
            <input type="password" value={password} onChange={(e) => setPassword(e.target.value)}
              style={{ width: "100%", padding: "0.5rem", fontSize: "0.875rem", border: "1px solid #d1d5db", borderRadius: "4px", marginBottom: "1rem" }} />

            <button onClick={handleLocalLogin} disabled={loading || !username || !password}
              style={{ width: "100%", padding: "0.75rem", fontSize: "1rem", fontWeight: 600 }}>
              {loading ? "Signing in…" : "Sign in"}
            </button>
          </>
        )}

        {authMode === "hanko" && (
          <div style={{ textAlign: "center", padding: "1rem", color: "#6b7280", fontSize: "0.875rem" }}>
            Hanko passkey login will be implemented here.
            {/* Hanko integration: render <hanko-elements> web component */}
          </div>
        )}

        {/* Offline tools — Tauri only */}
        {isTauri() && (
          <>
            <hr style={{ margin: "1.5rem 0", border: "none", borderTop: "1px solid #e5e7eb" }} />
            <p style={{ fontSize: "0.813rem", color: "#6b7280", textAlign: "center", marginBottom: "0.75rem" }}>
              No vault? Use offline tools instead.
            </p>
            <div style={{ display: "flex", gap: "0.75rem" }}>
              <button style={{ flex: 1, padding: "0.5rem", fontSize: "0.875rem" }}>
                Extract messages
              </button>
              <button style={{ flex: 1, padding: "0.5rem", fontSize: "0.875rem" }}>
                Format conversion
              </button>
            </div>
          </>
        )}
      </div>
    </div>
  );
}
```

> **Note:** The local auth login endpoint (`/v1/auth/login`) and Hanko integration are placeholder implementations. The actual authentication API in message-vault-rs uses per-account Import API tokens (Bearer auth). The login flow for the browser-based SPA needs a session-based or token-exchange mechanism. This task implements the UI with the correct call sites; the actual auth flow is a separate integration task.

- [ ] **Step 3: Build and verify**

```bash
cd web && npm run build
```

Expected: compiles cleanly.

- [ ] **Step 4: Commit**

```bash
git add web/src/screens/LoginScreen.tsx web/src/lib/tauri-check.ts
git commit -m "feat(web): add login screen with auth mode detection

Calls GET /v1/auth/mode to discover Hanko vs local auth.
Offline tools (Extract, Format) shown in Tauri only.
isTauri() helper without @tauri-apps/api import.

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

### Task 4: Left panel component

**Files:**
- Create: `web/src/components/LeftPanel.tsx`
- Modify: `web/src/components/FormRow.tsx` (no changes — reference only for styling)

**Interfaces:**
- Produces: `LeftPanel` component — global search, saved groups placeholder, nav links, Import/Export buttons, Profile/Settings
- Consumes: `useAuth()`, `isTauri()`

- [ ] **Step 1: Write LeftPanel**

```typescript
// web/src/components/LeftPanel.tsx

import { useAuth } from "../lib/auth";
import { isTauri } from "../lib/tauri-check";

export default function LeftPanel({ activeView, onNavigate }: {
  activeView: string;
  onNavigate: (view: string) => void;
}) {
  const { logout } = useAuth();

  const linkStyle = (view: string) => ({
    padding: "0.375rem 0.75rem",
    fontSize: "0.875rem",
    cursor: "pointer",
    borderRadius: "4px",
    background: activeView === view ? "#e5e7eb" : "transparent",
    fontWeight: activeView === view ? 600 : 400,
    border: "none",
    textAlign: "left" as const,
    width: "100%",
    display: "block",
    color: "#1f2937",
  });

  return (
    <div style={{
      width: "220px", flexShrink: 0, borderRight: "1px solid #e5e7eb",
      background: "#f9fafb", display: "flex", flexDirection: "column",
      height: "100vh", overflow: "auto",
    }}>
      {/* Global search */}
      <div style={{ padding: "0.75rem" }}>
        <input
          type="search"
          placeholder="Search vault"
          style={{
            width: "100%", padding: "0.375rem 0.5rem", fontSize: "0.813rem",
            border: "1px solid #d1d5db", borderRadius: "4px",
          }}
          onKeyDown={(e) => {
            if (e.key === "Enter") {
              onNavigate("search");
            }
          }}
        />
      </div>

      {/* Saved groups placeholder */}
      <div style={{ padding: "0 0.75rem", marginBottom: "0.5rem" }}>
        <div style={{ fontSize: "0.688rem", fontWeight: 600, color: "#9ca3af", textTransform: "uppercase", letterSpacing: "0.05em", marginBottom: "0.25rem" }}>
          Saved Groups
        </div>
        <div style={{ fontSize: "0.813rem", color: "#9ca3af", padding: "0.25rem 0" }}>
          No saved groups yet
        </div>
      </div>

      <div style={{ borderTop: "1px solid #e5e7eb", margin: "0 0.75rem" }} />

      {/* Navigation */}
      <div style={{ padding: "0.5rem 0.75rem", flex: 1 }}>
        <button style={linkStyle("conversations")} onClick={() => onNavigate("conversations")}>
          Conversations
        </button>
        <button style={linkStyle("contacts")} onClick={() => onNavigate("contacts")}>
          Contacts
        </button>
        <button style={linkStyle("trash")} onClick={() => onNavigate("trash")}>
          Trash
        </button>
      </div>

      {/* Import/Export — Tauri only */}
      {isTauri() && (
        <div style={{ padding: "0.5rem 0.75rem", borderTop: "1px solid #e5e7eb" }}>
          <button
            onClick={() => onNavigate("import")}
            style={{
              width: "100%", padding: "0.5rem", marginBottom: "0.375rem",
              fontSize: "0.875rem", fontWeight: 600,
            }}
          >
            Import
          </button>
          <button
            onClick={() => onNavigate("export")}
            style={{ width: "100%", padding: "0.5rem", fontSize: "0.875rem" }}
          >
            Export
          </button>
        </div>
      )}

      {/* Profile + Settings */}
      <div style={{ padding: "0.5rem 0.75rem", borderTop: "1px solid #e5e7eb" }}>
        <button style={linkStyle("profile")} onClick={() => onNavigate("profile")}>
          Profile
        </button>
        <button style={linkStyle("settings")} onClick={() => onNavigate("settings")}>
          Settings
        </button>
        <button
          onClick={logout}
          style={{
            ...linkStyle(""),
            color: "#991b1b",
            marginTop: "0.25rem",
          }}
        >
          Sign out
        </button>
      </div>
    </div>
  );
}
```

- [ ] **Step 2: Build and verify**

```bash
cd web && npm run build
```

Expected: compiles cleanly.

- [ ] **Step 3: Commit**

```bash
git add web/src/components/LeftPanel.tsx
git commit -m "feat(web): add left panel component

Global search, saved groups placeholder, Conversations/Contacts/Trash
nav, Import/Export (Tauri only), Profile/Settings/Sign out.

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

### Task 5: App layout and router

**Files:**
- Modify: `web/src/App.tsx` — replace with router + auth guard + layout
- Create: `web/src/components/AppLayout.tsx`

**Interfaces:**
- Produces: Routed app with auth guard — `/login` shows LoginScreen, everything else requires auth and renders inside AppLayout
- Consumes: `AuthProvider`, `LeftPanel`, `LoginScreen`

- [ ] **Step 1: Write AppLayout**

```typescript
// web/src/components/AppLayout.tsx

import { useState, type ReactNode } from "react";
import LeftPanel from "./LeftPanel";

export default function AppLayout({ children }: { children: ReactNode }) {
  const [activeView, setActiveView] = useState("conversations");

  return (
    <div style={{ display: "flex", height: "100vh", fontFamily: "system-ui" }}>
      <LeftPanel activeView={activeView} onNavigate={setActiveView} />
      <main style={{ flex: 1, overflow: "auto", background: "#fff" }}>
        {children}
      </main>
    </div>
  );
}
```

- [ ] **Step 2: Rewrite App.tsx**

```typescript
// web/src/App.tsx

import { AuthProvider, useAuth } from "./lib/auth";
import AppLayout from "./components/AppLayout";
import LoginScreen from "./screens/LoginScreen";

function AppContent() {
  const { isAuthenticated } = useAuth();

  if (!isAuthenticated) {
    return <LoginScreen />;
  }

  return (
    <AppLayout>
      <div style={{
        display: "flex", alignItems: "center", justifyContent: "center",
        height: "100%", color: "#9ca3af", fontSize: "0.875rem",
      }}>
        Select a conversation to view messages
      </div>
    </AppLayout>
  );
}

export default function App() {
  return (
    <AuthProvider>
      <AppContent />
    </AuthProvider>
  );
}
```

- [ ] **Step 3: Build and verify**

```bash
cd web && npm run build
```

Expected: compiles cleanly. The app now shows LoginScreen on load and an empty layout after login.

- [ ] **Step 4: Commit**

```bash
git add web/src/App.tsx web/src/components/AppLayout.tsx
git commit -m "feat(web): add app layout shell with router and auth guard

AuthProvider wraps the app. Unauthenticated → LoginScreen.
Authenticated → AppLayout with LeftPanel + main content area.
Empty placeholder state in main content until screens are added.

Co-Authored-By: Claude <noreply@anthropic.com>"
```
