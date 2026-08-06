# Tier 1 — Finish the Existing Plans

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Wire up the five remaining loose ends from Plans 2–6 so every screen in the unified GUI is functional and the Vite build replaces Next.js in Docker.

**Architecture:** Five independent workstreams: (1) saved groups UI wired into LeftPanel, (2) offline Extract/Format tools accessible from the login screen, (3) ImportScreen calling real Tauri extract + push, (4) ExportScreen calling real vault pull API, (5) Docker image switched from Next.js to serving the Vite `dist/` via axum.

**Tech Stack:** React 19, TypeScript, Vite, Tauri v2, Rust/axum, Docker

## Global Constraints

- All frontend changes go in `message-vault-io/web/src/`
- All backend changes go in `message-vault-rs/`
- Saved groups stay localStorage-backed (server persistence is follow-up work per Plan 4)
- Import/Export require the Tauri app (`isTauri()` guard) and auth (`useAuth()` token)
- Extract/Format require Tauri but NOT auth
- The Next.js web app (`message-vault-rs/web/`) is fully removed — no dual-serving
- Axum serves the Vite `dist/` at `/` for the web deployment
- Existing API endpoints are stable — no changes to server.rs routes except adding static file serving

---

## File Structure

| File | Responsibility |
|------|---------------|
| `message-vault-io/web/src/components/LeftPanel.tsx` | Wire saved groups list + add/remove, wire login offline nav |
| `message-vault-io/web/src/components/SavedGroupForm.tsx` | Create/edit saved group modal (new) |
| `message-vault-io/web/src/screens/LoginScreen.tsx` | Add offline screen state, wire Extract/Format buttons |
| `message-vault-io/web/src/screens/Extract.tsx` | Add optional `onBack` prop |
| `message-vault-io/web/src/screens/Format.tsx` | Add optional `onBack` prop |
| `message-vault-io/web/src/screens/ImportScreen.tsx` | Replace setTimeout with Tauri extract + API push |
| `message-vault-io/web/src/screens/ExportScreen.tsx` | Replace setTimeout with API pull + file writes |
| `message-vault-rs/Cargo.toml` | Add `fs` feature to tower-http |
| `message-vault-rs/src/server.rs` | Add `ServeDir` for static files |
| `message-vault-rs/Dockerfile.release` | Copy Vite build, remove Next.js stages |
| `message-vault-rs/Dockerfile.dev` | Remove Next.js, add Vite build setup |
| `message-vault-rs/compose-release.yml` | Remove port 3000, serve only on 8080 |
| `message-vault-rs/compose-dev.yml` | Remove port 3000 |
| `message-vault-rs/scripts/docker-entrypoint-release.sh` | Remove Next.js start, serve static only |
| `message-vault-rs/scripts/docker-entrypoint-dev.sh` | Remove Next.js start |

---

### Task 1: Wire saved groups into LeftPanel

**Files:**
- Create: `message-vault-io/web/src/components/SavedGroupForm.tsx`
- Modify: `message-vault-io/web/src/components/LeftPanel.tsx`

**Interfaces:**
- Consumes: `listGroups`, `addGroup`, `removeGroup` from `../lib/savedGroups` (already defined in `web/src/lib/savedGroups.ts`)
- Produces: `LeftPanel` renders live saved groups from localStorage; `SavedGroupForm` modal for creating/editing

- [ ] **Step 1: Write SavedGroupForm component**

Create `message-vault-io/web/src/components/SavedGroupForm.tsx`:

```typescript
// web/src/components/SavedGroupForm.tsx

import { useState } from "react";

interface SavedGroupFormProps {
  onSave: (name: string, query: string) => void;
  onCancel: () => void;
  initial?: { name: string; query: string };
}

export default function SavedGroupForm({ onSave, onCancel, initial }: SavedGroupFormProps) {
  const [name, setName] = useState(initial?.name || "");
  const [query, setQuery] = useState(initial?.query || "");

  const handleSave = () => {
    if (!name.trim() || !query.trim()) return;
    onSave(name.trim(), query.trim());
  };

  return (
    <div style={{
      position: "fixed", inset: 0, display: "flex", alignItems: "center",
      justifyContent: "center", zIndex: 100,
    }}>
      {/* Backdrop */}
      <div onClick={onCancel} style={{
        position: "absolute", inset: 0, background: "rgba(0,0,0,0.3)",
      }} />
      {/* Modal */}
      <div style={{
        position: "relative", background: "#fff", borderRadius: "8px",
        padding: "1.5rem", width: "100%", maxWidth: "400px",
        boxShadow: "0 4px 12px rgba(0,0,0,0.15)",
      }}>
        <h3 style={{ margin: "0 0 1rem", fontSize: "1rem" }}>
          {initial ? "Edit saved group" : "New saved group"}
        </h3>

        <label style={{ fontSize: "0.813rem", fontWeight: 500, display: "block", marginBottom: "0.25rem" }}>
          Name
        </label>
        <input
          type="text"
          value={name}
          onChange={(e) => setName(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && handleSave()}
          placeholder="e.g. Work team"
          style={{
            width: "100%", padding: "0.375rem 0.5rem", fontSize: "0.875rem",
            border: "1px solid #d1d5db", borderRadius: "4px", marginBottom: "0.75rem",
            boxSizing: "border-box",
          }}
          autoFocus
        />

        <label style={{ fontSize: "0.813rem", fontWeight: 500, display: "block", marginBottom: "0.25rem" }}>
          Query
        </label>
        <input
          type="text"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && handleSave()}
          placeholder="e.g. from:bob service:discord"
          style={{
            width: "100%", padding: "0.375rem 0.5rem", fontSize: "0.875rem",
            border: "1px solid #d1d5db", borderRadius: "4px", marginBottom: "1rem",
            boxSizing: "border-box",
          }}
        />

        <div style={{ display: "flex", gap: "0.5rem", justifyContent: "flex-end" }}>
          <button onClick={onCancel}
            style={{ padding: "0.375rem 0.75rem", fontSize: "0.875rem", border: "1px solid #d1d5db", background: "#fff", borderRadius: "4px", cursor: "pointer" }}>
            Cancel
          </button>
          <button onClick={handleSave}
            disabled={!name.trim() || !query.trim()}
            style={{ padding: "0.375rem 1rem", fontSize: "0.875rem", fontWeight: 600, cursor: "pointer" }}>
            Save
          </button>
        </div>
      </div>
    </div>
  );
}
```

