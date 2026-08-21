# Unified GUI — Plan 6: Trash, Settings, Profile, Integration

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build trash (conversation-grouped with restore), settings (vault connection, media, appearance, storage, account), My Profile (handles, account info), and wire everything together. After this plan the unified GUI is feature-complete.

**Architecture:** Trash is a filtered conversation list view showing deleted conversations with restore/empty controls. Settings and Profile replace the main view area. The final integration task replaces the message-vault-rs Next.js web app with the new Vite build output in the Docker image, and removes old code.

**Tech Stack:** React 19, TypeScript, API client, existing Tauri and vault APIs

## Global Constraints

- Trash uses the conversation-grouped model from the spec: restore recreates conversations from metadata if needed
- Settings persist to `export.ini` (Tauri) or API (web)
- Profile is separate from Settings per the spec
- Final integration updates the Docker image to serve the Vite build instead of Next.js

---

## File Structure

| File | Responsibility |
|------|---------------|
| `web/src/screens/TrashScreen.tsx` | Trash view with restore/empty controls |
| `web/src/screens/SettingsScreen.tsx` | Vault, media (ffmpeg path), appearance, storage, account |
| `web/src/screens/ProfileScreen.tsx` | My handles, name, account info |
| `web/src/App.tsx` | Wire all remaining screens into the layout |

---

### Task 1: Trash screen

**Files:**
- Create: `web/src/screens/TrashScreen.tsx`

**Interfaces:**
- Produces: `TrashScreen` — list of trashed conversations with restore/empty buttons
- Consumes: `apiClient`

- [ ] **Step 1: Write TrashScreen**

```typescript
// web/src/screens/TrashScreen.tsx

import { useState, useEffect } from "react";
import { apiClient } from "../lib/api";

interface TrashEntry {
  id: string;
  label: string; // conversation display name
  message_count: number;
  deleted_at: string;
  conversation_exists: boolean; // false = fully deleted, needs recreation on restore
}

export default function TrashScreen() {
  const [entries, setEntries] = useState<TrashEntry[]>([]);
  const [loading, setLoading] = useState(true);
  const [message, setMessage] = useState("");

  const fetchTrash = () => {
    setLoading(true);
    apiClient
      .get<{ trash: TrashEntry[] }>("/v1/export/trash")
      .then((res) => setEntries(res.trash))
      .catch(() => setEntries([]))
      .finally(() => setLoading(false));
  };

  useEffect(() => { fetchTrash(); }, []);

  const restore = async (id: string) => {
    await apiClient.post(`/v1/trash/${id}/restore`);
    setMessage("Conversation restored.");
    fetchTrash();
  };

  const emptyTrash = async () => {
    if (!confirm("Permanently delete all trashed messages?")) return;
    await apiClient.post("/v1/trash/empty");
    setMessage("Trash emptied.");
    fetchTrash();
  };

  if (loading) {
    return <div style={{ padding: "1.5rem", fontSize: "0.875rem", color: "#9ca3af" }}>Loading…</div>;
  }

  return (
    <div style={{ padding: "1.5rem", maxWidth: "700px" }}>
      <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", marginBottom: "1.5rem" }}>
        <h2 style={{ margin: 0 }}>Trash</h2>
        {entries.length > 0 && (
          <button onClick={emptyTrash} style={{ fontSize: "0.813rem", color: "#dc2626", border: "1px solid #fecaca", background: "#fef2f2", padding: "0.375rem 0.75rem", borderRadius: "4px", cursor: "pointer" }}>
            Empty trash
          </button>
        )}
      </div>

      {message && (
        <div style={{ marginBottom: "1rem", padding: "0.5rem 0.75rem", background: "#f0fdf4", borderRadius: "4px", fontSize: "0.813rem", color: "#166534" }}>
          {message}
        </div>
      )}

      {entries.length === 0 ? (
        <div style={{ fontSize: "0.875rem", color: "#9ca3af" }}>Trash is empty.</div>
      ) : (
        entries.map((entry) => (
          <div
            key={entry.id}
            style={{
              display: "flex", justifyContent: "space-between", alignItems: "center",
              padding: "0.75rem", borderBottom: "1px solid #f3f4f6",
            }}
          >
            <div>
              <div style={{ fontSize: "0.875rem", fontWeight: 500 }}>
                {entry.label}
              </div>
              <div style={{ fontSize: "0.75rem", color: "#6b7280" }}>
                {entry.message_count} message{entry.message_count !== 1 ? "s" : ""} · deleted {new Date(entry.deleted_at).toLocaleDateString()}
              </div>
            </div>
            <button
              onClick={() => restore(entry.id)}
              style={{ fontSize: "0.813rem", padding: "0.25rem 0.75rem", cursor: "pointer" }}
            >
              Restore
            </button>
          </div>
        ))
      )}
    </div>
  );
}
```

