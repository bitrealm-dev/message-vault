# ffmpeg Tools Folder Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let the desktop Appearance setting store a folder of `ffmpeg`/`ffprobe` tools, apply it through `MESSAGE_VAULT_IO_BIN`, and show whether both binaries work.

**Architecture:** Extend `crates/libs/media` with a clearable tools-dir override and a probe API. Tauri exposes `probe_ffmpeg_tools` and `set_ffmpeg_tools_dir`. The Appearance Media section probes on Save/mount, sets the env override only when both tools pass `-version`, and keeps the folder string in `localStorage` (`mv-ffmpeg-path`).

**Tech Stack:** Rust (`media`, Tauri 2 commands), React/TypeScript Vite SPA under `web/`.

**Spec:** `docs/superpowers/specs/2026-08-09-ffmpeg-tools-folder-design.md`

## Global Constraints

- Folder only (not a path to a single binary); blank means default discovery.
- Both `ffmpeg` and `ffprobe` required; check with `-version`.
- Changing or clearing the folder must take effect without restarting the app (invalidate tool path cache).
- Do not set `MESSAGE_VAULT_IO_BIN` on a failed Save probe.
- Tauri-only UI; no web-only ffmpeg setting.
- No new extract/format invoke arguments — jobs keep using the media resolver.

## File map

| File | Role |
|------|------|
| `crates/libs/media/src/tools.rs` | Override dir, cache invalidate, `find_tool`, probe types/API |
| `crates/libs/media/src/lib.rs` | Re-export probe API + `set_tools_dir` |
| `src-tauri/src/commands/ffmpeg.rs` | Tauri wrappers |
| `src-tauri/src/commands/mod.rs` | `pub mod ffmpeg` |
| `src-tauri/src/main.rs` | Register commands |
| `web/src/lib/tauri.ts` | Typed `probeFfmpegTools` / `setFfmpegToolsDir` |
| `web/src/screens/settings/AppearanceSection.tsx` | Folder UI, Save/mount probe status, PathPicker |

---

### Task 1: Media tools override + probe API

**Files:**
- Modify: `crates/libs/media/src/tools.rs`
- Modify: `crates/libs/media/src/lib.rs`
- Test: unit tests inside `tools.rs` `#[cfg(test)]`

**Interfaces:**
- Consumes: existing `find_tool` / `command_ok` / `executable_name`
- Produces:
  - `pub fn set_tools_dir(dir: Option<PathBuf>)` — stores override; clears tool path cache
  - `pub fn tools_dir() -> Option<PathBuf>` — current override (for tests)
  - `pub struct FfmpegToolsProbe { pub ok: bool, pub ffmpeg_path: Option<PathBuf>, pub ffprobe_path: Option<PathBuf>, pub error: Option<String> }`
  - `pub fn probe_ffmpeg_tools(dir: Option<&Path>) -> FfmpegToolsProbe` — if `Some(dir)`, resolve only under that folder; if `None`, use current override if set, else full default discovery. Sets `ok` only when both tools resolve.

**Implementation notes for `tools.rs`:**

Replace `OnceLock<Option<PathBuf>>` caches with something that can clear, for example:

```rust
use std::sync::{Mutex, OnceLock};

struct ToolCache {
    ffmpeg: Option<PathBuf>,
    ffprobe: Option<PathBuf>,
}

fn tool_cache() -> &'static Mutex<ToolCache> {
    static CACHE: OnceLock<Mutex<ToolCache>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(ToolCache { ffmpeg: None, ffprobe: None }))
}

fn tools_override() -> &'static Mutex<Option<PathBuf>> {
    static OVERRIDE: OnceLock<Mutex<Option<PathBuf>>> = OnceLock::new();
    OVERRIDE.get_or_init(|| Mutex::new(None))
}

pub fn set_tools_dir(dir: Option<PathBuf>) {
    *tools_override().lock().expect("tools override lock") = dir;
    let mut c = tool_cache().lock().expect("tool cache lock");
    c.ffmpeg = None;
    c.ffprobe = None;
}
```

In `find_tool` / `resolve_tool`:

1. If override is `Some(dir)`, look only at `dir.join(executable_name(name))` with `command_ok(..., &["-version"])`.
2. Else keep today’s candidate order (exe sibling / lib / `MESSAGE_VAULT_IO_BIN` / PATH / bare).

When `set_tools_dir(Some(path))` is called from Tauri, also set `MESSAGE_VAULT_IO_BIN` in the Tauri layer (Task 2) so CLI-style env stays consistent; the media override is the source of truth for cache-safe lookups inside the library.

`probe_ffmpeg_tools`:

```rust
pub fn probe_ffmpeg_tools(dir: Option<&Path>) -> FfmpegToolsProbe {
    let previous = tools_dir();
    if let Some(d) = dir {
        set_tools_dir(Some(d.to_path_buf()));
    } else {
        // leave override as-is for "current process" probe when dir is None
    }
    // If dir is Some, temporarily set; always restore previous after probe when probing a candidate folder that is not being applied yet.
    ...
}
```

Prefer this probe behavior to avoid stomping the live override during Save-before-set:

- If `dir` is `Some(d)`: resolve **only** under `d` without mutating the process override (helper `find_tool_in_dir(dir, name)`).
- If `dir` is `None`: use `resolve_tool` as configured (override or default discovery).

```rust
fn find_tool_in_dir(dir: &Path, name: &str) -> Option<PathBuf> {
    let candidate = dir.join(executable_name(name));
    if candidate.is_file() && command_ok(&candidate, &["-version"]) {
        Some(candidate)
    } else {
        None
    }
}

pub fn probe_ffmpeg_tools(dir: Option<&Path>) -> FfmpegToolsProbe {
    let (ffmpeg, ffprobe) = match dir {
        Some(d) => (find_tool_in_dir(d, "ffmpeg"), find_tool_in_dir(d, "ffprobe")),
        None => (resolve_tool("ffmpeg"), resolve_tool("ffprobe")),
    };
    match (ffmpeg, ffprobe) {
        (Some(f), Some(p)) => FfmpegToolsProbe {
            ok: true,
            ffmpeg_path: Some(f),
            ffprobe_path: Some(p),
            error: None,
        },
        (f, p) => {
            let mut parts = Vec::new();
            if f.is_none() {
                parts.push("ffmpeg not found or failed -version");
            }
            if p.is_none() {
                parts.push("ffprobe not found or failed -version");
            }
            FfmpegToolsProbe {
                ok: false,
                ffmpeg_path: f,
                ffprobe_path: p,
                error: Some(parts.join("; ")),
            }
        }
    }
}
```

`set_tools_dir(Some(dir))` must make `resolve_tool` prefer that directory (override mutex). `set_tools_dir(None)` clears override so default discovery returns.

- [ ] **Step 1: Write failing tests** in `tools.rs`:

```rust
#[cfg(unix)]
#[test]
fn probe_folder_requires_both_tools() {
    let dir = tempfile::tempdir().unwrap();
    let ffmpeg = dir.path().join("ffmpeg");
    std::fs::write(&ffmpeg, "#!/bin/sh\nexit 0\n").unwrap();
    let mut perms = std::fs::metadata(&ffmpeg).unwrap().permissions();
    use std::os::unix::fs::PermissionsExt;
    perms.set_mode(0o755);
    std::fs::set_permissions(&ffmpeg, perms).unwrap();

    let probe = probe_ffmpeg_tools(Some(dir.path()));
    assert!(!probe.ok);
    assert!(probe.ffmpeg_path.is_some());
    assert!(probe.ffprobe_path.is_none());
}

#[cfg(unix)]
#[test]
fn set_tools_dir_overrides_and_clears_cache() {
    let dir = tempfile::tempdir().unwrap();
    for name in ["ffmpeg", "ffprobe"] {
        let p = dir.path().join(name);
        std::fs::write(&p, "#!/bin/sh\nexit 0\n").unwrap();
        let mut perms = std::fs::metadata(&p).unwrap().permissions();
        use std::os::unix::fs::PermissionsExt;
        perms.set_mode(0o755);
        std::fs::set_permissions(&p, perms).unwrap();
    }
    set_tools_dir(Some(dir.path().to_path_buf()));
    assert!(ffmpeg_available());
    set_tools_dir(None);
}
```

