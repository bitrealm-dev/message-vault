---
title: Contributing
description: Set up a development environment, run tests, and open pull requests for Message Vault.
---

Welcome. Product overview and install paths are in the [User Guide](/vault/user/) and the repository [README](https://github.com/bitrealm-io/message-vault/blob/main/README.md). Architecture and release notes for maintainers live under [`docs/maintainers/`](https://github.com/bitrealm-io/message-vault/blob/main/docs/maintainers/README.md).

Before contributing, read the [Code of Conduct](https://github.com/bitrealm-io/message-vault/blob/main/CODE_OF_CONDUCT.md).

For a short “clone and run” path without the full contributor checklist, see [Run from source](/vault/developer/run-from-source/).

## Prerequisites

| Tool | Notes |
|------|--------|
| **Rust** | Stable toolchain via [rustup](https://rustup.rs/). This workspace uses Rust edition **2024**, which needs **Rust 1.85+**. CI builds with the latest stable. |
| **Windows** | [Visual Studio Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/) with the "Desktop development with C++" workload (MSVC). |
| **macOS** | Xcode Command Line Tools (`xcode-select --install`). |
| **Linux** | C toolchain plus GUI system libs (see [Linux packages](#linux-packages) below). |
| **Node.js 22+** | For the desktop app frontend (`web/`) and the docs site (`docs/`). |

Optional for full WhatsApp / media features while developing: Python (`pip`) for `wtsexporter`, and `ffmpeg` / `ffprobe` on `PATH` (or see [Helper binaries](#helper-binaries-and-environment-variables)).

### Linux packages

The Tauri desktop app needs a C toolchain and WebKit2GTK system libraries at **build time** and **runtime**. On Debian/Ubuntu:

```bash
sudo apt update
sudo apt install \
  build-essential pkg-config \
  libwebkit2gtk-4.1-dev libgtk-3-dev \
  libappindicator3-dev librsvg2-dev patchelf \
  libssl-dev libjavascriptcoregtk-4.1-dev libsoup-3.0-dev
```

On Fedora:

```bash
sudo dnf install \
  gcc pkgconf-pkg-config \
  webkit2gtk4.1-devel gtk3-devel \
  libappindicator-gtk3-devel librsvg2-devel \
  openssl-devel javascriptcoregtk4.1-devel libsoup3-devel
```

### WSL2

Use WSL2 with WSLg enabled (Windows 11) or an X server like VcXsrv (Windows 10). Keep the repository in the Linux filesystem (`~/repo/...`), not under `/mnt/c`. Install Rust and Node.js inside WSL rather than invoking Windows `cargo` or `npm.cmd`.

Set `DISPLAY` if using a standalone X server:

```bash
export DISPLAY=$(cat /etc/resolv.conf | grep nameserver | awk '{print $2}'):0
cargo tauri dev
```

## Clone and build

```bash
git clone https://github.com/bitrealm-io/message-vault.git
cd message-vault
cargo build --workspace
```

The first build compiles every workspace crate and can take several minutes.

Release profile:

```bash
cargo build --workspace --release
```

Release packaging uses `cargo tauri build`, which bundles the desktop app frontend, Rust backend, and all exporter libraries into a single platform installer. Exporters are linked as libraries. Standalone exporter CLIs can be built from this repo as well.

## Run the app

### One-time setup

```bash
cargo install tauri-cli --version "^2"
cd web && npm ci && cd ..
```

### Dev mode (hot reload)

```bash
cargo tauri dev
```

This starts the Vite dev server on `localhost:5173` and opens a native window. Editing files under `web/src/` triggers instant reload; changes to Rust code under `src-tauri/` recompile and restart the backend.

### Release mode (no hot reload, faster exports)

```bash
cargo build --release --workspace
./target/release/message-vault
```

Use a release build when testing real exports. Debug builds compile faster, but parsing, attachment hashing, and JSON serialization can be substantially slower.

### Vault server

The vault API runs on this machine; the website is Vite or Tauri on the host. Rust and FFmpeg on PATH are enough.

```bash
./scripts/run-vault-dev.sh                 # API at http://127.0.0.1:8080; keep data/ if present
./scripts/run-vault-dev.sh --reset          # wipe data/, empty vault
./scripts/run-vault-dev.sh --reset-demo     # wipe data/, seed sample inbox
./scripts/run-vault-dev.sh --sqlweb         # SQLite browser at http://127.0.0.1:8081 (`sqlite_web` on PATH)
cd web && npm run dev                   # website at http://localhost:5173 (proxies /v1)
# cargo tauri dev                       # desktop window instead of a browser
```

Create an account in the website when the vault is empty. For CLI import and export, create an API token under **Settings → Account**. To try the published image without compiling: [Try the vault](/vault/user/get-started/try-the-vault/). To build a release-shaped image from this checkout: [Operator Docker](/vault/developer/docker-compose/).

Settings persist in `export.ini` (working directory or next to the binary). Template: [`crates/core/message-vault-io-core/export.example.ini`](https://github.com/bitrealm-io/message-vault/blob/main/crates/core/message-vault-io-core/export.example.ini). Backup passwords are never written.

## Helper binaries and environment variables

Most export work runs in-process as Rust libraries. A few features still shell out to sibling tools:

| Helper | Used for |
|--------|----------|
| `wtsexporter` | WhatsApp extract step |
| `ffmpeg` / `ffprobe` | Media convert / compress |

Lookup order: beside the current executable → `lib/` / `cli/` next to the GUI (or `../lib/` from `cli/`) → legacy one directory up → directory in `MESSAGE_VAULT_IO_BIN` → `PATH`. WhatsApp also accepts an explicit `WTSEXPORTER` path.

| Variable | Purpose |
|----------|---------|
| `MESSAGE_VAULT_IO_BIN` | Directory that contains helper binaries |
| `WTSEXPORTER` | Full path to the WhatsApp extractor |

Local options:

- Install WhatsApp helper: `pip install 'whatsapp-chat-exporter>=0.13'`
- Install system `ffmpeg` / `ffprobe`, or copy them from a [release archive](https://github.com/bitrealm-io/message-vault/releases) next to your built GUI
- After `cargo build --workspace --release`, point helpers at the build output:

```powershell
# Windows PowerShell
$env:MESSAGE_VAULT_IO_BIN = "$PWD\target\release"
./target/release/message-vault.exe
```

```bash
# Linux / macOS
export MESSAGE_VAULT_IO_BIN="$PWD/target/release"
./target/release/message-vault
```

## Test

```bash
cargo test --workspace
```

Run a single crate:

```bash
cargo test -p go-sms-pro-exporter
```

Exporter smoke tests under `crates/*/tests/convert_smoke.rs` use committed fixtures. Personal phone backups are not required to run the suite.

Frontend (`web/`):

```bash
cd web && npm ci && npm run lint && npm test
```

## Docs site (optional)

User-facing docs are Astro Starlight under `docs/`, published to **https://bitrealm.io/** by `.github/workflows/docs.yml` (manual dispatch or push to `main` that touches `docs/**`).

```bash
cd docs
npm ci
npm run dev
```

With the current site layout, the local site is under **http://localhost:4321/vault/** (and related `/vault/user/` and `/vault/developer/` paths), not `/`.

Before publishing doc changes: `npm run check` and `npm run build`.

Command-line reference pages live under `docs/src/content/docs/vault/developer/reference/cli/`. Edit those files directly, then:

```bash
cd docs
npm run check
npm run build
```

### Publishing / custom domain

GitHub Pages on this repo serves the built site. The custom domain is `bitrealm.io` (`docs/public/CNAME`). After enabling Pages (source: GitHub Actions) and setting the domain, remove the same custom domain from any older Pages site that still claims it. The `bitrealm.io` A records should keep pointing at GitHub Pages. Add a verification TXT record only if GitHub’s Pages settings request one.

Guides live at:

- User Guide: `https://bitrealm.io/vault/user/`
- Developer docs: `https://bitrealm.io/vault/developer/`

Do not add a DNS name `vault`. GitHub Pages uses one custom domain: `bitrealm.io`. After GitHub shows a valid certificate, turn on **Enforce HTTPS** in the repository Pages settings. Leave the DNS records named `api`, `app`, and `cdn` alone.

## Workspace map

- **Libraries:** under `crates/libs/` — `ir`, `contacts`, `media`, `mail`, `sbr`, `phone`, `csv`, `obfuscate`; plus `message-vault-io-core`
- **Exporter crates:** under `crates/exporters/` — `imessage-ir-exporter`, `whatsapp-exporter`, `sms-backup-restore-exporter`, and experimental converters (GO SMS Pro, iMazing, OpenExtract, SMS Backup+)
- **GUI:** Tauri v2 app in `src-tauri/` with React + Vite frontend in `web/`
- **Server:** `message-vault-server` crate — the vault REST API, SQLite database, and web UI
- **CLI tools:** `vault-push`, `vault-pull`, `message-reexport` (package `message-reexport`), and individual exporter CLIs — built from this repo

This project is **AGPL-3.0**. `imessage-ir-exporter` still depends on `imessage-database` (**GPL-3.0-or-later**). Combined binaries are AGPL-3.0.

## Contribution rules

1. **Keep changes focused.** Prefer small PRs that do one job over mixed refactors and features.
2. **Match existing style.** Follow patterns already used in nearby crates; do not add drive-by renames or unrelated edits.
3. **Verify before you open a PR.** At minimum: `cargo fmt --all -- --check`, `cargo build --workspace`, and `cargo test --workspace`. If you touched docs under `docs/`, also run `npm run check` there. If you touched `web/`, also run `npm run lint` and `npm test` there.
4. **No secrets or personal data.** Do not commit passwords, vault keys, certificates, `.env` files with credentials, or real message backups. Use fixtures under `crates/*/tests/fixtures/` for test data.
5. **Respect licenses.** This project is AGPL-3.0. Call out GPL implications when changing `imessage-ir-exporter` or anything that pulls `imessage-database` into new binaries.
6. **Document CLI changes** on the matching page under `docs/src/content/docs/vault/developer/reference/cli/`.
7. **Put design depth in maintainer docs**, not on this page. Architecture, format contracts, GUI option matrices, and releases stay under [`docs/maintainers/`](https://github.com/bitrealm-io/message-vault/blob/main/docs/maintainers/README.md).
8. **Use a pull request template.** GitHub offers a default form plus feature and bug-fix templates under [`.github/`](https://github.com/bitrealm-io/message-vault/tree/main/.github).

## Troubleshooting

| Symptom | What to try |
|---------|-------------|
| `webkit2gtk` / `libsoup` not found | Install WebKit2GTK and GTK3 dev packages; see [Linux packages](#linux-packages) |
| "Could not find wtsexporter / ffmpeg / ffprobe" | Install the helper, put it on `PATH`, or set `MESSAGE_VAULT_IO_BIN` / `WTSEXPORTER` |
| Windows linker / `link.exe` errors | Install MSVC Build Tools with the C++ desktop workload |
| `cargo tauri` not found | Install with `cargo install tauri-cli --version "^2"` |
| Frontend not loading in dev mode | Run `cd web && npm ci` first, then `cargo tauri dev` |

## Further reading

- [Run from source](/vault/developer/run-from-source/)
- [Operator Docker](/vault/developer/docker-compose/)
- [Maintainer documentation index](https://github.com/bitrealm-io/message-vault/blob/main/docs/maintainers/README.md)
- [Development and releases](https://github.com/bitrealm-io/message-vault/blob/main/docs/maintainers/developing.md)
- [Converter capabilities](/vault/developer/formats/)
- [User Guide](/vault/user/)
