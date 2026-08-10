# React Router v8 Declarative Mode — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace manual `useState`-based view switching with React Router v8 declarative mode (`<HashRouter>` + `<Routes>` + `<Route>`), giving the Tauri desktop app URL-based navigation, browser history, and deep linking.

**Architecture:** Wrap the app in `<HashRouter>`. Auth states become sibling routes with redirect guards. `AppLayout` becomes a layout route that reads `useLocation().pathname` to drive column visibility/content and renders child routes via `<Outlet />`. The message route (`/messages/:id`) is a special case: `AppLayout` renders a single `<Outlet />` in a flex container, and the child `MessageRoute` component produces both columns (ListColumn + main) as siblings — one component, two columns, one Outlet. `LeftPanel` switches from `onNavigate` callback to `useNavigate()`. Search/filter state moves into URL search params.

**Tech Stack:** React 19, React Router v8 (`react-router-dom`), Tauri v2, Vite 6, TypeScript

## Global Constraints

- Use `HashRouter` — Tauri desktop app has no real server for SPA fallback; hash URLs work universally
- Declarative mode only: `<Routes>` + `<Route>` JSX components (not `createHashRouter` data router)
- Auth guard must redirect unauthenticated users to `/login` and new accounts to `/onboarding`
- Contact drawer stays as a local-state overlay (not a route — it's a slide-over panel)
- Vite dev server on port 5173 must still proxy `/v1` → `http://127.0.0.1:8080`
- No new dependencies beyond `react-router-dom` v8
- `LoginScreen` currently takes `onRegister` callback → replace with `useNavigate('/register')`
- `RegisterScreen` currently takes `serverUrl` + `onBack` props → `serverUrl` from auth context, `onBack` → `useNavigate('/login')`



## File Map


| File                                  | Action     | Responsibility                                                                                    |
| ------------------------------------- | ---------- | ------------------------------------------------------------------------------------------------- |
| `web/package.json`                    | Modify     | Add `react-router-dom` v8                                                                         |
| `web/src/main.tsx`                    | Modify     | Wrap `<App />` in `<HashRouter>`                                                                  |
| `web/src/App.tsx`                     | Modify     | Replace `useState` state machine with `<Routes>` tree                                             |
| `web/src/components/AuthGuard.tsx`    | **Create** | Layout route that redirects based on auth state                                                   |
| `web/src/components/AppLayout.tsx`    | Modify     | Chrome shell — LeftPanel + column layout driven by `useLocation()`                                |
| `web/src/components/MessageRoute.tsx` | **Create** | Self-contained 2-column layout for `/messages/:id` — renders ListColumn + MessageView as siblings |
| `web/src/components/LeftPanel.tsx`    | Modify     | Replace `onNavigate` callback with `useNavigate()`; derive active nav from `useLocation()`        |
| `web/src/screens/LoginScreen.tsx`     | Modify     | Replace `onRegister` prop with `useNavigate()`                                                    |
| `web/src/screens/RegisterScreen.tsx`  | Modify     | Replace `serverUrl` + `onBack` props with auth context + `useNavigate()`                          |
| `web/src/lib/views.ts`                | Delete     | `ActiveView` type no longer needed                                                                |




## Route Map

```
/login              → LoginScreen (redirect to / if already authenticated)
/register           → RegisterScreen (redirect to / if already authenticated)
/onboarding         → OnboardingScreen (redirect to / if not needsOnboarding)

  ── AuthGuard ──
    ── AppLayout ──
      /                        → ConversationList (index)
      /contacts                → ContactList
      /trash                   → TrashScreen
      /import                  → ImportScreen
      /export                  → ExportScreen
      /settings                → SettingsScreen
      /messages/:conversationId → MessageRoute (ListColumn[ConvList] + main[MessageView])
```



## Column Layout Decision

AppLayout owns the 3-column chrome. For the `/messages/:id` route, **AppLayout renders a single** `<Outlet />` **in a flex div** — it does not try to split the outlet across two slots. The child `MessageRoute` component renders both `<ListColumn>` and `<main>` as sibling elements. This avoids the "one Outlet, two slots" problem.

---



### Task 1: Install react-router-dom v8

**Files:**

- Modify: `web/package.json`

**Produces:** `react-router-dom` v8 available in node_modules

- [ ] **Step 1: Add dependency**

```bash
cd web && npm install react-router-dom@^8
```

- [ ] **Step 2: Verify install**

```bash
node -e "console.log(require('./web/node_modules/react-router-dom/package.json').version)"
```

Expected: prints `8.x.x`

- [ ] **Step 3: Commit**

```bash
git add web/package.json web/package-lock.json
git commit -m "deps: add react-router-dom v8"
```

---



### Task 2: Wrap app in HashRouter

**Files:**

- Modify: `web/src/main.tsx`

**Consumes:** `react-router-dom` from Task 1

- [ ] **Step 1: Update main.tsx**

```tsx
import React from "react";
import ReactDOM from "react-dom/client";
import { HashRouter } from "react-router-dom";
import App from "./App";
import { initFfmpegToolsFromStorage } from "./lib/ffmpeg-tools";
import "./theme.css";

initFfmpegToolsFromStorage();

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <HashRouter>
      <App />
    </HashRouter>
  </React.StrictMode>,
);
```

- [ ] **Step 2: Verify app loads without crashes**

Run: `cd web && npm run dev`
Open `http://localhost:5173` — app renders (URL shows `/#/`). Existing useState routing still works inside HashRouter.

- [ ] **Step 3: Commit**

```bash
git add web/src/main.tsx
git commit -m "feat: wrap app in HashRouter"
```

---



### Task 3: Create AuthGuard layout route

**Files:**

- Create: `web/src/components/AuthGuard.tsx`

**Consumes:** `useAuth()` from `@/lib/auth`, `Navigate`, `Outlet` from `react-router-dom`

**Produces:** `<AuthGuard />` — renders `<Outlet />` if authenticated + onboarded, else redirects

- [ ] **Step 1: Write AuthGuard component**

```tsx
import { Navigate, Outlet } from "react-router-dom";
import { useAuth } from "@/lib/auth";

/**
 * Layout route: renders child routes via <Outlet /> if the user
 * is authenticated and has completed onboarding. Otherwise redirects.
 */
export function AuthGuard() {
  const { isAuthenticated, needsOnboarding } = useAuth();

  if (!isAuthenticated) {
    return <Navigate to="/login" replace />;
  }

  if (needsOnboarding) {
    return <Navigate to="/onboarding" replace />;
  }

  return <Outlet />;
}
```

- [ ] **Step 2: Verify TypeScript compiles**

Run: `cd web && npx tsc --noEmit`
Expected: no errors in this file

- [ ] **Step 3: Commit**

```bash
git add web/src/components/AuthGuard.tsx
git commit -m "feat: add AuthGuard layout route component"
```

---



### Task 4: Rewrite App.tsx with Routes

**Files:**

- Modify: `web/src/App.tsx`

**Consumes:** `AuthGuard` from Task 3

- [ ] **Step 1: Rewrite App.tsx**

```tsx
import { Routes, Route, Navigate } from "react-router-dom";
import { AuthProvider, useAuth } from "./lib/auth";
import { ThemeProvider } from "./lib/ThemeProvider";
import { AuthGuard } from "./components/AuthGuard";
import AppLayout from "./components/AppLayout";
import LoginScreen from "./screens/LoginScreen";
import RegisterScreen from "./screens/RegisterScreen";
import OnboardingScreen from "./screens/OnboardingScreen";
import ConversationList from "./screens/ConversationList";
import ContactList from "./screens/ContactList";
import TrashScreen from "./screens/TrashScreen";
import ImportScreen from "./screens/ImportScreen";
import ExportScreen from "./screens/ExportScreen";
import SettingsScreen from "./screens/SettingsScreen";

function AppRoutes() {
  const { isAuthenticated, needsOnboarding } = useAuth();

  return (
    <Routes>
      {/* Public routes — redirect to / if already authenticated */}
      <Route
        path="/login"
        element={
          isAuthenticated ? (
            needsOnboarding ? <Navigate to="/onboarding" replace /> : <Navigate to="/" replace />
          ) : (
            <LoginScreen />
          )
        }
      />
      <Route
        path="/register"
        element={
          isAuthenticated ? (
            needsOnboarding ? <Navigate to="/onboarding" replace /> : <Navigate to="/" replace />
          ) : (
            <RegisterScreen />
          )
        }
      />
      <Route
        path="/onboarding"
        element={
          isAuthenticated && needsOnboarding ? (
            <OnboardingScreen />
          ) : (
            <Navigate to="/" replace />
          )
        }
      />

      {/* Protected routes — AuthGuard redirects to /login or /onboarding */}
      <Route element={<AuthGuard />}>
        <Route element={<AppLayout />}>
          <Route index element={<ConversationList />} />
          <Route path="contacts" element={<ContactList />} />
          <Route path="trash" element={<TrashScreen />} />
          <Route path="import" element={<ImportScreen />} />
          <Route path="export" element={<ExportScreen />} />
          <Route path="settings" element={<SettingsScreen />} />
          {/* /messages/:id route added in Task 9 when MessageRoute.tsx is created */}
        </Route>
      </Route>

      {/* Catch-all */}
      <Route path="*" element={<Navigate to="/" replace />} />
    </Routes>
  );
}

export default function App() {
  return (
    <ThemeProvider>
      <AuthProvider>
        <AppRoutes />
      </AuthProvider>
    </ThemeProvider>
  );
}
```

- [ ] **Step 2: Verify TypeScript**

Run: `cd web && npx tsc --noEmit`
Expected: errors in `AppLayout.tsx`, `LeftPanel.tsx`, `LoginScreen.tsx`, `RegisterScreen.tsx` — all fixed in subsequent tasks.

- [ ] **Step 3: Commit**

```bash
git add web/src/App.tsx
git commit -m "feat: replace useState state machine with React Router Routes tree"
```

---



### Task 5: Rewrite AppLayout — pathname-driven columns, search params, single-Outlet message route

**Files:**

- Modify: `web/src/components/AppLayout.tsx`

**Consumes:** `Outlet`, `useLocation`, `useNavigate`, `useSearchParams` from `react-router-dom`

**What changes:**

- `activeView` state → derived from `useLocation().pathname` via `modeFromPathname()`
- `conversationSearch` / `conversationFilter` state → `searchParams.get("q")` / `searchParams.get("f")`
- `contactSearch` state → `searchParams.get("cq")`
- `selectedConversation` state → removed (MessageRoute handles it)
- `selectedContact` → stays as local state (drawer overlay, not a route)
- `onNavigate` prop to LeftPanel → removed
- Message routes: render `<Outlet />` once in a flex div — MessageRoute provides both columns

- [ ] **Step 1: Rewrite AppLayout**

```tsx
import { useState } from "react";
import { Outlet, useLocation, useNavigate, useSearchParams } from "react-router-dom";
import LeftPanel from "./LeftPanel";
import ListColumn from "./ListColumn";
import ConversationList from "../screens/ConversationList";
import ContactDrawer, {
  type ContactBrowseKind,
  type ContactPreview,
} from "./ContactDrawer";

function contactBrowseQuery(contactId: string, kind: ContactBrowseKind): string {
  if (kind === "direct") return `contact:${contactId} is:direct`;
  if (kind === "group") return `contact:${contactId} is:group`;
  return `contact:${contactId}`;
}

function visibleBrowseQuery(
  kind: ContactBrowseKind,
  handles: string[],
  contactId: string,
): string {
  const typeSuffix =
    kind === "direct" ? " is:direct" : kind === "group" ? " is:group" : "";
  const handle = handles.find((h) => h.trim().length > 0)?.trim();
  if (handle) return `handle:${handle}${typeSuffix}`;
  return `contact:${contactId}${typeSuffix}`;
}

type ColumnMode = "conversations" | "contacts" | "trash" | "import" | "export" | "settings";

function modeFromPathname(pathname: string): ColumnMode {
  if (pathname.startsWith("/messages/")) return "conversations";
  if (pathname === "/contacts") return "contacts";
  if (pathname === "/trash") return "trash";
  if (pathname === "/import") return "import";
  if (pathname === "/export") return "export";
  if (pathname === "/settings") return "settings";
  return "conversations";
}

const emptyMainStyle = {
  display: "flex",
  alignItems: "center",
  justifyContent: "center",
  height: "100%",
  color: "var(--muted)",
  fontSize: "0.875rem",
} as const;

export default function AppLayout() {
  const location = useLocation();
  const navigate = useNavigate();
  const [searchParams, setSearchParams] = useSearchParams();

  const [selectedContact, setSelectedContact] = useState<ContactPreview | null>(null);

  const pathname = location.pathname;
  const mode = modeFromPathname(pathname);
  const isMessageRoute = pathname.startsWith("/messages/");
  const contactsMode = mode === "contacts";

  const conversationSearch = searchParams.get("q") || "";
  const conversationFilter = searchParams.get("f") || "";
  const contactSearch = searchParams.get("cq") || "";

  const searchQuery = contactsMode ? contactSearch : conversationSearch;

  function updateSearchParams(updates: Record<string, string>) {
    const next = new URLSearchParams(searchParams);
    for (const [k, v] of Object.entries(updates)) {
      if (v) next.set(k, v); else next.delete(k);
    }
    setSearchParams(next, { replace: true });
  }

  const handleSearch = (q: string) => {
    if (/\bsearch:contacts\b/i.test(q) || contactsMode) {
      navigate(`/contacts?cq=${encodeURIComponent(q)}`);
    } else {
      navigate(`/?q=${encodeURIComponent(q)}`);
    }
  };

  const handleSearchChange = (q: string) => {
    if (contactsMode) {
      updateSearchParams({ cq: q });
      return;
    }
    updateSearchParams({ q: q, f: "" });
  };

  const handleBrowseContactConversations = ({
    contactId,
    kind,
    handles = [],
  }: {
    contactId: string;
    kind: ContactBrowseKind;
    handles?: string[];
  }) => {
    const visible = visibleBrowseQuery(kind, handles, contactId);
    const apiQuery = contactBrowseQuery(contactId, kind);
    setSelectedContact(null);
    navigate(`/?q=${encodeURIComponent(visible)}&f=${encodeURIComponent(apiQuery)}`);
  };

  const isFullScreen = mode === "import" || mode === "export" || mode === "settings";
  const isTrash = mode === "trash";

  // Contact drawer: read openContactId from location state (set by MessageRoute)
  const locationState = location.state as { openContactId?: string } | null;
  const openContactId = locationState?.openContactId ?? null;

  return (
    <div style={{ display: "flex", height: "100vh", fontFamily: "system-ui", background: "var(--bg)", color: "var(--text)" }}>
      <LeftPanel
        onSearchChange={handleSearchChange}
        onSearch={handleSearch}
      />

      {/* Conversations + Contacts: ListColumn via <Outlet />, main is placeholder */}
      {(mode === "conversations" || mode === "contacts") && !isMessageRoute && (
        <>
          <ListColumn
            searchQuery={searchQuery}
            searchMode={contactsMode ? "contacts" : "messages"}
            onSearchChange={handleSearchChange}
            onSearch={handleSearch}
          >
            <Outlet />
          </ListColumn>
          <main style={{ flex: 1, overflow: "auto", background: "var(--bg)", color: "var(--text)", minWidth: 0 }}>
            {mode === "conversations" ? (
              <div style={emptyMainStyle}>Select a conversation to view messages</div>
            ) : (
              <div style={emptyMainStyle}>Select a contact to view details</div>
            )}
          </main>
        </>
      )}

      {/* Trash: ListColumn shows ConversationList with trash query; main shows TrashScreen via <Outlet /> */}
      {isTrash && (
        <>
          <ListColumn
            searchQuery=""
            searchMode="messages"
            onSearchChange={handleSearchChange}
            onSearch={handleSearch}
          >
            <ConversationList
              selectedId={null}
              onSelect={() => {}}
              query="is:trash"
            />
          </ListColumn>
          <main style={{ flex: 1, overflow: "auto", background: "var(--bg)", color: "var(--text)", minWidth: 0 }}>
            <Outlet />
          </main>
        </>
      )}

      {/* Message route: single <Outlet /> — MessageRoute renders both ListColumn + main */}
      {isMessageRoute && (
        <div style={{ display: "flex", flex: 1, minWidth: 0 }}>
          <Outlet />
        </div>
      )}

      {/* Full-screen views: no ListColumn, just main */}
      {isFullScreen && (
        <main style={{ flex: 1, overflow: "auto", background: "var(--bg)", color: "var(--text)", minWidth: 0 }}>
          <Outlet />
        </main>
      )}

      <ContactDrawer
        contactId={selectedContact?.id ?? openContactId ?? null}
        preview={selectedContact}
        onClose={() => setSelectedContact(null)}
        onBrowseConversations={handleBrowseContactConversations}
      />
    </div>
  );
}
```

- [ ] **Step 2: Verify TypeScript compiles**

Run: `cd web && npx tsc --noEmit`
Expected: errors only in `LeftPanel.tsx` (still expects old props) — fixed in Task 6.

- [ ] **Step 3: Commit**

```bash
git add web/src/components/AppLayout.tsx
git commit -m "feat: drive AppLayout columns from URL pathname + search params"
```

---



### Task 6: Rewrite LeftPanel — useNavigate instead of onNavigate callback

**Files:**

- Modify: `web/src/components/LeftPanel.tsx`

**Consumes:** `useLocation`, `useNavigate` from `react-router-dom`

**What changes:**

- Remove `activeView` and `onNavigate` props
- Derive active nav item from `useLocation().pathname`
- Each nav button's `onClick` calls `navigate("/path")`
- Saved groups, search callbacks, Tauri gating, logout — unchanged

- [ ] **Step 1: Update imports and function signature**

At the top of `LeftPanel.tsx`, add:

```tsx
import { useLocation, useNavigate } from "react-router-dom";
```

Change the function signature from:

```tsx
export default function LeftPanel({
  activeView,
  onNavigate,
  onSearchChange,
  onSearch,
}: {
  activeView: ActiveView;
  onNavigate: (view: ActiveView) => void;
  onSearchChange: (v: string) => void;
  onSearch: (q: string) => void;
})
```

To:

```tsx
export default function LeftPanel({
  onSearchChange,
  onSearch,
}: {
  onSearchChange: (v: string) => void;
  onSearch: (q: string) => void;
})
```

Remove the `ActiveView` import:

```tsx
// DELETE this line:
import type { ActiveView } from "../lib/views";
```

- [ ] **Step 2: Add router hooks and active derivation**

Add these lines at the top of the function body, after the `const { logout } = useAuth();` line:

```tsx
const location = useLocation();
const navigate = useNavigate();

function isActive(path: string): boolean {
  if (path === "/") return location.pathname === "/" || location.pathname.startsWith("/messages/");
  return location.pathname.startsWith(path);
}
```

- [ ] **Step 3: Update the linkStyle helper**

Replace:

```tsx
const linkStyle = (view: ActiveView): CSSProperties => ({
  ...
  background: activeView === view ? "var(--hover)" : "transparent",
  fontWeight: activeView === view ? 600 : 400,
  ...
});
```

With:

```tsx
const linkStyle = (active: boolean): CSSProperties => ({
  padding: "0.375rem 0.75rem",
  fontSize: "0.875rem",
  cursor: "pointer",
  borderRadius: "4px",
  background: active ? "var(--hover)" : "transparent",
  fontWeight: active ? 600 : 400,
  border: "none",
  textAlign: "left",
  width: "100%",
  display: "flex",
  alignItems: "center",
  gap: "0.5rem",
  color: "var(--text)",
});

const signOutStyle: CSSProperties = {
  ...linkStyle(false),
  background: "transparent",
  fontWeight: 400,
  color: "var(--danger)",
  marginTop: "0.25rem",
};
```

- [ ] **Step 4: Update each nav button**

Replace each `onClick={() => onNavigate("viewName")}` with `onClick={() => navigate("/path")}`:

```tsx
{/* Conversations */}
<button style={linkStyle(isActive("/"))} onClick={() => navigate("/")}>
  <ConversationsIcon />
  Conversations
</button>

{/* Contacts */}
<button style={linkStyle(isActive("/contacts"))} onClick={() => navigate("/contacts")}>
  <ContactsIcon />
  Contacts
</button>

{/* Trash */}
<button style={linkStyle(isActive("/trash"))} onClick={() => navigate("/trash")}>
  <TrashIcon />
  Trash
</button>

{/* Import */}
<button style={linkStyle(isActive("/import"))} onClick={() => navigate("/import")}>
  <ImportIcon />
  Import
</button>

{/* Export */}
<button style={linkStyle(isActive("/export"))} onClick={() => navigate("/export")}>
  <ExportIcon />
  Export
</button>

{/* Settings */}
<button style={linkStyle(isActive("/settings"))} onClick={() => navigate("/settings")}>
  Settings
</button>
```

- [ ] **Step 5: Verify TypeScript compiles**

Run: `cd web && npx tsc --noEmit`
Expected: errors only in `LoginScreen.tsx` and `RegisterScreen.tsx` — fixed in Tasks 7-8.

- [ ] **Step 6: Commit**

```bash
git add web/src/components/LeftPanel.tsx
git commit -m "feat: replace onNavigate callback with useNavigate in LeftPanel"
```

---



### Task 7: Update LoginScreen — useNavigate instead of onRegister prop

**Files:**

- Modify: `web/src/screens/LoginScreen.tsx`

**Consumes:** `useNavigate` from `react-router-dom`

- [ ] **Step 1: Replace** `onRegister` **prop with** `useNavigate`

At the top of the file, add:

```tsx
import { useNavigate } from "react-router-dom";
```

Change the function signature:

```tsx
// Before:
export default function LoginScreen({ onRegister }: { onRegister?: () => void }) {

// After:
export default function LoginScreen() {
```

Add the hook inside the function body:

```tsx
const navigate = useNavigate();
```

- [ ] **Step 2: Update the "Create an account" button**

Find the block guarded by `onRegister &&`:

```tsx
{onRegister && (
  <>
    <div style={orRowStyle}>
      <span style={orLineStyle} />
      <span style={orTextStyle}>OR</span>
      <span style={orLineStyle} />
    </div>
    <button type="button" onClick={onRegister} style={accentLink}>
      Create an account
    </button>
  </>
)}
```

Replace with (always renders when `authMode === "local"`):

```tsx
<>
  <div style={orRowStyle}>
    <span style={orLineStyle} />
    <span style={orTextStyle}>OR</span>
    <span style={orLineStyle} />
  </div>
  <button type="button" onClick={() => navigate("/register")} style={accentLink}>
    Create an account
  </button>
</>
```

- [ ] **Step 3: Verify TypeScript compiles**

Run: `cd web && npx tsc --noEmit`

- [ ] **Step 4: Commit**

```bash
git add web/src/screens/LoginScreen.tsx
git commit -m "feat: replace onRegister callback with useNavigate in LoginScreen"
```

---



### Task 8: Update RegisterScreen — useNavigate instead of props

**Files:**

- Modify: `web/src/screens/RegisterScreen.tsx`

**Consumes:** `useNavigate` from `react-router-dom`, `useAuth` from `@/lib/auth`

- [ ] **Step 1: Replace** `serverUrl` **and** `onBack` **props**

At the top of the file, add:

```tsx
import { useNavigate } from "react-router-dom";
```

Change the function signature:

```tsx
// Before:
export default function RegisterScreen({ serverUrl, onBack }: { serverUrl: string; onBack: () => void }) {

// After:
export default function RegisterScreen() {
```

Add hooks:

```tsx
const navigate = useNavigate();
const { login, serverUrl } = useAuth();
```

- [ ] **Step 2: Update the back button**

Find:

```tsx
<AuthBackButton label="Back to login" onClick={onBack} />
```

Replace with:

```tsx
<AuthBackButton label="Back to login" onClick={() => navigate("/login")} />
```

- [ ] **Step 3: Verify TypeScript compiles**

Run: `cd web && npx tsc --noEmit`

- [ ] **Step 4: Commit**

```bash
git add web/src/screens/RegisterScreen.tsx
git commit -m "feat: replace onBack callback with useNavigate in RegisterScreen"
```

---



### Task 9: Create MessageRoute — self-contained 2-column layout for /messages/:id

**Files:**

- Create: `web/src/components/MessageRoute.tsx`
- Modify: `web/src/App.tsx`

**Consumes:** `useParams`, `useLocation`, `useNavigate`, `useSearchParams` from `react-router-dom`

**Produces:** `<MessageRoute />` — renders `<ListColumn>` + `<main>` as siblings. AppLayout renders it via a single `<Outlet />` in a flex container. Conversation data flows through `useLocation().state`.

- [ ] **Step 1: Write MessageRoute**

```tsx
import { useParams, useLocation, useNavigate, useSearchParams } from "react-router-dom";
import ListColumn from "./ListColumn";
import ConversationList from "../screens/ConversationList";
import MessageView from "../screens/MessageView";
import type { Conversation } from "../lib/types";

export default function MessageRoute() {
  const { conversationId } = useParams<{ conversationId: string }>();
  const location = useLocation();
  const navigate = useNavigate();
  const [searchParams, setSearchParams] = useSearchParams();

  const conversationSearch = searchParams.get("q") || "";
  const conversationFilter = searchParams.get("f") || "";
  const query = conversationFilter || conversationSearch;

  const state = location.state as {
    conversation?: Conversation;
    openContactId?: string;
  } | null;
  const conversation = state?.conversation ?? null;
  const openContactId = state?.openContactId ?? null;

  const handleSearchChange = (q: string) => {
    const next = new URLSearchParams(searchParams);
    if (q) next.set("q", q); else next.delete("q");
    next.delete("f");
    setSearchParams(next, { replace: true });
  };

  const handleSearch = (q: string) => {
    navigate(`/messages/${conversationId}?q=${encodeURIComponent(q)}`, {
      state: { conversation, openContactId },
    });
  };

  return (
    <>
      <ListColumn
        searchQuery={conversationSearch}
        searchMode="messages"
        onSearchChange={handleSearchChange}
        onSearch={handleSearch}
      >
        <ConversationList
          selectedId={conversationId ?? null}
          onSelect={(c) =>
            navigate(`/messages/${c.id}`, {
              state: { conversation: c, openContactId },
            })
          }
          query={query}
        />
      </ListColumn>
      <main style={{ flex: 1, overflow: "auto", background: "var(--bg)", color: "var(--text)", minWidth: 0 }}>
        {conversation ? (
          <MessageView
            conversation={conversation}
            onOpenContact={(contactId: string) => {
              navigate(location.pathname + location.search, {
                state: { conversation, openContactId: contactId },
              });
            }}
          />
        ) : (
          <div style={{
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            height: "100%",
            color: "var(--muted)",
            fontSize: "0.875rem",
          }}>
            Select a conversation to view messages
          </div>
        )}
      </main>
    </>
  );
}
```

- [ ] **Step 2: Wire MessageRoute into App.tsx**

In `web/src/App.tsx`, add the import and route:

```tsx
// ADD this import near the other component imports:
import MessageRoute from "./components/MessageRoute";

// ADD this route inside <Route element={<AppLayout />}> (before the closing </Route>):
<Route path="messages/:conversationId" element={<MessageRoute />} />
```

- [ ] **Step 3: Verify TypeScript compiles**

Run: `cd web && npx tsc --noEmit`
Expected: zero errors across the entire project.

- [ ] **Step 4: Commit**

```bash
git add web/src/components/MessageRoute.tsx web/src/App.tsx
git commit -m "feat: add MessageRoute for URL-driven conversation selection via /messages/:id"
```

---



### Task 10: Delete ActiveView type + final cleanup

**Files:**

- Delete: `web/src/lib/views.ts`

- [ ] **Step 1: Verify no remaining references**

```bash
grep -r "ActiveView\|from.*lib/views\|from.*\.\./lib/views" web/src --include="*.tsx" --include="*.ts"
```

Expected: no output.

- [ ] **Step 2: Delete the file**

```bash
rm web/src/lib/views.ts
```

- [ ] **Step 3: Verify full TypeScript compilation**

```bash
cd web && npx tsc --noEmit
```

Expected: zero errors.

- [ ] **Step 4: Commit**

```bash
git rm web/src/lib/views.ts
git commit -m "chore: remove ActiveView type (replaced by route paths)"
```

---



### Task 11: End-to-end verification

- [ ] **Step 1: Start dev server**

```bash
cd web && npm run dev
```

- [ ] **Step 2: Manual smoke test checklist**

Navigate through the app at `http://localhost:5173`:


| Test                                          | Expected behavior                                        |
| --------------------------------------------- | -------------------------------------------------------- |
| Visit `/#/`                                   | Conversation list + "Select a conversation" placeholder  |
| Visit `/#/login`                              | Login screen renders                                     |
| Click "Create an account"                     | Navigates to `/#/register`                               |
| Click "Back to login" on register             | Navigates to `/#/login`                                  |
| Log in                                        | Redirects to `/#/` (or `/#/onboarding` for new accounts) |
| Click "Conversations" in sidebar              | URL → `/#/`, conversation list shows                     |
| Click "Contacts" in sidebar                   | URL → `/#/contacts`, contact list shows                  |
| Click "Trash" in sidebar                      | URL → `/#/trash`, trash view shows                       |
| Click "Import" in sidebar                     | URL → `/#/import`, import screen shows                   |
| Click "Export" in sidebar                     | URL → `/#/export`, export screen shows                   |
| Click "Settings" in sidebar                   | URL → `/#/settings`, settings screen shows               |
| Click a conversation                          | URL → `/#/messages/<id>`, message view loads             |
| Browser back button                           | Returns to previous view                                 |
| Browser forward button                        | Returns to message view                                  |
| Search for conversations                      | URL includes `?q=...`                                    |
| Visit `/#/nonexistent`                        | Redirects to `/#/`                                       |
| Sign out                                      | Redirects to `/#/login`                                  |
| Directly visit `/#/settings` while logged out | Redirects to `/#/login`                                  |


- [ ] **Step 3: Fix issues, commit corrections**

```bash
git add -A
git commit -m "fix: smoke test corrections for React Router migration"
```