- [ ] **Step 2: Run tests — expect fail**

```bash
cargo test -p media probe_folder_requires_both_tools set_tools_dir_overrides -- --nocapture
```

Expected: compile error or fail because APIs missing.

- [ ] **Step 3: Implement** `set_tools_dir`, clearable cache, `find_tool_in_dir`, `probe_ffmpeg_tools`, wire `find_tool` to honor override first. Re-export from `lib.rs`:

```rust
pub use tools::{ffmpeg_available, probe_ffmpeg_tools, set_tools_dir, FfmpegToolsProbe};
```

- [ ] **Step 4: Run tests — expect pass**

```bash
cargo test -p media --lib
```

- [ ] **Step 5: Commit**

```bash
git add crates/libs/media/src/tools.rs crates/libs/media/src/lib.rs
git commit -m "$(cat <<'EOF'
feat(media): probe and override ffmpeg tools folder

Add a clearable tools-dir override and probe_ffmpeg_tools so the
desktop app can verify and prefer a user-chosen tools folder.
EOF
)"
```

---

### Task 2: Tauri probe / set commands

**Files:**
- Create: `src-tauri/src/commands/ffmpeg.rs`
- Modify: `src-tauri/src/commands/mod.rs`
- Modify: `src-tauri/src/main.rs`

**Interfaces:**
- Consumes: `media::{probe_ffmpeg_tools, set_tools_dir, FfmpegToolsProbe}`
- Produces: Tauri commands below (camelCase from TS)

```rust
#[derive(serde::Serialize)]
pub struct FfmpegToolsProbeDto {
    pub ok: bool,
    pub ffmpeg_path: Option<String>,
    pub ffprobe_path: Option<String>,
    pub error: Option<String>,
}

#[tauri::command]
pub fn probe_ffmpeg_tools(dir: Option<String>) -> FfmpegToolsProbeDto {
    let path = dir.as_deref().map(str::trim).filter(|s| !s.is_empty()).map(Path::new);
    let p = media::probe_ffmpeg_tools(path);
    FfmpegToolsProbeDto {
        ok: p.ok,
        ffmpeg_path: p.ffmpeg_path.map(|x| x.display().to_string()),
        ffprobe_path: p.ffprobe_path.map(|x| x.display().to_string()),
        error: p.error,
    }
}

#[tauri::command]
pub fn set_ffmpeg_tools_dir(dir: Option<String>) -> Result<FfmpegToolsProbeDto, String> {
    let trimmed = dir.as_deref().map(str::trim).filter(|s| !s.is_empty());
    match trimmed {
        None => {
            // SAFETY: desktop process owns this env for the session.
            unsafe { std::env::remove_var("MESSAGE_VAULT_IO_BIN") };
            media::set_tools_dir(None);
            Ok(probe_ffmpeg_tools(None))
        }
        Some(s) => {
            let path = PathBuf::from(s);
            let probe = media::probe_ffmpeg_tools(Some(path.as_path()));
            if !probe.ok {
                return Err(probe.error.unwrap_or_else(|| "ffmpeg tools not found".into()));
            }
            unsafe { std::env::set_var("MESSAGE_VAULT_IO_BIN", &path) };
            media::set_tools_dir(Some(path));
            Ok(FfmpegToolsProbeDto { /* from probe */ })
        }
    }
}
```

Register in `main.rs`:

```rust
commands::ffmpeg::probe_ffmpeg_tools,
commands::ffmpeg::set_ffmpeg_tools_dir,
```

- [ ] **Step 1: Add `ffmpeg.rs` and module export**
- [ ] **Step 2: Register commands in `main.rs`**
- [ ] **Step 3: Compile**

```bash
cd src-tauri && cargo check
```

Expected: success.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/commands/ffmpeg.rs src-tauri/src/commands/mod.rs src-tauri/src/main.rs
git commit -m "$(cat <<'EOF'
feat(tauri): expose ffmpeg tools probe and set commands