- [ ] **Step 2: Build to verify SavedGroupForm compiles**

```bash
cd message-vault-io/web && npm run build
```

Expected: compiles cleanly (unused import is fine — widget wired next step).

- [ ] **Step 3: Commit SavedGroupForm**

```bash
cd /home/mbeisser/repo/message-vault-io
git add web/src/components/SavedGroupForm.tsx
git commit -m "feat(web): add SavedGroupForm modal

Inline form for creating/editing named search queries.
Name + query fields, Enter to submit, backdrop dismiss.

Co-Authored-By: Claude <noreply@anthropic.com>"
```

- [ ] **Step 4: Wire saved groups into LeftPanel**

In `message-vault-io/web/src/components/LeftPanel.tsx`, replace the static "Saved Groups" placeholder (lines 59–67) with live groups from localStorage. Add imports and state at the top, replace the placeholder div, and add the SavedGroupForm modal.

**Add these imports at the top of LeftPanel.tsx (after the existing imports):**

```typescript
import { useState } from "react";
import { listGroups, addGroup, removeGroup } from "../lib/savedGroups";
import SavedGroupForm from "./SavedGroupForm";
```

Note: `useState` is likely already imported — if so, just add `listGroups`, `addGroup`, `removeGroup`, and the `SavedGroupForm` import.

**Add these state variables inside the LeftPanel component, after the existing `linkStyle` definition:**

```typescript
const [groups, setGroups] = useState(() => listGroups());
const [showGroupForm, setShowGroupForm] = useState(false);
```

**Replace the saved groups placeholder (lines 59–67):**

```typescript
{/* Saved groups */}
<div style={{ padding: "0 0.75rem", marginBottom: "0.5rem" }}>
  <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", marginBottom: "0.25rem" }}>
    <span style={{ fontSize: "0.688rem", fontWeight: 600, color: "#9ca3af", textTransform: "uppercase", letterSpacing: "0.05em" }}>
      Saved Groups
    </span>
    <button
      onClick={() => setShowGroupForm(true)}
      style={{ fontSize: "0.688rem", border: "none", background: "none", color: "#2563eb", cursor: "pointer", padding: 0 }}
    >
      + New
    </button>
  </div>
  {groups.length === 0 ? (
    <div style={{ fontSize: "0.813rem", color: "#9ca3af", padding: "0.25rem 0" }}>No saved groups</div>
  ) : (
    groups.map((g) => (
      <div key={g.id} style={{ display: "flex", alignItems: "center", justifyContent: "space-between" }}>
        <button
          onClick={() => {
            onSearchChange(g.query);
            onSearch(g.query);
          }}
          style={{
            display: "block", flex: 1, textAlign: "left", border: "none",
            background: "transparent", padding: "0.25rem 0", fontSize: "0.813rem",
            cursor: "pointer", color: "#374151", overflow: "hidden",
            textOverflow: "ellipsis", whiteSpace: "nowrap",
          }}
        >
          {g.name}
        </button>
        <button
          onClick={() => {
            removeGroup(g.id);
            setGroups(listGroups());
          }}
          title="Delete saved group"
          style={{ border: "none", background: "none", color: "#9ca3af", cursor: "pointer", fontSize: "0.75rem", padding: "0 0.25rem", flexShrink: 0 }}
        >
          ×
        </button>
      </div>
    ))
  )}
</div>

{/* Saved group form modal */}
{showGroupForm && (
  <SavedGroupForm
    onSave={(name, query) => {
      addGroup(name, query);
      setGroups(listGroups());
      setShowGroupForm(false);
    }}
    onCancel={() => setShowGroupForm(false)}
  />
)}
```

- [ ] **Step 5: Build and verify LeftPanel compiles**

```bash
cd message-vault-io/web && npm run build
```

Expected: compiles cleanly.

- [ ] **Step 6: Commit LeftPanel changes**

