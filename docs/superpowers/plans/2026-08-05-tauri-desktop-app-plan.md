# Tauri Desktop App Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the Slint desktop GUI with a Tauri v2 app that reuses all existing Rust extraction, format conversion, and HTTP client code.

**Architecture:** A Tauri v2 shell (`src-tauri/`) hosts a React + Vite SPA (`web/`). The frontend calls Tauri commands via `invoke()`; the Rust backend wraps existing exporter/format/push/pull crate `run()` functions on background threads and streams progress events to the frontend. The vault server (message-vault-rs, Docker) is unchanged — the desktop app communicates with it over HTTP.

**Tech Stack:** Tauri v2, React 19, Vite, TypeScript, Rust 1.85+ (edition 2024). Existing crates: message-vault-io-core, 7 exporter crates, vault-push, vault-pull, message-reexport.

## Global Constraints

- Rust edition 2024 throughout (match existing workspace)
- Vault server is always Docker — desktop app never accesses SQLite directly
- Slint GUI (`crates/message-vault-io-gui/`) remains working until Phase 3 completion
- ffmpeg is a runtime dependency, detected at startup
- Exporters linked with `default-features = false` (same pattern as Slint GUI)
- Progress events flow through Tauri events (not polling)
- `export.ini` persistence reuses existing `message-vault-io-core::ExportIniState`

---

## File Structure

```
message-vault-io/                          (root, workspace)
  Cargo.toml                               MODIFY: add src-tauri to workspace members
  src-tauri/                               CREATE: Tauri Rust backend
    Cargo.toml                             depends on tauri, message-vault-io-core, all exporters,
                                           vault-push, vault-pull, message-reexport
    tauri.conf.json                        window title "Message Vault", app identifier,
                                           capabilities for filesystem + shell
    build.rs                               standard tauri-build::build()
    capabilities/
      default.json                         filesystem access, shell (ffmpeg), dialog
    src/
      main.rs                              Tauri entry: build app, register commands, manage state
      commands/
        mod.rs                             re-export all command modules
        extract.rs                         Tauri command wrapping exporter run()
        format.rs                          Tauri command wrapping message-reexport
        push.rs                            Tauri command wrapping vault-push::run()
        pull.rs                            Tauri command wrapping vault-pull::run()
        contacts.rs                        Tauri command wrapping message-contacts
      state.rs                             AppState: CancelFlag, export.ini persistence
  web/                                     CREATE: React + Vite SPA
    package.json                           react, react-dom, @tauri-apps/api, typescript
    vite.config.ts                         Vite config with Tauri plugin
    tsconfig.json                          TypeScript config
    index.html                             entry HTML
    src/
      main.tsx                             React entry point
      App.tsx                              Router: Home | Extract | Format | Push | Pull | Settings
      screens/
        Home.tsx                           Dashboard: recent exports, vault status, quick actions
        Extract.tsx                        Exporter picker, path, media options, progress
        Format.tsx                         Input dir, target format picker, progress
        Push.tsx                           Vault URL, API key, source name, progress
        Pull.tsx                           Vault URL, API key, query, output dir, progress
        Settings.tsx                       ffmpeg path, default dirs, vault credentials
      components/
        ProgressBar.tsx                    Determinate/indeterminate bar + log tail
        FormRow.tsx                        Label + input with fixed label column
        LogViewer.tsx                      Scrollable monospace log
        PathPicker.tsx                     Text input + browse button (Tauri dialog)
      lib/
        tauri.ts                           Typed invoke wrappers for each command
        types.ts                           Shared TypeScript types (ExporterConfig, Progress, etc.)
```

| File | Responsibility |
|------|---------------|
| `src-tauri/Cargo.toml` | Declare all Rust dependencies including Tauri and workspace crates |
| `src-tauri/tauri.conf.json` | Window config, app identifier, capabilities, bundle settings |
| `src-tauri/src/main.rs` | Register commands, manage `Arc<Mutex<AppState>>`, run Tauri app |
| `src-tauri/src/state.rs` | `AppState` struct: `CancelFlag`, `ExportIniState` load/save |
| `src-tauri/src/commands/extract.rs` | `#[tauri::command]` wrapper: deserialize `ExporterConfig`, call `run_exporter()` on a thread, forward `ProcessEvent` via Tauri event channel |
| `src-tauri/src/commands/format.rs` | `#[tauri::command]` wrapper around `message_reexport::convert_directory()` |
| `src-tauri/src/commands/push.rs` | `#[tauri::command]` wrapper around `vault_push::run()` |
| `src-tauri/src/commands/pull.rs` | `#[tauri::command]` wrapper around `vault_pull::run()` |
| `src-tauri/src/commands/contacts.rs` | `#[tauri::command]` wrapper: parse VCF/CSV, return contact list |
| `web/src/App.tsx` | Hash router mapping screens, shared layout (sidebar or tab bar) |
| `web/src/screens/Home.tsx` | Dashboard with status cards and quick-action buttons |
| `web/src/screens/Extract.tsx` | Full extraction form: exporter picker, path, media opts, run/cancel |
| `web/src/lib/tauri.ts` | Typed `invoke()` wrappers returning `Promise<T>`, progress event listener helper |

