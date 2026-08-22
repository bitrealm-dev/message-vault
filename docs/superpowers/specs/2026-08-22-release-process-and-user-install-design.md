# Release process and user install path

**Date:** 2026-08-22  
**Status:** Approved for planning

## Context

CI already ships a product when a `v*` tag is pushed: the Docker image `bitrealm/message-vault` and unsigned Tauri installers (Linux `.deb` + AppImage, Windows `.msi`, macOS `.dmg`). Nothing is published to npm or PyPI.

The User Guide still describes an older zip/tarball that put `ffmpeg` / `ffprobe` under `lib/` and `wtsexporter` under `cli/`, next to the app. The Tauri installer does not bundle those helpers. Convert and WhatsApp extract still look for them on `PATH` (and in those old folders if present).

`docs/maintainers/developing.md` is the Slint-era release write-up. It talks about `.tgz` archives and a workflow file that no longer exists. The maintainer index still links to it. Contributing (`docs/src/content/docs/vault/developer/contributing.md`) has no maintainer release steps.

End users should not be told to compile the vault or to assemble a sidecar folder. The vault comes from Docker (FFmpeg for playback is already in that image). The desktop app comes from GitHub Releases. FFmpeg and `wtsexporter` for Convert / WhatsApp extract are installed with package managers, with download pages as a fallback.

Extract-in-Docker (upload or mount a backup into the vault container) is a later product change, not this work.

## Goal

A maintainer can cut a release from Contributing: bump four version files, update the changelog, push a `v*` tag. A user can install from the User Guide: Docker for the vault, Tauri installer for the desktop app, package-manager commands for FFmpeg and `wtsexporter`. `developing.md` is gone. Nothing points at it.

## Non-goals

- Bundling FFmpeg or `wtsexporter` inside the Tauri installer
- Moving extract or convert into the Docker vault
- A shared `VERSION` file or a script that writes the four version configs
- Code signing (Windows / macOS stay unsigned; SmartScreen and Gatekeeper notes stay)
- Changing `.github/workflows/ci.yml` (the tag jobs already match what ships)
- Publishing to npm or PyPI
- Rewriting Operator Docker or Run from source except dropping any `developing.md` link
- Changing contributor Environment Setup (`apt` FFmpeg and `pipx` helpers stay for people who compile)

## Decisions

1. **Two artifacts, one product version.** Semantic Versioning (`MAJOR.MINOR.PATCH`). Example: `0.8.0`. Git tag `v0.8.0`. Docker Hub tag `0.8.0` (no `v`), plus `0.8`, `latest`, and `sha-…`. Merge to `main` does not ship. Pushing the tag ships.
2. **Four version files stay in lockstep.** Edit all four before tagging: `src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json`, `web/package.json`, `crates/vault/server/Cargo.toml`. Leave other `Cargo.toml` files at `0.1.0`. Do not bump `crates/message-vault-io-gui/` (`0.6.0`) or `web-next/` (`0.3.0`). JSONL schema version 3 is independent of the product version.
3. **No shared version file in this change.** Tauri can omit `version` or point at `package.json`; the vault crate can inherit `workspace.package.version`. That still leaves more than one file, and `src-tauri` is excluded from the workspace. A `VERSION` file plus a writer script is later work.
4. **Release Process lives on Contributing.** Last section, after License. Mark it maintainers-only so a first-time contributor can skip it. Do not add a separate Developer sidebar page.
5. **Test-before-release is CI on `main`.** After merge, `fmt`, workspace tests, and `web` tests have already run. `./scripts/check-pr.sh` is optional locally. Then changelog, version bump, tag.
6. **Changelog is Keep a Changelog.** In the same commit as the four version files, move `[Unreleased]` notes under the new version heading in `CHANGELOG.md`. GitHub Release body can stay the CI template (installers + Docker pull). Do not rewrite `ci.yml` to scrape the changelog.
7. **Do not tag unless a release is intended.**
8. **User path is Docker + Tauri + PATH helpers.** Install the desktop app from GitHub Releases (`.deb` / AppImage / `.msi` / `.dmg`), not `.tgz` / `.zip` sidecar archives. Try the vault stays Docker. Update is `docker pull` (same volume) plus a new installer. Helpers:

   | Tool | Windows | Linux | macOS | Download if the command fails |
   |------|---------|-------|-------|-------------------------------|
   | FFmpeg | `winget install -e --id Gyan.FFmpeg` | `sudo apt-get install ffmpeg` | `brew install ffmpeg` | https://ffmpeg.org/download.html |
   | wtsexporter | `pipx install "whatsapp-chat-exporter[android_backup,crypt15]"` | same | same | https://github.com/KnugiHK/WhatsApp-Chat-Exporter/releases and https://wts.knugi.dev/ |

   The desktop app finds both on `PATH`.