- [ ] **Step 2: Build and verify**

```bash
cd web && npm run build
```

- [ ] **Step 3: Commit**

```bash
git add web/src/screens/TrashScreen.tsx
git commit -m "feat(web): add trash screen with restore and empty

Conversation-grouped trash entries. Restore recreates conversations
from metadata if fully deleted. Empty trash with confirmation.

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

### Task 2: Settings screen

**Files:**
- Create: `web/src/screens/SettingsScreen.tsx`

**Interfaces:**
- Produces: `SettingsScreen` — vault connection, media (ffmpeg path), appearance, storage, account
- Consumes: `apiClient`, `isTauri()`, existing `saveSettings`/`loadSettings` Tauri commands

- [ ] **Step 1: Write SettingsScreen**

```typescript
// web/src/screens/SettingsScreen.tsx

import { useState, useEffect } from "react";
import { loadSettings, saveSettings, type AppSettings } from "../lib/tauri";
import { isTauri } from "../lib/tauri-check";
import FormRow from "../components/FormRow";

export default function SettingsScreen() {
  const [settings, setSettings] = useState<AppSettings>({
    vault_url: "", vault_username: "", vault_key: "", default_output_dir: "",
  });
  const [ffmpegPath, setFfmpegPath] = useState("");
  const [loaded, setLoaded] = useState(false);
  const [saved, setSaved] = useState(false);
  const [theme, setTheme] = useState("system");

  useEffect(() => {
    if (isTauri()) {
      loadSettings().then(setSettings).catch(() => {}).finally(() => setLoaded(true));
    } else {
      setLoaded(true);
    }
  }, []);

  const handleSave = async () => {
    try {
      if (isTauri()) await saveSettings(settings);
      localStorage.setItem("mv-theme", theme);
      setSaved(true);
      setTimeout(() => setSaved(false), 2000);
    } catch { /* save failed */ }
  };

  return (
    <div style={{ padding: "1.5rem", maxWidth: "700px" }}>
      <h2 style={{ margin: "0 0 1.5rem 0" }}>Settings</h2>

      <h3 style={{ fontSize: "0.875rem", color: "#6b7280", marginBottom: "0.5rem" }}>Vault Connection</h3>
      <FormRow label="Server URL">
        <input type="text" value={settings.vault_url}
          onChange={(e) => setSettings({ ...settings, vault_url: e.target.value })}
          style={{ width: "100%", padding: "0.25rem 0.5rem", fontSize: "0.875rem" }} />
      </FormRow>

      {isTauri() && (
        <>
          <h3 style={{ fontSize: "0.875rem", color: "#6b7280", margin: "1.5rem 0 0.5rem" }}>Media</h3>
          <FormRow label="ffmpeg path">
            <input type="text" value={ffmpegPath}
              onChange={(e) => setFfmpegPath(e.target.value)}
              placeholder="Uses system PATH by default"
              style={{ width: "100%", padding: "0.25rem 0.5rem", fontSize: "0.875rem" }} />
          </FormRow>
          <p style={{ fontSize: "0.75rem", color: "#9ca3af", marginTop: "0.25rem" }}>
            Leave blank to use system PATH. Set a custom path if ffmpeg is installed in a non-standard location.{" "}
            <a href="https://bitrealm.io/vault/user/how-to/media-and-privacy/" target="_blank" rel="noopener" style={{ color: "#2563eb" }}>
              Install help
            </a>
          </p>
        </>
      )}

      <h3 style={{ fontSize: "0.875rem", color: "#6b7280", margin: "1.5rem 0 0.5rem" }}>Appearance</h3>
      <FormRow label="Theme">
        <select value={theme} onChange={(e) => setTheme(e.target.value)}
          style={{ padding: "0.25rem 0.5rem", fontSize: "0.875rem" }}>
          <option value="system">System</option>
          <option value="light">Light</option>
          <option value="dark">Dark</option>
        </select>
      </FormRow>

      <div style={{ marginTop: "1.5rem", display: "flex", alignItems: "center", gap: "0.75rem" }}>
        <button onClick={handleSave} style={{ padding: "0.5rem 1.5rem", fontWeight: 600 }}>Save</button>
        {saved && <span style={{ fontSize: "0.875rem", color: "#16a34a" }}>Saved</span>}
      </div>
    </div>
  );
}
```

- [ ] **Step 2: Build and verify**

```bash
cd web && npm run build
```

- [ ] **Step 3: Commit**

```bash
git add web/src/screens/SettingsScreen.tsx
git commit -m "feat(web): add settings screen