---

## Phase 1: Scaffold

### Task 1: Initialize Tauri project structure

**Files:**
- Create: `src-tauri/Cargo.toml`
- Create: `src-tauri/tauri.conf.json`
- Create: `src-tauri/build.rs`
- Create: `src-tauri/capabilities/default.json`
- Create: `src-tauri/src/main.rs`
- Create: `src-tauri/src/state.rs`
- Modify: `Cargo.toml` (workspace root — add `src-tauri` to members)

**Interfaces:**
- Produces: `AppState` struct in `state.rs` with `cancel_flag: CancelFlag`, `ini: ExportIniState`
- Produces: Tauri app builder in `main.rs` with `app.manage(AppState::default())`
- Consumes: `message-vault-io-core` (CancelFlag, ExportIniState), `tauri` crate

- [ ] **Step 1: Add src-tauri to workspace members**

```toml
# Cargo.toml (root) — add to members array:
"src-tauri",
```

- [ ] **Step 2: Create src-tauri/Cargo.toml**

```toml
[package]
name = "message-vault-io-tauri"
version = "0.1.0"
edition = "2024"
description = "Tauri desktop app for message-vault-io"

[lib]
name = "message_vault_io_tauri_lib"
crate-type = ["lib", "cdylib", "staticlib"]

[build-dependencies]
tauri-build = { version = "2", features = [] }

[dependencies]
tauri = { version = "2", features = [] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
anyhow = "1.0"
message-vault-io-core = { path = "../crates/message-vault-io-core" }
```

- [ ] **Step 3: Create src-tauri/build.rs**

```rust
fn main() {
    tauri_build::build()
}
```

- [ ] **Step 4: Create src-tauri/tauri.conf.json**

```json
{
  "$schema": "https://raw.githubusercontent.com/nicoverbruggen/tauri-v2-schema/refs/heads/main/schema.json",
  "productName": "Message Vault",
  "version": "0.1.0",
  "identifier": "io.bitrealm.message-vault",
  "build": {
    "frontendDist": "../web/dist",
    "devUrl": "http://localhost:5173",
    "beforeDevCommand": "cd web && npm run dev",
    "beforeBuildCommand": "cd web && npm run build"
  },
  "app": {
    "withGlobalTauri": true,
    "windows": [
      {
        "title": "Message Vault",
        "width": 1024,
        "height": 768,
        "resizable": true,
        "fullscreen": false
      }
    ],
    "security": {
      "csp": null
    }
  },
  "bundle": {
    "active": true,
    "targets": "all",
    "icon": [
      "icons/32x32.png",
      "icons/128x128.png",
      "icons/128x128@2x.png",
      "icons/icon.icns",
      "icons/icon.ico"
    ]
  }
}
```

- [ ] **Step 5: Create src-tauri/capabilities/default.json**

```json
{
  "$schema": "../gen/schemas/desktop-schema.json",
  "identifier": "default",
  "description": "Default capability for the main window",
  "windows": ["main"],
  "permissions": [
    "core:default",
    "dialog:default",
    "dialog:allow-open",
    "dialog:allow-save",
    "shell:default",
    "shell:allow-open"
  ]
}
```

- [ ] **Step 6: Create src-tauri/src/main.rs**

```rust
// Prevents additional console window on Windows in release
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod state;

use state::AppState;
use std::sync::{Arc, Mutex};

fn main() {
    tauri::Builder::default()
        .manage(Arc::new(Mutex::new(AppState::new())))
        .invoke_handler(tauri::generate_handler![
            commands::extract::extract,
            commands::extract::cancel,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

- [ ] **Step 7: Create src-tauri/src/state.rs**

```rust
use message_vault_io_core::{CancelFlag, ExportIniState};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

pub struct AppState {
    pub cancel_flag: CancelFlag,
    pub ini: ExportIniState,
}