9. **Delete `docs/maintainers/developing.md`.** Point the maintainer index at Contributing (Release Process). Do not leave a stub.

## What changes

| Path | Change |
|------|--------|
| `docs/src/content/docs/vault/developer/contributing.md` | Add **Release Process** at the end (copy outline below) |
| `docs/src/content/docs/vault/user/get-started/install-the-desktop-app.md` | Tauri installers + helper table; keep unsigned warnings |
| `docs/src/content/docs/vault/user/how-to/update.md` | Docker pull + new installer; drop “extract archive into the same folder” |
| `docs/src/content/docs/vault/user/how-to/troubleshooting.md` | PATH / package-manager; drop `lib/` + `cli/` next to the app as the primary story |
| `docs/src/content/docs/vault/user/prepare-a-backup/android-whatsapp.md` | Tell the reader to install `wtsexporter`; remove “the desktop app ships with `wtsexporter`” |
| Duplicate pages under `docs/src/content/docs/get-started/`, `how-to/`, and `prepare-a-backup/` | Apply the same User Guide edits if those files still exist, so search does not show the zip/tarball story |
| `docs/maintainers/README.md` | Replace the `developing.md` link with Contributing → Release Process |
| `docs/maintainers/developing.md` | Delete |

## Contributing — Release Process outline

Place this after **License**. Short sentences. No “we” / “us” / “our”.

### Release Process

For maintainers. Skip this section when opening a first pull request.

**What ships**

One product version (Semantic Versioning). Two artifacts:

- Vault image `bitrealm/message-vault:<version>` on Docker Hub
- Unsigned desktop installers on GitHub Releases

Nothing is published to npm or PyPI. Pushing git tag `v<version>` is what runs the release jobs. A merge to `main` does not.

**Before tagging**

1. Merge the work to `main`. Wait until CI on `main` is green.
2. Move `[Unreleased]` entries in `CHANGELOG.md` under the new version heading.
3. Set these four files to the same number (example `0.8.0`):
   - `src-tauri/Cargo.toml`
   - `src-tauri/tauri.conf.json`
   - `web/package.json`
   - `crates/vault/server/Cargo.toml`
4. Commit and push that bump on `main`.
5. Tag that commit `v0.8.0` and push the tag.

**After the tag**

GitHub Actions builds the image and the installers and opens a GitHub Release named `Message Vault v0.8.0`. Installers are not code-signed. Users may see SmartScreen or Gatekeeper warnings.

Do not create or push tags unless a release should ship.

## User Guide copy notes

**Install the desktop app**

- Linux: `.deb` or AppImage from [Releases](https://github.com/bitrealm-io/message-vault/releases)
- Windows: `.msi`. SmartScreen: run once; not signed
- macOS: `.dmg` (Apple Silicon). Gatekeeper: allow once; not signed
- Then the helper table (FFmpeg and `wtsexporter`)
- “Build from source” stays a link to Run from source, not the default

**Update**

- Stop / remove container, `docker pull`, start again on the same named volume (or `docker compose pull && docker compose up -d`)
- Install the new desktop build from Releases
- Pin example stays `bitrealm/message-vault:0.7.3` style (no `v` on Docker tags)

**Troubleshooting**

- Missing FFmpeg / `wtsexporter`: run the package-manager command, confirm `ffmpeg` and `wtsexporter` on `PATH`, then retry
- Drop “extract the whole archive” / `lib/` / `cli/` as the first diagnosis

**Android WhatsApp**

- One sentence: install `wtsexporter` using the commands on Install the desktop app (or repeat the `pipx` line)
- Keep “the app runs `wtsexporter`” as behavior, not as “it is bundled”

## Voice

Match Contributing and the User Guide: short sentences, concrete commands, no “we” / “us” / “our”. Starlight asides are optional. Do not use GitHub `> [!TIP]` alerts.

## Verification

- Contributing has **Release Process** after **License**
- Four version files are listed; no `VERSION` file is introduced
- `developing.md` is deleted; maintainer README does not link to it
- Install / Update / Troubleshooting / Android WhatsApp no longer describe sidecar zip/tarball layout as the product
- Duplicate non-`/vault/` copies of those pages match, or they are removed
- `cd docs && npm run check && npm run build` succeeds

## Success criteria

- A maintainer can ship from the Contributing page without opening `developing.md`
- A user can run Docker + a Tauri installer + two helper commands without compiling or assembling `lib/` and `cli/`
- First-time contributors can skip Release Process and still follow Environment Setup