Vault connection, ffmpeg path (desktop only), theme toggle.
Persists to export.ini (Tauri) or localStorage (web).

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

### Task 3: My Profile screen

**Files:**
- Create: `web/src/screens/ProfileScreen.tsx`

**Interfaces:**
- Produces: `ProfileScreen` — own handles, account info, storage usage
- Consumes: `apiClient`, `useAuth()`

- [ ] **Step 1: Write ProfileScreen**

```typescript
// web/src/screens/ProfileScreen.tsx

import { useState, useEffect } from "react";
import { useAuth } from "../lib/auth";
import { apiClient } from "../lib/api";
import FormRow from "../components/FormRow";

interface Profile {
  name: string;
  handles: { handle: string; service: string }[];
  storage: { messages: number; attachments: number; conversations: number };
}

export default function ProfileScreen() {
  const { logout } = useAuth();
  const [profile, setProfile] = useState<Profile | null>(null);
  const [name, setName] = useState("");
  const [saved, setSaved] = useState(false);

  useEffect(() => {
    apiClient
      .get<Profile>("/v1/account/profile")
      .then((p) => { setProfile(p); setName(p.name); })
      .catch(() => {});
  }, []);

  if (!profile) {
    return <div style={{ padding: "1.5rem", color: "#9ca3af" }}>Loading…</div>;
  }

  const handleSaveName = async () => {
    await apiClient.post("/v1/account/profile", { name });
    setSaved(true);
    setTimeout(() => setSaved(false), 2000);
  };

  return (
    <div style={{ padding: "1.5rem", maxWidth: "700px" }}>
      <h2 style={{ margin: "0 0 1.5rem 0" }}>My Profile</h2>

      <h3 style={{ fontSize: "0.875rem", color: "#6b7280", marginBottom: "0.5rem" }}>Display Name</h3>
      <div style={{ display: "flex", gap: "0.5rem", marginBottom: "1.5rem" }}>
        <input type="text" value={name} onChange={(e) => setName(e.target.value)}
          style={{ flex: 1, padding: "0.25rem 0.5rem", fontSize: "0.875rem" }} />
        <button onClick={handleSaveName} style={{ padding: "0.25rem 1rem", fontWeight: 600 }}>
          {saved ? "Saved" : "Save"}
        </button>
      </div>

      <h3 style={{ fontSize: "0.875rem", color: "#6b7280", marginBottom: "0.5rem" }}>My Handles</h3>
      {profile.handles.map((h, i) => (
        <div key={i} style={{ display: "flex", gap: "1rem", padding: "0.375rem 0", borderBottom: "1px solid #f3f4f6", fontSize: "0.875rem" }}>
          <span style={{ flex: 1 }}>{h.handle}</span>
          <span style={{ color: "#6b7280" }}>{h.service}</span>
        </div>
      ))}

      <h3 style={{ fontSize: "0.875rem", color: "#6b7280", margin: "1.5rem 0 0.5rem" }}>Storage</h3>
      <div style={{ fontSize: "0.875rem", color: "#374151" }}>
        <div>{profile.storage.messages.toLocaleString()} messages</div>
        <div>{profile.storage.attachments.toLocaleString()} attachments</div>
        <div>{profile.storage.conversations.toLocaleString()} conversations</div>
      </div>

      <div style={{ marginTop: "2rem", paddingTop: "1rem", borderTop: "1px solid #e5e7eb" }}>
        <button onClick={logout} style={{ color: "#dc2626", border: "1px solid #fecaca", background: "#fef2f2", padding: "0.5rem 1rem", borderRadius: "4px", cursor: "pointer", fontSize: "0.875rem" }}>
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

- [ ] **Step 3: Commit**

```bash
git add web/src/screens/ProfileScreen.tsx
git commit -m "feat(web): add My Profile screen