impl AppState {
    pub fn new() -> Self {
        // load_or_default returns (state, form, error_message)
        let (ini, _form, _load_error) = ExportIniState::load_or_default();
        Self {
            cancel_flag: Arc::new(AtomicBool::new(false)),
            ini,
        }
    }
}
```

- [ ] **Step 8: Create placeholder src-tauri/src/commands/mod.rs**

```rust
pub mod extract;
```

- [ ] **Step 9: Create placeholder src-tauri/src/commands/extract.rs**

```rust
use std::sync::{Arc, Mutex};
use std::sync::atomic::Ordering;
use crate::state::AppState;

#[tauri::command]
pub async fn cancel(state: tauri::State<'_, Arc<Mutex<AppState>>>) -> Result<(), String> {
    let state = state.lock().map_err(|e| e.to_string())?;
    state.cancel_flag.store(true, Ordering::SeqCst);
    Ok(())
}

#[tauri::command]
pub async fn extract(
    _state: tauri::State<'_, Arc<Mutex<AppState>>>,
    _source: String,
    _path: String,
    _output_dir: String,
) -> Result<(), String> {
    Err("not implemented".to_string())
}
```

- [ ] **Step 10: Run cargo check**

```bash
cargo check --workspace
```

Expected: The workspace compiles. The new `src-tauri` crate is resolved but the placeholder command is unused.

- [ ] **Step 11: Commit**

```bash
git add Cargo.toml src-tauri/
git commit -m "feat(tauri): scaffold project structure with placeholder command

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

### Task 2: Initialize React + Vite frontend

**Files:**
- Create: `web/package.json`
- Create: `web/vite.config.ts`
- Create: `web/tsconfig.json`
- Create: `web/index.html`
- Create: `web/src/main.tsx`
- Create: `web/src/App.tsx`
- Create: `web/src/lib/tauri.ts`
- Create: `web/src/lib/types.ts`

**Interfaces:**
- Produces: `invokeExtract`, `invokeCancel`, `onProgress` helpers in `lib/tauri.ts`
- Produces: `ExtractConfig`, `ProgressEvent` types in `lib/types.ts`
- Consumes: `@tauri-apps/api` (`invoke`, `listen`)

- [ ] **Step 1: Create web/package.json**

```json
{
  "name": "message-vault-io-web",
  "private": true,
  "version": "0.1.0",
  "type": "module",
  "scripts": {
    "dev": "vite",
    "build": "tsc && vite build",
    "preview": "vite preview"
  },
  "dependencies": {
    "@tauri-apps/api": "^2",
    "react": "^19.2.4",
    "react-dom": "^19.2.4"
  },
  "devDependencies": {
    "@types/react": "^19",
    "@types/react-dom": "^19",
    "@vitejs/plugin-react": "^4",
    "typescript": "^5",
    "vite": "^6"
  }
}
```

- [ ] **Step 2: Create web/vite.config.ts**

```typescript
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: {
    port: 5173,
    strictPort: true,
  },
});
```

- [ ] **Step 3: Create web/tsconfig.json**

```json
{
  "compilerOptions": {
    "target": "ES2020",
    "module": "ESNext",
    "moduleResolution": "bundler",
    "jsx": "react-jsx",
    "strict": true,
    "esModuleInterop": true,
    "skipLibCheck": true,
    "forceConsistentCasingInFileNames": true,
    "resolveJsonModule": true,
    "isolatedModules": true,
    "noEmit": true
  },
  "include": ["src"]
}
```

- [ ] **Step 4: Create web/index.html**

```html
<!DOCTYPE html>
<html lang="en">
  <head>
    <meta charset="UTF-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    <title>Message Vault</title>
  </head>
  <body>
    <div id="root"></div>
    <script type="module" src="/src/main.tsx"></script>
  </body>
</html>
```

- [ ] **Step 5: Create web/src/main.tsx**

```tsx
import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
```

- [ ] **Step 6: Create web/src/App.tsx**

```tsx
function App() {
  return (
    <div style={{ padding: "2rem", fontFamily: "system-ui" }}>
      <h1>Message Vault</h1>
      <p>Tauri desktop app — scaffold</p>
    </div>
  );
}

export default App;
```

- [ ] **Step 7: Create web/src/lib/types.ts**

```typescript
export interface ExtractConfig {
  source: string;
  path: string;
  output_dir: string;
  media: MediaConfig;
}

export interface MediaConfig {
  mode: "copy" | "convert" | "compress" | "none";
  convert_resolution?: number;
  convert_fps?: number;
}

export interface ProgressEvent {
  kind: string;
  message: string;
  current: number;
  total?: number;
}
```

- [ ] **Step 8: Create web/src/lib/tauri.ts**