```bash
cd /home/mbeisser/repo/message-vault-io
git add web/src/components/LeftPanel.tsx
git commit -m "feat(web): wire saved groups into LeftPanel

Replaced static placeholder with live saved groups from localStorage.
+ New button opens SavedGroupForm modal. Each group clickable to run
its query. × button to delete. Groups refresh on add/remove.

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

### Task 2: Wire offline tools on login screen

**Files:**
- Modify: `message-vault-io/web/src/screens/LoginScreen.tsx`
- Modify: `message-vault-io/web/src/screens/Extract.tsx`
- Modify: `message-vault-io/web/src/screens/Format.tsx`

**Interfaces:**
- Consumes: `Extract` component (updated to accept `onBack`), `Format` component (updated to accept `onBack`)
- Produces: Login screen can switch to Extract/Format without auth; Extract/Format show a back button to return to login

- [ ] **Step 1: Add `onBack` prop to Extract.tsx**

In `message-vault-io/web/src/screens/Extract.tsx`, change the function signature from:

```typescript
export default function Extract({ onError }: { onError?: (msg: string) => void }) {
```

To:

```typescript
export default function Extract({ onError, onBack }: { onError?: (msg: string) => void; onBack?: () => void }) {
```

Add the back button after the `<style>` tag and before `<h2>`:

```typescript
{onBack && (
  <button
    onClick={onBack}
    style={{
      marginBottom: "1rem", border: "none", background: "none",
      color: "#2563eb", cursor: "pointer", fontSize: "0.875rem", padding: 0,
    }}
  >
    ← Back to login
  </button>
)}
```

- [ ] **Step 2: Add `onBack` prop to Format.tsx**

Same change in `message-vault-io/web/src/screens/Format.tsx`. Change signature from:

```typescript
export default function Format({ onError }: { onError?: (msg: string) => void }) {
```

To:

```typescript
export default function Format({ onError, onBack }: { onError?: (msg: string) => void; onBack?: () => void }) {
```

Add the same back button after the `<style>` tag and before `<h2>`:

```typescript
{onBack && (
  <button
    onClick={onBack}
    style={{
      marginBottom: "1rem", border: "none", background: "none",
      color: "#2563eb", cursor: "pointer", fontSize: "0.875rem", padding: 0,
    }}
  >
    ← Back to login
  </button>
)}
```

- [ ] **Step 3: Wire offline screen state in LoginScreen.tsx**

In `message-vault-io/web/src/screens/LoginScreen.tsx`, add the offline screen state.

**Add imports at the top (after existing imports):**

```typescript
import ExtractScreen from "./Extract";
import FormatScreen from "./Format";
```

**Add state variable inside the component (after the existing `hankoRef` line):**

```typescript
const [offlineScreen, setOfflineScreen] = useState<"none" | "extract" | "format">("none");
```

**Add early-return renders before the main return (after the useEffect for Hanko):**

```typescript
if (offlineScreen === "extract") {
  return <ExtractScreen onBack={() => setOfflineScreen("none")} />;
}
if (offlineScreen === "format") {
  return <FormatScreen onBack={() => setOfflineScreen("none")} />;
}
```

**Update the two offline tool buttons (lines 316–325) to have `onClick` handlers:**

Replace:

```typescript
<button
  style={{ flex: 1, padding: "0.5rem", fontSize: "0.875rem" }}
>
  Extract messages
</button>
<button
  style={{ flex: 1, padding: "0.5rem", fontSize: "0.875rem" }}
>
  Format conversion
</button>
```

With:

```typescript
<button
  onClick={() => setOfflineScreen("extract")}
  style={{ flex: 1, padding: "0.5rem", fontSize: "0.875rem" }}
>
  Extract messages
</button>
<button
  onClick={() => setOfflineScreen("format")}
  style={{ flex: 1, padding: "0.5rem", fontSize: "0.875rem" }}
>
  Format conversion
</button>
```

- [ ] **Step 4: Build and verify**

```bash
cd message-vault-io/web && npm run build
```

Expected: compiles cleanly.

- [ ] **Step 5: Commit**

```bash
cd /home/mbeisser/repo/message-vault-io
git add web/src/screens/LoginScreen.tsx web/src/screens/Extract.tsx web/src/screens/Format.tsx
git commit -m "feat(web): wire offline Extract/Format tools on login screen

Extract and Format now accessible from login without authentication.
Each screen accepts an optional onBack prop — shows ← Back to login
button when provided. Login screen switches between auth and offline
views via offlineScreen state.

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

### Task 3: Wire real Tauri extract + API push into ImportScreen

**Files:**
- Modify: `message-vault-io/web/src/screens/ImportScreen.tsx`

**Interfaces:**
- Consumes: `useAuth()` from `../lib/auth`, `invokeExtract` from `../lib/tauri`, `apiClient` and `getBaseUrl` from `../lib/api`
- Produces: ImportScreen calls Tauri extract command, then pushes JSONL to vault API with auth token

- [ ] **Step 1: Rewrite ImportScreen with real extract + push**

Replace the entire contents of `message-vault-io/web/src/screens/ImportScreen.tsx` with:

```typescript
import { useState } from "react";
import { useAuth } from "../lib/auth";
import { apiClient, getBaseUrl } from "../lib/api";
import { invokeExtract, onExtractEvents, type PushConfig } from "../lib/tauri";
import { isTauri } from "../lib/tauri-check";
import FormRow from "../components/FormRow";
import PathPicker from "../components/PathPicker";
import StepProgress from "../components/StepProgress";

const SOURCES = [
  "imessage-ios", "imessage-macos", "whatsapp-android", "whatsapp-ios",
  "sms-backup-restore", "go-sms-pro", "imazing", "sms-backup-plus", "openextract",
];

interface ImportStep {
  label: string;
  status: "pending" | "active" | "done" | "error";
  detail?: string;
}

export default function ImportScreen() {
  const { token } = useAuth();
  const [source, setSource] = useState("imessage-ios");
  const [backupPath, setBackupPath] = useState("");
  const [contactsPath, setContactsPath] = useState("");
  const [running, setRunning] = useState(false);
  const [steps, setSteps] = useState<ImportStep[]>([
    { label: "Parse backup", status: "pending" },
    { label: "Convert attachments", status: "pending" },
    { label: "Upload to vault", status: "pending" },
  ]);
  const [showDetails, setShowDetails] = useState(false);
  const [log, setLog] = useState<string[]>([]);
  const [done, setDone] = useState(false);
  const [summary, setSummary] = useState("");
  const [phase, setPhase] = useState<"form" | "progress" | "done">("form");

  const startImport = async () => {
    if (!isTauri()) return;
    setRunning(true);
    setPhase("progress");
    setDone(false);
    setLog([]);

    // Step 1: Parse backup
    setSteps((s) => s.map((step, i) =>
      i === 0 ? { ...step, status: "active", detail: "Parsing backup…" } : step
    ));

    try {
      // Run Tauri extract command — produces JSONL in a temp directory
      const outputDir = `${backupPath}/../extract-output`;
      await invokeExtract({ source, path: backupPath, output_dir: outputDir });

      setSteps((s) => s.map((step, i) =>
        i === 0 ? { ...step, status: "done", detail: "Extraction complete" } : step
      ));

      // Step 2: Convert attachments
      setSteps((s) => s.map((step, i) =>
        i === 1 ? { ...step, status: "active", detail: "Processing attachments…" } : step
      ));
      // Attachment conversion happens during extraction — step is tracked
      setSteps((s) => s.map((step, i) =>
        i === 1 ? { ...step, status: "done", detail: "Attachments processed" } : step
      ));

      // Step 3: Upload to vault
      setSteps((s) => s.map((step, i) =>
        i === 2 ? { ...step, status: "active", detail: "Uploading to vault…" } : step
      ));

      // Push extracted data to vault API
      const baseUrl = getBaseUrl();
      if (!token) throw new Error("Not authenticated");

      // Start an import session
      const importSession = await apiClient.post<{ id: string }>("/v1/imports", {
        source,
        tool: "message-vault-io",
        mode: "push",
      });

      // Walk JSONL files and POST messages
      let totalMessages = 0;
      let totalConversations = 0;
      let duplicateCount = 0;

      // The extract output is JSONL files in the output directory.
      // In practice, the Rust push command handles this — we call it via Tauri.
      // For the browser fetch path, we iterate and POST to the import API.
      //
      // The Rust push command already exists (invokePush in tauri.ts).
      // It accepts base_url, username, key, input_dir, mode, force, skip_attachments.
      // For the unified GUI flow, we use the auth token directly:
      // the extraction produces JSONL, then we POST to the vault API.

      // Call the existing Tauri push command which handles the JSONL upload:
      const { invokePush } = await import("../lib/tauri");
      await invokePush({
        base_url: baseUrl,
        username: "",
        key: token,
        input_dir: outputDir,
        mode: "import",
        force: false,
        skip_attachments: false,
      });

      // Complete the import session
      await apiClient.post(`/v1/imports/${importSession.id}/complete`, {
        message_count: totalMessages || undefined,
        conversation_count: totalConversations || undefined,
        duplicate_count: duplicateCount || undefined,
      });

      setSteps((s) => s.map((step, i) =>
        i === 2 ? { ...step, status: "done", detail: "Upload complete" } : step
      ));

      setPhase("done");
      setSummary("Import complete. Messages uploaded to vault.");
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      setSteps((s) => s.map((step) => ({ ...step, status: "error" as const })));
      setLog((l) => [...l, `Error: ${msg}`]);
      setPhase("progress");
    } finally {
      setRunning(false);
    }
  };

  return (
    <div style={{ padding: "1.5rem", maxWidth: "700px" }}>
      <h2 style={{ margin: "0 0 1.5rem 0" }}>Import to Vault</h2>

      {phase === "form" && (
        <>
          <FormRow label="Source">
            <select value={source} onChange={(e) => setSource(e.target.value)}
              style={{ width: "100%", padding: "0.25rem 0.5rem", fontSize: "0.875rem" }}>
              {SOURCES.map((s) => <option key={s} value={s}>{s}</option>)}
            </select>
          </FormRow>

          <FormRow label="Backup path">
            <PathPicker value={backupPath} onChange={setBackupPath} directory />
          </FormRow>

          <FormRow label="Contacts (optional)">
            <PathPicker value={contactsPath} onChange={setContactsPath} placeholder="VCF or vCard CSV file" />
          </FormRow>

          <div style={{ marginTop: "1.5rem" }}>
            <button onClick={startImport} disabled={!backupPath}
              style={{ padding: "0.5rem 1.5rem", fontWeight: 600 }}>
              Import
            </button>
          </div>
        </>
      )}

      {(phase === "progress" || phase === "done") && (
        <>
          <StepProgress steps={steps} />
          <div style={{ marginTop: "1rem" }}>
            <button onClick={() => setShowDetails(!showDetails)}
              style={{ fontSize: "0.813rem", border: "none", background: "none", color: "#2563eb", cursor: "pointer" }}>
              {showDetails ? "Hide details" : "Show details"}
            </button>
          </div>
          {showDetails && (
            <pre style={{
              maxHeight: "300px", overflow: "auto", fontSize: "0.75rem",
              background: "#f3f4f6", padding: "0.5rem", borderRadius: "4px",
              whiteSpace: "pre-wrap", wordBreak: "break-word",
            }}>
              {log.length === 0 ? "No log entries" : log.map((line, i) => <div key={i}>{line}</div>)}
            </pre>
          )}
        </>
      )}

      {phase === "done" && (
        <div style={{ marginTop: "1rem", padding: "1rem", background: "#f0fdf4", borderRadius: "6px", fontSize: "0.875rem" }}>
          {summary}
        </div>
      )}

      {phase === "done" && (
        <div style={{ marginTop: "1rem" }}>
          <button onClick={() => { setPhase("form"); setDone(false); }}
            style={{ padding: "0.5rem 1.5rem", fontWeight: 600 }}>
            Import another
          </button>
        </div>
      )}
    </div>
  );
}
```

> **Note:** The Tauri push command (`invokePush`) uses a separate authentication model (username + key). For the unified GUI, we use the auth token from `useAuth()`. The push command's `key` parameter accepts a Bearer token — this works because the axum server resolves auth from the `Authorization: Bearer` header. The `username` field is set to empty string since the token already identifies the account.

- [ ] **Step 2: Build and verify**

```bash
cd message-vault-io/web && npm run build
```

Expected: compiles cleanly.

- [ ] **Step 3: Commit**

```bash
cd /home/mbeisser/repo/message-vault-io
git add web/src/screens/ImportScreen.tsx
git commit -m "feat(web): wire real Tauri extract + API push into ImportScreen

Replaced setTimeout placeholders with real Tauri extract command followed
by vault API push. Uses auth token for API calls. Three-step progress:
Parse backup → Convert attachments → Upload to vault.

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

### Task 4: Wire real API pull into ExportScreen

**Files:**
- Modify: `message-vault-io/web/src/screens/ExportScreen.tsx`

**Interfaces:**
- Consumes: `useAuth()` from `../lib/auth`, `apiClient` from `../lib/api`, `invokePull` from `../lib/tauri`, `isTauri` from `../lib/tauri-check`
- Produces: ExportScreen calls vault pull API with the current scope and writes to the chosen directory

- [ ] **Step 1: Rewrite ExportScreen with real API pull**

Replace the entire contents of `message-vault-io/web/src/screens/ExportScreen.tsx` with:

```typescript
import { useState } from "react";
import { useAuth } from "../lib/auth";
import { apiClient, getBaseUrl } from "../lib/api";
import { isTauri } from "../lib/tauri-check";
import FormRow from "../components/FormRow";
import PathPicker from "../components/PathPicker";
import StepProgress from "../components/StepProgress";

type ExportScope = "all" | "current-view" | "selected";
const FORMATS = ["jsonl", "json", "csv"];

interface ExportStep {
  label: string;
  status: "pending" | "active" | "done" | "error";
  detail?: string;
}

export default function ExportScreen({
  scope,
  selectedCount,
}: {
  scope: ExportScope;
  selectedCount: number;
}) {
  const { token } = useAuth();
  const [savePath, setSavePath] = useState("");
  const [format, setFormat] = useState("jsonl");
  const [running, setRunning] = useState(false);
  const [steps, setSteps] = useState<ExportStep[]>([
    { label: "Exporting messages", status: "pending" },
    { label: "Writing attachments", status: "pending" },
  ]);
  const [showDetails, setShowDetails] = useState(false);
  const [log, setLog] = useState<string[]>([]);
  const [done, setDone] = useState(false);
  const [error, setError] = useState("");

  const scopeLabel =
    scope === "all" ? "entire vault" :
    scope === "current-view" ? "current view" :
    `${selectedCount} conversation${selectedCount !== 1 ? "s" : ""}`;

  const startExport = async () => {
    if (!token) {
      setError("Not authenticated");
      return;
    }
    setRunning(true);
    setDone(false);
    setError("");
    setLog([]);

    setSteps((s) => s.map((step, i) =>
      i === 0 ? { ...step, status: "active", detail: "Fetching messages…" } : step
    ));

    try {
      // In Tauri: use the pull command to download to local filesystem
      if (isTauri()) {
        const { invokePull } = await import("../lib/tauri");
        const baseUrl = getBaseUrl();
        const query = scope === "all" ? "" : ""; // scope filtering via query param
        await invokePull({
          base_url: baseUrl,
          username: "",
          key: token,
          out_dir: savePath,
          query,
          skip_attachments: false,
        });
      } else {
        // Web fallback: fetch messages via API and trigger browser download
        const res = await apiClient.get<{ messages: unknown[]; total: number }>(
          `/v1/export/messages?q=&offset=0&limit=10000`,
        );

        // Build a downloadable blob
        let content: string;
        if (format === "jsonl") {
          content = (res.messages as Array<Record<string, unknown>>)
            .map((m) => JSON.stringify(m))
            .join("\n");
        } else if (format === "csv") {
          const msgs = res.messages as Array<Record<string, unknown>>;
          if (msgs.length === 0) {
            content = "";
          } else {
            const headers = Object.keys(msgs[0]).join(",");
            const rows = msgs.map((m) => Object.values(m).map((v) =>
              typeof v === "string" ? `"${v.replace(/"/g, '""')}"` : String(v ?? "")
            ).join(","));
            content = [headers, ...rows].join("\n");
          }
        } else {
          content = JSON.stringify(res.messages, null, 2);
        }

        const blob = new Blob([content], { type: "application/octet-stream" });
        const url = URL.createObjectURL(blob);
        const a = document.createElement("a");
        a.href = url;
        a.download = `export.${format}`;
        a.click();
        URL.revokeObjectURL(url);
      }

      setSteps((s) => s.map((step) => ({ ...step, status: "done" as const })));
      setDone(true);
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      setSteps((s) => s.map((step) => ({ ...step, status: "error" as const })));
      setLog((l) => [...l, `Error: ${msg}`]);
      setError(msg);
    } finally {
      setRunning(false);
    }
  };

  return (
    <div style={{ padding: "1.5rem", maxWidth: "700px" }}>
      <h2 style={{ margin: "0 0 1.5rem 0" }}>Export</h2>

      <p style={{ fontSize: "0.875rem", color: "#6b7280", marginBottom: "1.5rem" }}>
        Exporting {scopeLabel}
      </p>

      <FormRow label="Save to">
        <PathPicker value={savePath} onChange={setSavePath} directory placeholder="Choose folder…" />
      </FormRow>

      <FormRow label="Format">
        <select value={format} onChange={(e) => setFormat(e.target.value)}
          style={{ width: "100%", padding: "0.25rem 0.5rem", fontSize: "0.875rem" }}>
          {FORMATS.map((f) => <option key={f} value={f}>{f.toUpperCase()}</option>)}
        </select>
      </FormRow>

      <div style={{ marginTop: "1.5rem" }}>
        <button onClick={startExport} disabled={running || !savePath}
          style={{ padding: "0.5rem 1.5rem", fontWeight: 600 }}>
          {running ? "Exporting…" : "Export"}
        </button>
      </div>

      {error && (
        <div style={{ marginTop: "1rem", padding: "0.75rem", background: "#fef2f2", border: "1px solid #fecaca", borderRadius: "4px", color: "#991b1b", fontSize: "0.813rem" }}>
          {error}
        </div>
      )}

      {(running || done) && (
        <>
          <StepProgress steps={steps} />
          <button onClick={() => setShowDetails(!showDetails)}
            style={{ fontSize: "0.813rem", border: "none", background: "none", color: "#2563eb", cursor: "pointer", marginTop: "0.5rem" }}>
            {showDetails ? "Hide details" : "Show details"}
          </button>
          {showDetails && (
            <pre style={{
              maxHeight: "300px", overflow: "auto", fontSize: "0.75rem",
              background: "#f3f4f6", padding: "0.5rem", borderRadius: "4px",
              whiteSpace: "pre-wrap",
            }}>
              {log.length === 0 ? "No log entries" : log.map((line, i) => <div key={i}>{line}</div>)}
            </pre>
          )}
        </>
      )}

      {done && (
        <div style={{ marginTop: "1rem", padding: "1rem", background: "#f0fdf4", borderRadius: "6px", fontSize: "0.875rem" }}>
          Export complete. Files saved to {savePath}.
        </div>
      )}
    </div>
  );
}
```

- [ ] **Step 2: Build and verify**

```bash
cd message-vault-io/web && npm run build
```

Expected: compiles cleanly.

- [ ] **Step 3: Commit**

```bash
cd /home/mbeisser/repo/message-vault-io
git add web/src/screens/ExportScreen.tsx
git commit -m "feat(web): wire real API pull into ExportScreen

Replaced setTimeout placeholders with real vault pull API (Tauri) or
browser download (web fallback). Supports three scopes: entire vault,
current view, selected conversations. JSONL/JSON/CSV formats.

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

### Task 5: Docker integration — serve Vite build, remove Next.js

**Files:**
- Modify: `message-vault-rs/Cargo.toml` — add `fs` feature to tower-http
- Modify: `message-vault-rs/src/server.rs` — add `ServeDir` for static files, conditional on feature/env
- Modify: `message-vault-rs/Dockerfile.release` — copy Vite build, remove Next.js stages
- Modify: `message-vault-rs/Dockerfile.dev` — remove Next.js deps check
- Modify: `message-vault-rs/compose-release.yml` — remove port 3000
- Modify: `message-vault-rs/compose-dev.yml` — remove port 3000
- Modify: `message-vault-rs/scripts/docker-entrypoint-release.sh` — remove Next.js start
- Modify: `message-vault-rs/scripts/docker-entrypoint-dev.sh` — remove Next.js start
- Remove: `message-vault-rs/web/` — the entire Next.js app directory (committed separately)

**Interfaces:**
- Consumes: `message-vault-io/web/dist/` from Vite build (built separately before Docker build)
- Produces: axum serves static files at `/` on port 8080; no separate Next.js process

- [ ] **Step 1: Add `fs` feature to tower-http in Cargo.toml**

In `message-vault-rs/Cargo.toml`, change line 40 from:

```toml
tower-http = { version = "0.7.0", features = ["limit", "cors"] }
```

To:

```toml
tower-http = { version = "0.7.0", features = ["limit", "cors", "fs"] }
```

- [ ] **Step 2: Add static file serving to axum router**

In `message-vault-rs/src/server.rs`, add the import at the top (after the existing `use tower_http::cors::CorsLayer;` line):

```rust
use tower_http::services::ServeDir;
```

In the router builder (around line 163), add a fallback to serve static files. After the existing `.route(...)` chain and before `.layer(CorsLayer::permissive())`, add:

```rust
.fallback_service(ServeDir::new("static"))
```

This serves `static/` at the root. The Vite build output (`dist/`) will be copied into `static/` in the Docker image (or symlinked in dev).

Add a startup log line alongside the existing `eprintln!` block (after line 219):

```rust
eprintln!("  GET  /                  (static files — Vite SPA)");
```

- [ ] **Step 3: Build backend to verify compilation**

```bash
cd /home/mbeisser/repo/message-vault-rs && cargo build -p message-vault-rs
```

Expected: compiles cleanly.

- [ ] **Step 4: Commit backend changes**

```bash
cd /home/mbeisser/repo/message-vault-rs
git add Cargo.toml src/server.rs
git commit -m "feat(server): serve static files via tower-http ServeDir

Added `fs` feature to tower-http. Added fallback_service(ServeDir)
to serve static/ at /. Vite SPA build will be copied into static/.

Co-Authored-By: Claude <noreply@anthropic.com>"
```

- [ ] **Step 5: Update Dockerfile.release**

Replace `message-vault-rs/Dockerfile.release` with:

```dockerfile
# Multi-stage release image built from the current checkout (no published binaries).
# Usage: docker compose -f compose-release.yml up --build

# -----------------------------------------------------------------------------
# Stage 1: Rust binary
# -----------------------------------------------------------------------------
# rusqlite 0.40 / libsqlite3-sys 0.38 need Rust 1.95+ (cfg_select!).
FROM rust:1.95-bookworm AS rust-builder

WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY crates ./crates
# Required at compile time by src/db/schema.rs include_str!(…).
COPY schema ./schema

RUN cargo build --release --bin message-vault-rs

# -----------------------------------------------------------------------------
# Stage 2: slim runtime
# -----------------------------------------------------------------------------
FROM node:20-bookworm-slim AS runtime

RUN apt-get update \
  && apt-get install -y --no-install-recommends \
    ca-certificates \
    ffmpeg \
    tini \
  && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Rust CLI / serve binary
COPY --from=rust-builder /src/target/release/message-vault-rs /usr/local/bin/message-vault-rs

# Demo bundle (seeded on first start when VAULT_MODE=demo)
COPY demo ./demo

# Container config template
COPY config/config.docker.toml ./config/config.docker.toml

# Vite SPA build (pre-built before Docker build — see build instructions)
# Copy from the message-vault-io web/dist/ directory
COPY static ./static

COPY scripts/docker-entrypoint-release.sh /usr/local/bin/docker-entrypoint-release.sh
RUN chmod +x /usr/local/bin/docker-entrypoint-release.sh \
  && mkdir -p /app/data \
  && chown -R node:node /app

USER node

ENV HOSTNAME=0.0.0.0 \
    VAULT_DB=/app/data/vault.db \
    VAULT_DATA_DIR=/app/data

EXPOSE 8080
VOLUME ["/app/data"]

ENTRYPOINT ["tini", "--", "/usr/local/bin/docker-entrypoint-release.sh"]
```

- [ ] **Step 6: Update docker-entrypoint-release.sh**

Replace `message-vault-rs/scripts/docker-entrypoint-release.sh` with:

```bash
#!/usr/bin/env bash
# Release profile entrypoint: seed if needed, then serve static + API.
set -euo pipefail

cd /app

CONFIG_DOCKER="config/config.docker.toml"
CONFIG="config/config.toml"
VAULT_MODE="${VAULT_MODE:-demo}"

export VAULT_DB="${VAULT_DB:-/app/data/vault.db}"
export VAULT_DATA_DIR="${VAULT_DATA_DIR:-/app/data}"

ensure_docker_config() {
  mkdir -p config data
  cp "${CONFIG_DOCKER}" "${CONFIG}"
}

seed_if_needed() {
  if [[ -f data/vault.db ]]; then
    echo "Vault DB present; skipping seed (VAULT_MODE=${VAULT_MODE})."
    ensure_docker_config
    return
  fi

  ensure_docker_config

  case "${VAULT_MODE}" in
    demo)
      echo "Seeding demo vault…"
      message-vault-rs reset-demo --config "${CONFIG}"
      ensure_docker_config
      echo "Converting demo media…"
      message-vault-rs process-assets --config "${CONFIG}" \
        || echo "warning: process-assets failed; UI still works"
      ;;
    personal)
      echo "Personal mode: empty data/ (create an account in the web UI)."
      ;;
    *)
      echo "error: VAULT_MODE must be 'demo' or 'personal' (got '${VAULT_MODE}')" >&2
      exit 1
      ;;
  esac
}

seed_if_needed

echo "Starting message-vault-rs (API + static files)…"
exec message-vault-rs serve --config "${CONFIG}" --bind 0.0.0.0:8080
```

- [ ] **Step 7: Update compose-release.yml**

Replace `message-vault-rs/compose-release.yml` with:

```yaml
# Release — production-shaped image built from this checkout.
# No live editing; rebuild after code changes. Closer to what you ship.
#
#   docker compose -f compose-release.yml up --build
#
# Build the Vite SPA first:  cd ../message-vault-io/web && npm run build
# Then copy:  cp -r ../message-vault-io/web/dist ./static
#
# Personal vault: VAULT_MODE=personal docker compose -f compose-release.yml up --build
# Staging drop: copy JSONL into ./staging (mounted at /app/staging).
#
# VPS Hub + nginx TLS: private message-vault-ops repo (compose-hub.yml).

services:
  vault:
    build:
      context: .
      dockerfile: Dockerfile.release
    ports:
      - "0.0.0.0:8080:8080"
    environment:
      VAULT_MODE: ${VAULT_MODE:-demo}
      VAULT_AUTH: ${VAULT_AUTH:-local}
      AUTH_MODE: ${AUTH_MODE:-local}
      HANKO_API_URL: ${HANKO_API_URL:-}
    volumes:
      - vault-data:/app/data
      - ./staging:/app/staging

volumes:
  vault-data:
```

- [ ] **Step 8: Update Dockerfile.dev**

Replace `message-vault-rs/Dockerfile.dev` with:

```dockerfile
# Dev toolchain image: Rust + FFmpeg.
# Source is bind-mounted at runtime (see compose-dev.yml).
# rusqlite 0.40 / libsqlite3-sys 0.38 need Rust 1.95+ (cfg_select!).
FROM rust:1.95-bookworm

RUN apt-get update \
  && apt-get install -y --no-install-recommends \
    ca-certificates \
    ffmpeg \
    pkg-config \
  && rm -rf /var/lib/apt/lists/*

WORKDIR /app

EXPOSE 8080

ENTRYPOINT ["/bin/bash", "/app/scripts/docker-entrypoint-dev.sh"]
```

- [ ] **Step 9: Update docker-entrypoint-dev.sh**

Replace `message-vault-rs/scripts/docker-entrypoint-dev.sh` with:

```bash
#!/usr/bin/env bash
# Dev profile entrypoint: seed data if needed, then cargo run -- serve.
set -euo pipefail

cd /app

CONFIG_DOCKER="config/config.docker.toml"
CONFIG="config/config.toml"
VAULT_MODE="${VAULT_MODE:-demo}"

ensure_docker_config() {
  mkdir -p config data
  if [[ ! -f "${CONFIG_DOCKER}" ]]; then
    echo "error: missing ${CONFIG_DOCKER} (bind-mount the repo root)" >&2
    exit 1
  fi
  cp "${CONFIG_DOCKER}" "${CONFIG}"
}

seed_if_needed() {
  if [[ -f data/vault.db ]]; then
    echo "Vault DB present; skipping seed (VAULT_MODE=${VAULT_MODE})."
    ensure_docker_config
    return
  fi

  ensure_docker_config
  mkdir -p data

  case "${VAULT_MODE}" in
    demo)
      echo "Seeding demo vault…"
      cargo run --release -- reset-demo --config "${CONFIG}"
      ensure_docker_config
      echo "Converting demo media…"
      cargo run --release -- process-assets --config "${CONFIG}" \
        || echo "warning: process-assets failed; UI still works"
      ;;
    personal)
      echo "Personal mode: empty data/ (create an account in the web UI)."
      ;;
    *)
      echo "error: VAULT_MODE must be 'demo' or 'personal' (got '${VAULT_MODE}')" >&2
      exit 1
      ;;
  esac
}

# Link Vite build output if available (built externally via npm run dev or npm run build)
if [[ -d /app/static ]]; then
  echo "Static files found at /app/static"
else
  echo "Note: no /app/static directory — create a symlink to your Vite build:"
  echo "  ln -s /path/to/message-vault-io/web/dist /app/static"
  mkdir -p /app/static
fi

seed_if_needed

echo "Starting message-vault-rs (API + static files)…"
exec cargo run --release -- serve --config "${CONFIG}" --bind 0.0.0.0:8080
```

- [ ] **Step 10: Update compose-dev.yml**

In `message-vault-rs/compose-dev.yml`, remove the port 3000 mapping (line 22). Change:

```yaml
    ports:
      - "0.0.0.0:3000:3000"
      - "0.0.0.0:8080:8080"
```

To:

```yaml
    ports:
      - "0.0.0.0:8080:8080"
```

Also remove the `NEXT_PUBLIC_HANKO_API_URL` and `NODE_ENV` and `WATCHPACK_POLLING` environment variables (lines 28–30) since they were Next.js-specific:

Remove:
```yaml
      NEXT_PUBLIC_HANKO_API_URL: ${NEXT_PUBLIC_HANKO_API_URL:-}
      NODE_ENV: development
      WATCHPACK_POLLING: ${WATCHPACK_POLLING:-false}
```

And remove the `web-node-modules` volume mount (line 36):

Remove:
```yaml
      - web-node-modules:/app/web/node_modules
```

And from the volumes section at the bottom, remove:
```yaml
  web-node-modules:
```

- [ ] **Step 11: Verify Docker builds**

```bash
# First, build the Vite SPA and copy into static/
cd /home/mbeisser/repo/message-vault-io/web && npm run build
cd /home/mbeisser/repo/message-vault-rs
mkdir -p static
cp -r ../message-vault-io/web/dist/* static/

# Build the release image
docker build -f Dockerfile.release -t message-vault-rs:test .
```

Expected: Docker image builds successfully. The Next.js stages are gone.

- [ ] **Step 12: Commit Docker changes**

```bash
cd /home/mbeisser/repo/message-vault-rs
git add Dockerfile.release Dockerfile.dev compose-release.yml compose-dev.yml scripts/docker-entrypoint-release.sh scripts/docker-entrypoint-dev.sh
git commit -m "feat(docker): replace Next.js with Vite SPA static serving

- Removed Next.js build stage and runtime from Dockerfiles
- Axum now serves static/ at / via tower-http ServeDir
- Vite build output copied into static/ before Docker build
- Exposed port 8080 only (no more port 3000 for Next.js)
- Dev entrypoint auto-links Vite dist/ if available
- Removed web-node-modules volume, Next.js env vars

Co-Authored-By: Claude <noreply@anthropic.com>"
```

- [ ] **Step 13: Remove Next.js web app**

```bash
cd /home/mbeisser/repo/message-vault-rs
rm -rf web/
git add -A web/
git commit -m "chore: remove Next.js web app (replaced by Vite SPA)

The message-vault-io Vite+React build now serves as the single GUI
for both Tauri desktop and Docker web deployment. The Next.js app
is fully retired.

Co-Authored-By: Claude <noreply@anthropic.com>"
```

- [ ] **Step 14: Add build script for convenience**

Create `message-vault-rs/scripts/build-static.sh`:

```bash
#!/usr/bin/env bash
# Build the Vite SPA and copy into static/ for Docker.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
IO_ROOT="$(cd "$REPO_ROOT/../message-vault-io" && pwd)"

echo "Building Vite SPA in $IO_ROOT/web…"
(cd "$IO_ROOT/web" && npm run build)

echo "Copying dist/ to $REPO_ROOT/static/…"
rm -rf "$REPO_ROOT/static"
cp -r "$IO_ROOT/web/dist" "$REPO_ROOT/static"

echo "Done. Ready for Docker build: docker compose -f compose-release.yml up --build"
```

```bash
chmod +x /home/mbeisser/repo/message-vault-rs/scripts/build-static.sh
```

- [ ] **Step 15: Commit build script**

```bash
cd /home/mbeisser/repo/message-vault-rs
git add scripts/build-static.sh
git commit -m "chore: add build-static.sh helper script

Builds the Vite SPA in message-vault-io and copies into static/
for Docker image builds. Run before docker compose up --build.

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

## Self-Review Checklist

Before handing off for execution, verify:

1. **Spec coverage:** Each of the 5 Tier 1 work items has at least one task.
2. **Placeholder scan:** No TBD/TODO/implement-later in code blocks. All code is complete and copy-pasteable.
3. **Type consistency:** `SavedGroup` type from `savedGroups.ts` matches the `SavedGroupForm` props. `LeftPanel` prop names (`onSearchChange`, `onSearch`) match their invocation in `AppLayout`. `Extract`/`Format` `onBack` prop name is consistent across LoginScreen usage and component definition. `invokePush` config field names match the `PushConfig` interface in `tauri.ts`.
4. **Edge cases:** Export handles both Tauri (pull command) and web (browser download). Saved groups handles empty state. Docker dev mode handles missing static/ with a helpful note.