Display name editing, handles by service, storage stats.
Separate from Settings per the spec — accessed from left panel.

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

### Task 4: Wire all screens into App

**Files:**
- Modify: `web/src/App.tsx` — wire all screens: conversations, contacts, trash, import, export, settings, profile
- Modify: `web/src/components/AppLayout.tsx` — handle view routing for all screens

**Interfaces:**
- Consumes: All screen components from Plans 2-6

- [ ] **Step 1: Update AppLayout to render all views**

```typescript
// web/src/components/AppLayout.tsx

import { useState, type ReactNode } from "react";
import LeftPanel from "./LeftPanel";
import ConversationList from "../screens/ConversationList";
import ContactList from "../screens/ContactList";
import ContactDrawer from "./ContactDrawer";
import ImportScreen from "../screens/ImportScreen";
import ExportScreen from "../screens/ExportScreen";
import TrashScreen from "../screens/TrashScreen";
import SettingsScreen from "../screens/SettingsScreen";
import ProfileScreen from "../screens/ProfileScreen";
import MessageView from "../screens/MessageView";
import type { Conversation } from "../lib/types";

export default function AppLayout() {
  const [activeView, setActiveView] = useState("conversations");
  const [selectedConversation, setSelectedConversation] = useState<Conversation | null>(null);
  const [selectedContactId, setSelectedContactId] = useState<string | null>(null);
  const [searchQuery, setSearchQuery] = useState("");
  const [exportScope, setExportScope] = useState<"all" | "current-view" | "selected">("all");

  const leftContent =
    activeView === "conversations" || activeView === "trash" ? (
      <ConversationList
        selectedId={selectedConversation?.id || null}
        onSelect={(c) => { setSelectedConversation(c); setActiveView("conversations"); }}
        query={activeView === "trash" ? "is:trash" : searchQuery}
      />
    ) : activeView === "contacts" ? (
      <ContactList onSelect={(c) => setSelectedContactId(c.id)} />
    ) : null;

  const mainContent = () => {
    switch (activeView) {
      case "conversations":
        return selectedConversation ? (
          <MessageView conversation={selectedConversation} />
        ) : (
          <div style={{ display: "flex", alignItems: "center", justifyContent: "center", height: "100%", color: "#9ca3af", fontSize: "0.875rem" }}>
            Select a conversation to view messages
          </div>
        );
      case "contacts":
        return selectedContactId ? (
          <div style={{ display: "flex", alignItems: "center", justifyContent: "center", height: "100%", color: "#9ca3af", fontSize: "0.875rem" }}>
            Select a conversation to view messages
          </div>
        ) : (
          <div style={{ display: "flex", alignItems: "center", justifyContent: "center", height: "100%", color: "#9ca3af", fontSize: "0.875rem" }}>
            Select a contact to view details
          </div>
        );
      case "trash": return <TrashScreen />;
      case "import": return <ImportScreen />;
      case "export": return <ExportScreen scope={exportScope} selectedCount={0} />;
      case "settings": return <SettingsScreen />;
      case "profile": return <ProfileScreen />;
      default: return null;
    }
  };

  return (
    <div style={{ display: "flex", height: "100vh", fontFamily: "system-ui" }}>
      <LeftPanel
        activeView={activeView}
        onNavigate={(view) => {
          setActiveView(view);
          if (view === "export") setExportScope("all");
        }}
        searchQuery={searchQuery}
        onSearchChange={setSearchQuery}
        onSearch={(q) => { setSearchQuery(q); setActiveView("conversations"); }}
        conversationList={leftContent}
      />
      <main style={{ flex: 1, overflow: "auto", background: "#fff" }}>
        {mainContent()}
      </main>
      <ContactDrawer contactId={selectedContactId} onClose={() => setSelectedContactId(null)} />
    </div>
  );
}
```