```typescript
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { ExtractConfig, ProgressEvent } from "./types";

export async function invokeExtract(config: ExtractConfig): Promise<void> {
  return invoke("extract", { config: JSON.stringify(config) });
}

export async function invokeCancel(): Promise<void> {
  return invoke("cancel");
}

export function onProgress(callback: (event: ProgressEvent) => void): Promise<UnlistenFn> {
  return listen<ProgressEvent>("extract:progress", (event) => {
    callback(event.payload);
  });
}
```

- [ ] **Step 9: Install dependencies and verify build**

```bash
cd web && npm ci && npm run build
```

Expected: Vite builds successfully, output in `web/dist/`.

- [ ] **Step 10: Commit**

```bash
git add web/
git commit -m "feat(tauri): scaffold React + Vite frontend

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

### Task 3: Wire Tauri dev mode and verify launch

**Files:**
- Modify: `src-tauri/Cargo.toml` (add missing Tauri features for dev)
- No new files

**Interfaces:**
- Consumes: `web/` built output from Task 2
- Produces: running Tauri dev app window showing "Message Vault"

- [ ] **Step 1: Install Tauri CLI**

```bash
cargo install tauri-cli --version "^2"
```

- [ ] **Step 2: Run Tauri in dev mode**

```bash
cargo tauri dev
```

Expected: Tauri compiles the Rust backend, starts Vite dev server on :5173, opens a native window displaying "Message Vault — Tauri desktop app — scaffold".

- [ ] **Step 3: Verify hot reload**

Edit `web/src/App.tsx` — change the heading text. Save. Expected: the window updates immediately.

- [ ] **Step 4: Verify Tauri invoke works from frontend**

Add to `App.tsx`:
```tsx
import { invoke } from "@tauri-apps/api/core";

function App() {
  const [msg, setMsg] = React.useState("");
  React.useEffect(() => {
    invoke("cancel").then(
      () => setMsg("invoke works"),
      (err) => setMsg(`invoke works (expected err: ${err})`),
    );
  }, []);
  return <div style={{padding: "2rem"}}><h1>Message Vault</h1><p>{msg}</p></div>;
}
```

Expected: Window shows "invoke works (expected err: not implemented)" — proves the frontend can call Rust commands.

- [ ] **Step 5: Revert the test code and commit**

```bash
git add -A
git commit -m "feat(tauri): verify dev mode and frontend-backend bridge

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

## Phase 2: Prove the extraction pattern

### Task 4: Implement extract command with SMS Backup & Restore exporter

**Files:**
- Modify: `src-tauri/Cargo.toml` (add sms-backup-restore-exporter dependency)
- Rewrite: `src-tauri/src/commands/extract.rs`
- Modify: `src-tauri/src/main.rs` (update command registration)
- Create: `src-tauri/src/commands/events.rs`

**Interfaces:**
- Consumes: `sms_backup_restore_exporter::run(config: &ExporterConfig) -> Result<RunResult>` — actual signature
- Consumes: `message_vault_io_core::{ExporterConfig, SourceConfig, SmsBackupRestoreConfig, LogSink, CancelFlag, RunResult}`
- Produces: Tauri events `extract:log` (String), `extract:finished` (String), `extract:error` ({detail, user_message})
- Produces: `extract` command returns immediately, work runs on `std::thread`, progress via Tauri events
- Design note: The Slint GUI has `jobs.rs` with `run_exporter()` dispatching to all 7 exporters and `prepare_config()` wiring cancel+log. The Tauri command replicates this dispatch inline rather than depending on `message-vault-io-gui` (which pulls in slint). Once the Slint GUI is removed in Phase 3, `run_exporter()` can move to `message-vault-io-core` and both can share it. For now, duplication is intentional.

- [ ] **Step 1: Add exporter dependency to src-tauri/Cargo.toml**

```toml
# Under [dependencies], add:
sms-backup-restore-exporter = { path = "../crates/exporters/sms-backup-restore-exporter", default-features = false }
```

- [ ] **Step 2: Create src-tauri/src/commands/events.rs — serializable event types**

`ProcessEvent` (from message-vault-io-core) only derives `Debug, Clone`, not `Serialize`. Define serializable event structs that can be emitted as Tauri events:

```rust
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct ExtractErrorEvent {
    pub detail: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_message: Option<String>,
}
```

- [ ] **Step 3: Rewrite src-tauri/src/commands/extract.rs**

Uses the real `ExporterConfig` shape — `cancel` and `log` fields are set directly on the config, and the exporter's `run()` takes `&ExporterConfig`:

```rust
use std::sync::{Arc, Mutex};
use std::thread;
use crate::state::AppState;
use message_vault_io_core::{
    CancelFlag, ExporterConfig, LogSink, MediaConfig, RunResult,
    SmsBackupRestoreConfig, SourceConfig,
};
use super::events::ExtractErrorEvent;

// CancelFlag is pub type CancelFlag = Arc<AtomicBool>.
// Clone shares the atomic — the cancel command flips it; the exporter polls it.
use std::sync::atomic::Ordering;

#[tauri::command]
pub async fn cancel(state: tauri::State<'_, Arc<Mutex<AppState>>>) -> Result<(), String> {
    let state = state.lock().map_err(|e| e.to_string())?;
    state.cancel_flag.store(true, Ordering::SeqCst);
    Ok(())
}

#[tauri::command]
pub async fn extract(
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
    app: tauri::AppHandle,
    source: String,
    path: String,
    output_dir: String,
) -> Result<(), String> {
    // Reset the shared cancel flag before starting a new job
    {
        let st = state.lock().map_err(|e| e.to_string())?;
        st.cancel_flag.store(false, Ordering::SeqCst);
    }

    let cancel = {
        let st = state.lock().map_err(|e| e.to_string())?;
        st.cancel_flag.clone()
    };

    let app_handle = app.clone();

    thread::spawn(move || {
        let log_app = app_handle.clone();
        let config = ExporterConfig {
            source: SourceConfig::SmsBackupRestore(SmsBackupRestoreConfig {
                path: Some(std::path::PathBuf::from(&path)),
            }),
            output_dir: std::path::PathBuf::from(&output_dir),
            media: MediaConfig::default(),
            cancel: Some(cancel),
            log: Some(LogSink::new(move |line: &str| {
                let _ = log_app.emit("extract:log", line.to_string());
            })),
            ..Default::default()
        };

        match sms_backup_restore_exporter::run(&config) {
            Ok(result) => {
                let msg = format!(
                    "Done: {} messages across {} conversations",
                    result.messages, result.conversations
                );
                let _ = app_handle.emit("extract:finished", msg);
            }
            Err(err) => {
                let _ = app_handle.emit("extract:error", ExtractErrorEvent {
                    detail: err.to_string(),
                    user_message: None,
                });
            }
        }
    });

    Ok(())
}
```

- [ ] **Step 4: Update src-tauri/src/commands/mod.rs**

```rust
pub mod events;
pub mod extract;
```

- [ ] **Step 5: Run cargo check**

```bash
cargo check -p message-vault-io-tauri
```

Fix any type mismatches — the real `SmsBackupRestoreConfig` and `RunResult` fields may differ slightly from what's shown. Read the actual struct definitions and adjust.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/
git commit -m "feat(tauri): implement extract command wrapping SMS Backup & Restore exporter

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

### Task 5: Build Extract screen UI

**Files:**
- Create: `web/src/screens/Extract.tsx`
- Create: `web/src/components/ProgressBar.tsx`
- Create: `web/src/components/FormRow.tsx`
- Create: `web/src/components/PathPicker.tsx`
- Modify: `web/src/App.tsx` (add tab navigation, import Extract screen)
- Modify: `web/src/lib/tauri.ts` (update invoke wrappers for real extract signature)
- Modify: `web/src/lib/types.ts` (add ProgressEvent variants)

**Interfaces:**
- Consumes: `invokeExtract`, `invokeCancel`, `onProgress` from `lib/tauri.ts`
- Produces: `Extract` screen component with exporter picker, path, media options, Run/Cancel buttons, progress bar

- [ ] **Step 1: Update web/src/lib/types.ts with event types matching the Rust side**

The Rust extract command emits three Tauri events:
- `extract:log` — payload is a plain `String` (log line)
- `extract:finished` — payload is a plain `String` (summary like "Done: 1423 messages across 87 conversations")
- `extract:error` — payload is `{ detail: string; user_message?: string }`

```typescript
export interface ExtractConfig {
  source: string;
  path: string;
  outputDir: string;
}

export interface ExtractErrorEvent {
  detail: string;
  user_message?: string;
}
```

- [ ] **Step 2: Update web/src/lib/tauri.ts with real invoke signatures and three listeners**