Wire Appearance settings to media::probe_ffmpeg_tools and
set_tools_dir, keeping MESSAGE_VAULT_IO_BIN in sync for the process.
EOF
)"
```

---

### Task 3: Frontend wrappers + Appearance UI

**Files:**
- Modify: `web/src/lib/tauri.ts`
- Modify: `web/src/screens/settings/AppearanceSection.tsx`

**Interfaces:**
- Consumes: Tauri commands from Task 2
- Produces:

```typescript
export interface FfmpegToolsProbe {
  ok: boolean;
  ffmpeg_path: string | null;
  ffprobe_path: string | null;
  error: string | null;
}

export async function probeFfmpegTools(dir: string | null): Promise<FfmpegToolsProbe> {
  return invoke("probe_ffmpeg_tools", { dir });
}

export async function setFfmpegToolsDir(dir: string | null): Promise<FfmpegToolsProbe> {
  return invoke("set_ffmpeg_tools_dir", { dir });
}
```

**AppearanceSection behavior:**

- Label: “ffmpeg tools folder” (contains ffmpeg and ffprobe).
- Use `PathPicker` with `directory` for choosing; keep editable text value.
- State: `ffmpegPath`, `status: "" | success | error message`, `checking`.
- On mount (Tauri): read `localStorage.getItem("mv-ffmpeg-path")`; if non-empty, `setFfmpegToolsDir(path)` then show probe result; if empty, optional `probeFfmpegTools(null)` to show default discovery status.
- On Save:
  - If blank: `localStorage.removeItem(...)`, `setFfmpegToolsDir(null)`, show “Using default discovery” + probe paths if any.
  - If non-blank: `probeFfmpegTools(path)` first; on failure set error and return; on success `setFfmpegToolsDir(path)`, `localStorage.setItem`, show both resolved paths.

- [ ] **Step 1: Add tauri.ts wrappers**
- [ ] **Step 2: Update AppearanceSection UI + Save/mount logic**
- [ ] **Step 3: Build**

```bash
cd web && npm run build
```

Expected: success.

- [ ] **Step 4: Manual smoke (desktop)**

1. Settings → Appearance → pick a folder without tools → Save → error, env not applied.
2. Folder with both tools → Save → success paths shown.
3. Restart app / remount settings → status restores; Import with compress still works.
4. Clear field → Save → default discovery.

- [ ] **Step 5: Commit**

```bash
git add web/src/lib/tauri.ts web/src/screens/settings/AppearanceSection.tsx
git commit -m "$(cat <<'EOF'
feat(web): apply and verify ffmpeg tools folder in Appearance

Probe both binaries on Save and restore MESSAGE_VAULT_IO_BIN from
localStorage when Settings opens so convert/compress use the folder.
EOF
)"
```

---

## Spec coverage checklist

| Spec requirement | Task |
|------------------|------|
| Folder-only meaning | 3 (UI copy) + 1 (`find_tool_in_dir`) |
| Both tools + `-version` | 1 probe |
| Set `MESSAGE_VAULT_IO_BIN` / override | 2 |
| Invalidate cache on change | 1 `set_tools_dir` |
| Probe + set Tauri commands | 2 |
| Save only sets on success | 2 `set_ffmpeg_tools_dir` Err + 3 UI |
| Mount restore | 3 |
| Blank = default discovery | 1–3 |
| No extract arg changes | (none) |
| Tests + web build | 1, 3 |

## Placeholder / consistency self-review

- Command names: `probe_ffmpeg_tools` / `set_ffmpeg_tools_dir` match TS invoke strings.
- DTO fields: `ok`, `ffmpeg_path`, `ffprobe_path`, `error` (snake in Rust; Tauri serde typically camelCases to JS — if the project uses snake in payloads elsewhere, match existing `ExtractErrorEvent` style; check `user_message` in events and keep **snake_case** in JSON if that is the app convention).

Verify against existing Tauri payloads before finishing Task 2: if events use snake_case, keep DTO snake_case and type the TS interface with snake_case fields (`ffmpeg_path`), not camelCase.
