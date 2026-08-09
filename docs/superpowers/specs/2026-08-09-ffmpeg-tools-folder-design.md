# ffmpeg tools folder for desktop convert/compress

**Date:** 2026-08-09  
**Status:** approved  
**Scope:** `crates/libs/media`, `src-tauri`, `web/` Appearance settings (Tauri only). No changes to `web-next/`.

## Problem

Settings → Appearance → Media lets the user type an “ffmpeg path” and saves it under the browser key `mv-ffmpeg-path`. Convert and compress never read that value. The Rust media library finds `ffmpeg` and `ffprobe` by searching next to the app binary, then the environment variable `MESSAGE_VAULT_IO_BIN` (a **directory** that must contain both tools), then `PATH`.

So the setting looks real and does nothing. Users who install ffmpeg outside the default search paths cannot point the desktop app at their tools folder. There is also no check that both binaries exist and run before Import or Extract tries convert/compress.

## Goal

1. Treat the setting as a **folder** that contains both `ffmpeg` and `ffprobe` (with `.exe` on Windows).
2. When that folder is set, Rust uses it for convert/compress the same way `MESSAGE_VAULT_IO_BIN` already works.
3. On Save (and when restoring a saved folder), report whether each binary was found and passed `-version`.

## Approach

Reuse `MESSAGE_VAULT_IO_BIN`. Do not thread a new path through every extract/format argument. The GUI probes the folder, then sets that environment variable for the process (or clears it when the field is blank). Jobs keep calling the existing media resolver.

## Media library (`crates/libs/media`)

### Folder meaning

- Non-empty override: look only under that directory for `ffmpeg` and `ffprobe` (plus `.exe` on Windows). Both must be files that succeed with `-version`.
- Empty override: keep today’s search order (beside the executable / `lib/`, then any existing `MESSAGE_VAULT_IO_BIN`, then `PATH`).

### Cache

Tool paths are cached with `OnceLock` today. After the first lookup, changing or clearing the tools folder would be ignored until process exit. The cache must be invalidated whenever the override directory is set or cleared, or lookups must not pin an answer across override changes.

### Public probe API

Expose something like:

- Input: optional directory string (empty / `None` = default discovery).
- Output: whether overall ok; resolved path for `ffmpeg` if found; resolved path for `ffprobe` if found; short error text when not ok.

This API is what Tauri commands call. It must exercise the same resolution rules convert/compress use.

## Tauri commands

Register two commands (names can match implementation style):

| Command | Behavior |
|---------|----------|
| `probe_ffmpeg_tools` | Optional folder. Run the media probe. Do not change process env by itself. |
| `set_ffmpeg_tools_dir` | Optional folder. Empty clears `MESSAGE_VAULT_IO_BIN` and invalidates the media cache. Non-empty sets `MESSAGE_VAULT_IO_BIN` to that folder and invalidates the cache. Prefer calling this only after a successful probe when applying a user Save. |

Extract, format, and import invoke paths stay as they are. They already go through `media` for convert/compress.

## Frontend (Appearance, Tauri only)

File: `web/src/screens/settings/AppearanceSection.tsx` (and thin wrappers in `web/src/lib/tauri.ts`).

1. Relabel the field as a **folder** containing ffmpeg and ffprobe. Placeholder: leave blank to use the app bundle / system PATH.
2. Keep storing the folder string in `localStorage` under `mv-ffmpeg-path`.
3. **Save:** call `probe_ffmpeg_tools`. If both tools are ok, call `set_ffmpeg_tools_dir`, persist to `localStorage`, show success including resolved paths. If not ok, do **not** set the env override; show which tool failed.
4. **Mount:** if `localStorage` has a non-empty folder, call `set_ffmpeg_tools_dir` and `probe_ffmpeg_tools` so Import/Extract use the folder without requiring another Save; show the probe status.
5. Optional: directory `PathPicker` beside the text field for choosing the folder.

Blank save clears `localStorage`, clears the env override, and shows that default discovery is in use (probe with no folder still useful so the user sees whether PATH/lib tools work).

## Out of scope

- Showing or editing this setting in the browser-only (non-Tauri) UI.
- Shipping or downloading ffmpeg binaries.
- Changing convert/compress CLI flags or quality options.
- Accepting a path to a single ffmpeg binary (folder only).

## Success criteria

1. With a valid tools folder saved, convert/compress uses binaries from that folder (verified by probe paths and by a real import/extract that needs ffmpeg).
2. With a bad or incomplete folder, Save shows a clear failure and does not leave a broken `MESSAGE_VAULT_IO_BIN` set.
3. Changing the folder (or clearing it) takes effect without restarting the app.
4. `cargo test -p media` (and any new probe tests) pass; `cd web && npm run build` succeeds.
5. Blank setting still finds tools via the previous discovery order when they are installed normally.

## Follow-ups (not this work)

- Wire Import’s extract phase to wait on `extract:*` job events (separate bug from this setting).
- Persist the tools folder in a Rust-side config file instead of only `localStorage` if desktop settings should survive clearing site data.