```typescript
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { ExtractConfig, ExtractErrorEvent } from "./types";

export async function invokeExtract(config: ExtractConfig): Promise<void> {
  return invoke("extract", {
    source: config.source,
    path: config.path,
    outputDir: config.outputDir,
  });
}

export async function invokeCancel(): Promise<void> {
  return invoke("cancel");
}

/** Returns an unlisten function that tears down all three listeners. */
export function onExtractEvents(callbacks: {
  onLog: (line: string) => void;
  onFinished: (summary: string) => void;
  onError: (err: ExtractErrorEvent) => void;
}): Promise<UnlistenFn> {
  return Promise.all([
    listen<string>("extract:log", (e) => callbacks.onLog(e.payload)),
    listen<string>("extract:finished", (e) => callbacks.onFinished(e.payload)),
    listen<ExtractErrorEvent>("extract:error", (e) => callbacks.onError(e.payload)),
  ]).then((unlisteners) => {
    return () => {
      unlisteners.forEach((u) => u());
    };
  });
}
```

- [ ] **Step 3: Create web/src/components/FormRow.tsx**

```tsx
interface FormRowProps {
  label: string;
  children: React.ReactNode;
}

export default function FormRow({ label, children }: FormRowProps) {
  return (
    <div style={{ display: "flex", alignItems: "center", marginBottom: "0.75rem", gap: "0.75rem" }}>
      <label style={{ width: "140px", flexShrink: 0, fontWeight: 500, fontSize: "0.875rem" }}>
        {label}
      </label>
      <div style={{ flex: 1 }}>{children}</div>
    </div>
  );
}
```

- [ ] **Step 4: Create web/src/components/PathPicker.tsx**

```tsx
import { open } from "@tauri-apps/plugin-dialog";

interface PathPickerProps {
  value: string;
  onChange: (path: string) => void;
  directory?: boolean;
  placeholder?: string;
}

export default function PathPicker({ value, onChange, directory, placeholder }: PathPickerProps) {
  const browse = async () => {
    const result = directory
      ? await open({ directory: true, multiple: false })
      : await open({ multiple: false });
    if (result && typeof result === "string") {
      onChange(result);
    }
  };

  return (
    <div style={{ display: "flex", gap: "0.5rem", flex: 1 }}>
      <input
        type="text"
        value={value}
        onChange={(e) => onChange(e.target.value)}
        placeholder={placeholder}
        style={{ flex: 1, padding: "0.25rem 0.5rem", fontSize: "0.875rem" }}
      />
      <button type="button" onClick={browse} style={{ padding: "0.25rem 0.75rem" }}>
        Browse
      </button>
    </div>
  );
}
```

- [ ] **Step 5: Create web/src/components/ProgressBar.tsx**

```tsx
interface ProgressBarProps {
  log: string[];
  running: boolean;
  current: number;
  total?: number;
}

export default function ProgressBar({ log, running, current, total }: ProgressBarProps) {
  const pct = total && total > 0 ? Math.round((current / total) * 100) : 0;

  return (
    <div>
      {running && (
        <div style={{ marginBottom: "0.5rem" }}>
          <div style={{
            height: "8px", background: "#e5e7eb", borderRadius: "4px", overflow: "hidden"
          }}>
            <div style={{
              height: "100%", width: total ? `${pct}%` : "100%",
              background: "#3b82f6",
              animation: total ? undefined : "indeterminate 1.5s ease-in-out infinite",
              transition: "width 0.3s ease",
            }} />
          </div>
          {total ? (
            <div style={{ fontSize: "0.75rem", color: "#6b7280", marginTop: "2px" }}>
              {current} / {total}
            </div>
          ) : null}
        </div>
      )}
      {log.length > 0 && (
        <pre style={{
          maxHeight: "200px", overflow: "auto", fontSize: "0.75rem",
          background: "#f3f4f6", padding: "0.5rem", borderRadius: "4px",
          margin: 0,
        }}>
          {log.map((line, i) => <div key={i}>{line}</div>)}
        </pre>
      )}
    </div>
  );
}
```

- [ ] **Step 6: Create web/src/screens/Extract.tsx**

Uses `onExtractEvents` with three callbacks matching the Rust event names:

```tsx
import { useState, useCallback, useRef } from "react";
import { invokeExtract, invokeCancel, onExtractEvents } from "../lib/tauri";
import FormRow from "../components/FormRow";
import PathPicker from "../components/PathPicker";
import ProgressBar from "../components/ProgressBar";
import type { UnlistenFn } from "@tauri-apps/api/event";

const SOURCES = [
  { id: "sms-backup-restore", label: "SMS Backup & Restore" },
  { id: "imessage-ios", label: "iMessage (iOS)" },
  { id: "imessage-macos", label: "iMessage (macOS)" },
  { id: "whatsapp-android", label: "WhatsApp (Android)" },
  { id: "whatsapp-ios", label: "WhatsApp (iOS)" },
  { id: "go-sms-pro", label: "GO SMS Pro" },
  { id: "imazing", label: "iMazing" },
  { id: "sms-backup-plus", label: "SMS Backup+" },
  { id: "openextract", label: "OpenExtract" },
];

export default function Extract() {
  const [source, setSource] = useState("sms-backup-restore");
  const [backupPath, setBackupPath] = useState("");
  const [outputDir, setOutputDir] = useState("");
  const [running, setRunning] = useState(false);
  const [log, setLog] = useState<string[]>([]);
  const [done, setDone] = useState(false);
  const unlistenRef = useRef<UnlistenFn | null>(null);

  const start = useCallback(async () => {
    setRunning(true);
    setDone(false);
    setLog([]);

    unlistenRef.current = await onExtractEvents({
      onLog: (line) => {
        setLog((prev) => [...prev, line]);
      },
      onFinished: (summary) => {
        setLog((prev) => [...prev, summary]);
        setRunning(false);
        setDone(true);
      },
      onError: (err) => {
        setLog((prev) => [...prev, `Error: ${err.detail}`]);
        if (err.user_message) {
          setLog((prev) => [...prev, err.user_message!]);
        }
        setRunning(false);
      },
    });

    try {
      await invokeExtract({ source, path: backupPath, outputDir });
    } catch (err) {
      setLog((prev) => [...prev, `Error starting extraction: ${err}`]);
      setRunning(false);
    }
  }, [source, backupPath, outputDir]);

  const cancel = useCallback(async () => {
    await invokeCancel();
  }, []);

  return (
    <div style={{ padding: "1.5rem", maxWidth: "700px" }}>
      <h2 style={{ margin: "0 0 1.5rem 0" }}>Extract Messages</h2>

      <FormRow label="Source">
        <select value={source} onChange={(e) => setSource(e.target.value)}
          style={{ padding: "0.25rem 0.5rem", fontSize: "0.875rem", width: "100%" }}>
          {SOURCES.map((s) => (
            <option key={s.id} value={s.id}>{s.label}</option>
          ))}
        </select>
      </FormRow>

      <FormRow label="Backup path">
        <PathPicker value={backupPath} onChange={setBackupPath} directory />
      </FormRow>

      <FormRow label="Output directory">
        <PathPicker value={outputDir} onChange={setOutputDir} directory />
      </FormRow>

      <div style={{ marginTop: "1.5rem", display: "flex", gap: "0.75rem" }}>
        <button onClick={start} disabled={running || !backupPath || !outputDir}
          style={{ padding: "0.5rem 1.5rem", fontWeight: 600 }}>
          {running ? "Running…" : "Extract"}
        </button>
        <button onClick={cancel} disabled={!running}
          style={{ padding: "0.5rem 1.5rem" }}>
          Cancel
        </button>
      </div>

      <div style={{ marginTop: "1.5rem" }}>
        <ProgressBar log={log} running={running} />
      </div>
    </div>
  );
}
```

- [ ] **Step 7: Simplify ProgressBar — remove determinate progress since the exporter doesn't expose counts during extraction**

The exporter `run()` function sends log lines but doesn't emit structured progress counts (no `current/total` mid-extraction). The Slint GUI shows indeterminate progress with a log tail. Match that:

```tsx
interface ProgressBarProps {
  log: string[];
  running: boolean;
}

export default function ProgressBar({ log, running }: ProgressBarProps) {
  return (
    <div>
      {running && (
        <div style={{ marginBottom: "0.5rem" }}>
          <div style={{
            height: "8px", background: "#e5e7eb", borderRadius: "4px", overflow: "hidden"
          }}>
            <div style={{
              height: "100%", width: "100%",
              background: "#3b82f6",
              animation: "indeterminate 1.5s ease-in-out infinite",
            }} />
          </div>
        </div>
      )}
      {log.length > 0 && (
        <pre style={{
          maxHeight: "300px", overflow: "auto", fontSize: "0.75rem",
          background: "#f3f4f6", padding: "0.5rem", borderRadius: "4px",
          margin: 0, whiteSpace: "pre-wrap", wordBreak: "break-word",
        }}>
          {log.map((line, i) => <div key={i}>{line}</div>)}
        </pre>
      )}
    </div>
  );
}
```

- [ ] **Step 7: Update web/src/App.tsx with tab navigation**

```tsx
import { useState } from "react";
import Extract from "./screens/Extract";

function App() {
  return (
    <div style={{ fontFamily: "system-ui", minHeight: "100vh", background: "#fafafa" }}>
      <Extract />
    </div>
  );
}

export default App;
```

- [ ] **Step 8: Install the Tauri dialog plugin**

```bash
cd src-tauri
cargo add tauri-plugin-dialog
```