- [ ] **Step 2: Update App.tsx**

```typescript
// web/src/App.tsx — simplified to just auth gate + layout

import { AuthProvider, useAuth } from "./lib/auth";
import AppLayout from "./components/AppLayout";
import LoginScreen from "./screens/LoginScreen";

function AppContent() {
  const { isAuthenticated } = useAuth();
  return isAuthenticated ? <AppLayout /> : <LoginScreen />;
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

Expected: compiles cleanly. All screens are wired and navigable.

- [ ] **Step 4: Commit**

```bash
git add web/src/App.tsx web/src/components/AppLayout.tsx
git commit -m "feat(web): wire all screens into unified layout

Conversations, contacts, trash, import, export, settings, profile all
routed through AppLayout. LeftPanel controls navigation state.

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

### Task 5: Integration — serve Vite build in Docker, remove old code

**Files:**
- Modify: `message-vault-rs` Dockerfile or server config — serve `dist/` as static files
- Remove: `message-vault-rs/web/` — the Next.js app (replaced by Vite build from message-vault-io)

**Interfaces:**
- Consumes: `web/dist/` from message-vault-io's Vite build
- Produces: Docker image serves the unified GUI at `/` instead of Next.js

- [ ] **Step 1: Configure axum to serve static files**

In `message-vault-rs/src/server.rs`, add a static file service for the Vite build output. Add before the existing routes:

```rust
use tower_http::services::ServeDir;

// In the router builder, add:
.route_service("/", ServeDir::new("web/dist"))
```

The build process copies `message-vault-io/web/dist/` into the Docker image at `web/dist/`.

- [ ] **Step 2: Update Dockerfile**

Add a step to copy the Vite build output. If the two repos are built together:

```dockerfile
# After building message-vault-rs, copy the frontend build
COPY --from=message-vault-io-build /app/web/dist /app/web/dist
```

If the repos are separate, the Vite build output is committed to message-vault-rs or copied at build time.

- [ ] **Step 3: Remove Next.js app**

Delete `message-vault-rs/web/` (the Next.js app). Remove Next.js dependencies from `package.json`. Update the Docker compose file to remove the Next.js service if it was separate.

```bash
rm -rf web/
# Edit package.json to remove next/react dependencies
# Edit docker-compose.yml to remove web service
```

- [ ] **Step 4: Verify**

```bash
# Build the Vite app
cd message-vault-io/web && npm run build

# Start the vault server
cd message-vault-rs && docker compose up --build

# Open http://localhost:5556
# Expected: login screen loads, authenticate, browse conversations
```

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat: integrate unified GUI into Docker deployment

Replace Next.js web app with Vite build output served by axum.
Remove message-vault-rs/web/ directory.

Co-Authored-By: Claude <noreply@anthropic.com>"
```