Add to `src-tauri/src/main.rs`:
```rust
// In the .build() chain, add:
.plugin(tauri_plugin_dialog::init())
```

- [ ] **Step 9: Verify the screen renders in Tauri dev**

```bash
cargo tauri dev
```

Expected: Window shows the Extract form with source picker, path inputs, and Browse buttons.

- [ ] **Step 10: Commit**

```bash
git add web/src/ src-tauri/
git commit -m "feat(tauri): build Extract screen with progress and cancel support

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

### Task 6: End-to-end extraction test

**Files:**
- No new files (manual test)

- [ ] **Step 1: Prepare a test backup**

Locate or create a small SMS Backup & Restore XML backup file for testing.

- [ ] **Step 2: Run extraction from the Tauri app**

```bash
cargo tauri dev
```

In the app window:
1. Select "SMS Backup & Restore" as source
2. Browse to the test backup directory
3. Choose an output directory
4. Click "Extract"

Expected: Progress bar fills, log lines appear, JSONL output files appear in the chosen output directory.

- [ ] **Step 3: Verify the output**

```bash
ls <output_dir>/
# Expected: one folder per conversation, each containing messages.jsonl + optional attachments/
cat <output_dir>/<conversation>/messages.jsonl | head -3
# Expected: valid JSONL with ConversationDocument fields
```

- [ ] **Step 4: Test cancel**

Start a large extraction, click "Cancel" mid-run. Expected: extraction stops cleanly, partial output may exist but no corrupted files.

- [ ] **Step 5: Commit any fixes**

```bash
git add -A
git commit -m "test(tauri): verify end-to-end extraction works with real backup"
```

---

## Phase 3: Full parity (outline)

Once Phase 2 proves the pattern, these tasks repeat it for the remaining exporters and features:

### Task 7-12: Remaining 6 exporter commands

Each follows the Task 4 pattern: add the crate dependency to `src-tauri/Cargo.toml`, add a variant to the extract command's `source` dispatch, wire the exporter's `run()` function. Order by complexity:

- Task 7: `go-sms-pro-exporter` (XML parsing, no external deps)
- Task 8: `sms-backup-plus-exporter` (mailparse, needs test data)
- Task 9: `openextract-exporter`
- Task 10: `imazing-exporter`
- Task 11: `whatsapp-exporter` (shells out to Python `wtsexporter`)
- Task 12: `imessage-ir-exporter` (GPL, most complex — iOS backups, rusqlite)

### Task 13: Format command and screen

Wrap `message-reexport::convert_directory()`. Create `Format.tsx` screen with format picker (JSONL/EML/MBOX/CSV/XML), input/output dir pickers, progress bar.

### Task 14: Vault Push command and screen

Wrap `vault_push::run()`. Create `Push.tsx` screen with vault URL, API key, source name, input dir, progress with upload stats.

### Task 15: Vault Pull command and screen

Wrap `vault_pull::run()`. Create `Pull.tsx` screen with vault URL, API key, search query, output dir, progress with download stats.

### Task 16: Contacts command

Wrap `message-contacts` library. Parse VCF/CSV on the Rust side, return structured contact data to the frontend for display.

### Task 17: Settings screen

Persistence for vault credentials, default paths, ffmpeg location. Reads/writes `export.ini` via Tauri commands that call `AppState.ini`.

### Task 18: Remove Slint GUI

Remove `crates/message-vault-io-gui/` from workspace members. Remove `slint` and `slint-build` from workspace dependencies. Delete the crate directory. Update any CI scripts that reference it.

---

## Phase 4: Polish (outline)

### Task 19: App icons and branding

Generate icon PNGs/ICNS/ICO from the app logo. Place in `src-tauri/icons/`. Verify in Tauri build output.

### Task 20: Platform installers

Configure `tauri.conf.json` bundle section for `.deb`, `.AppImage`, `.dmg`, `.msi`. Test `cargo tauri build` on each platform.

### Task 21: Documentation updates

Update `CONTRIBUTING.md` and README with Tauri dev instructions. Update the docs site (`docs/src/content/docs/`) for the new desktop app workflow. Remove references to the Slint GUI.

---

## Verification

After all Phase 2 tasks complete, run:

```bash
# Rust build
cargo build --workspace --release

# Frontend build
cd web && npm run build

# Existing tests must still pass
cargo test --workspace

# Tauri dev launches
cargo tauri dev
```

After Phase 3, additionally verify:

```bash
# All 7 exporters work from the Tauri UI
# vault-push and vault-pull complete successfully against a local vault server
# Format conversion produces valid output in all target formats
# Settings persist across app restarts
```
