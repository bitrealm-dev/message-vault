---
title: Contributing
description: Set up a development environment, run tests, and open pull requests for Message Vault.
---

Thank you for contributing to Message Vault.

- **Product overview and install paths:** [User Guide](/vault/user/) and the repository [README](https://github.com/bitrealm-io/message-vault/blob/main/README.md)
- **Clone and run without the full checklist:** [Run from source](/vault/developer/run-from-source/)
- **Release-shaped Docker / published image:** [Operator Docker](/vault/developer/docker-compose/)
- **Architecture and maintainer notes:** [`docs/maintainers/`](https://github.com/bitrealm-io/message-vault/blob/main/docs/maintainers/README.md)

Before contributing, read the [Code of Conduct](https://github.com/bitrealm-io/message-vault/blob/main/CODE_OF_CONDUCT.md).

## Prerequisites

| Tool | Notes |
|------|--------|
| **Rust** | Stable toolchain via [rustup](https://rustup.rs/). Edition **2024** needs **Rust 1.85+**. CI uses the latest stable. |
| **Windows** | [Visual Studio Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/) with the "Desktop development with C++" workload (MSVC). |
| **macOS** | Xcode Command Line Tools (`xcode-select --install`). |
| **Linux** | C toolchain plus GUI system libraries (see [Linux packages](#linux-packages)). |
| **Node.js 22+** | Desktop frontend (`web/`) and the docs site (`docs/`). |
| **tauri-cli 2.x** | `cargo install tauri-cli --version "^2"` for `cargo tauri dev` / `cargo tauri build`. |

Optional while developing WhatsApp extract or media convert/compress: Python (`pip`) for `wtsexporter`, and `ffmpeg` / `ffprobe` on `PATH` (or see [Helper binaries](#helper-binaries-and-environment-variables)).

### Linux packages

The Tauri desktop app needs a C toolchain and WebKit2GTK at **build time** and **runtime**. On Debian/Ubuntu:

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

Use WSL2 with WSLg (Windows 11) or an X server such as VcXsrv (Windows 10). Keep the repository under the Linux filesystem (`~/…`), not `/mnt/c`. Install Rust and Node.js inside WSL; do not call Windows `cargo` or `npm.cmd` from a Linux checkout.

If a standalone X server is required:

```bash
export DISPLAY=$(cat /etc/resolv.conf | grep nameserver | awk '{print $2}'):0
cargo tauri dev
```

## Build and run (contributor path)

Day-to-day vault + website steps are on [Run from source](/vault/developer/run-from-source/) (`./scripts/run-vault-dev.sh`, `cd web && npm run dev`, `cargo tauri dev`).

Workspace compile (first run can take several minutes):

```bash
git clone https://github.com/bitrealm-io/message-vault.git
cd message-vault
cargo build --workspace
```

`src-tauri/` is **not** a workspace member. Format and build it separately when changing the desktop shell:

```bash
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo build --manifest-path src-tauri/Cargo.toml
```

Desktop packaging for installers is `cargo tauri build` (not a substitute for `cargo build --workspace`). The Tauri package name is `message-vault-io-tauri`; after a release-profile Tauri build, the binary under `src-tauri`’s target directory is named from that package (not a top-level `message-vault` crate).

Use a **release** profile when timing real exports. Debug builds compile faster but parsing, hashing, and serialization are slower.

## Helper binaries and environment variables

Most export work runs in-process as Rust libraries. A few features still shell out:

| Helper | Used for |
|--------|----------|
| `wtsexporter` | WhatsApp extract |
| `ffmpeg` / `ffprobe` | Media convert / compress |

Lookup order: beside the current executable → `lib/` / `cli/` next to the GUI (or `../lib/` from `cli/`) → legacy one directory up → directory in `MESSAGE_VAULT_IO_BIN` → `PATH`. WhatsApp also accepts an explicit `WTSEXPORTER` path.

| Variable | Purpose |
|----------|---------|
| `MESSAGE_VAULT_IO_BIN` | Directory that contains helper binaries |
| `WTSEXPORTER` | Full path to the WhatsApp extractor |

Local options:

- Install WhatsApp helper: `pip install 'whatsapp-chat-exporter>=0.13'`
- Install system `ffmpeg` / `ffprobe`, or copy them from a [release archive](https://github.com/bitrealm-io/message-vault/releases) next to the built GUI
- After a release build, point helpers at the directory that holds those binaries, for example:

```bash
export MESSAGE_VAULT_IO_BIN="$PWD/target/release"
```

Desktop form settings persist in `export.ini` (working directory or next to the binary). Passwords are never written. Example layout: [`crates/message-vault-io-gui/export.example.ini`](https://github.com/bitrealm-io/message-vault/blob/main/crates/message-vault-io-gui/export.example.ini) (legacy Slint GUI example; field names still illustrate the ini shape).

## Test before a pull request

```bash
cargo fmt --all -- --check
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo build --workspace
cargo test --workspace
```

Single crate:

```bash
cargo test -p go-sms-pro-exporter
```

Exporter smoke tests under `crates/*/tests/convert_smoke.rs` use committed fixtures. Personal phone backups are not required.

If `web/` changed:

```bash
cd web && npm ci && npm run lint && npm test
```

If `docs/` changed:

```bash
cd docs && npm ci && npm run check && npm run build
```

CI on `main` runs the Rust fmt/build/test path above (including `src-tauri` fmt) and the web lint/test jobs when those trees change. Docs deploy from [`.github/workflows/docs.yml`](https://github.com/bitrealm-io/message-vault/blob/main/.github/workflows/docs.yml).

## Docs site

The public site is Astro Starlight under `docs/`, with guidebook pages under `docs/src/content/docs/vault/…` (URLs such as `/vault/user/` and `/vault/developer/`). The Message Vault product landing at `/` is `docs/src/pages/index.astro`. Published origin: **https://bitrealm.io/**.

```bash
cd docs
npm ci
npm run dev
```

Local preview: **http://localhost:4321/** (company page) and **http://localhost:4321/vault/developer/** (and other `/vault/…` paths). Run `npm run check` and `npm run build` before merging doc edits.

CLI reference pages live under `docs/src/content/docs/vault/developer/reference/cli/`. Edit those files directly.

Pages custom domain and DNS cutover notes belong in maintainer ops, not in every PR. The committed apex name is `docs/public/CNAME` (`bitrealm.io`).

## Workspace map

- **`crates/libs/`** — shared libraries (`ir`, `ir-format`, `contacts`, `media`, `mail`, `sbr`, `phone`, `csv`, `obfuscate`, …)
- **`crates/exporters/`** — backup converters (iMessage, WhatsApp, SMS Backup & Restore, plus experimental sources)
- **`crates/core/message-vault-io-core/`** — shared config, jobs, and GUI/CLI form model
- **`crates/cli/`** — `vault-push`, `vault-pull` (libraries + optional CLI binaries)
- **`crates/vault/server/`** — `message-vault-server` (HTTP API + SQLite)
- **`crates/vault/demo-seed/`** — sample data for demo reset
- **`src-tauri/`** + **`web/`** — Tauri v2 desktop shell and Vite SPA (shell excluded from the workspace)
- **`crates/message-vault-io-gui/`** — legacy Slint GUI (still in the tree; not the primary desktop path)

## License

[`LICENSE.md`](https://github.com/bitrealm-io/message-vault/blob/main/LICENSE.md) is the **Fair Core License** (FCL-1.0-ALv2). Some `Cargo.toml` files still declare `AGPL-3.0-only` from earlier packaging; treat `LICENSE.md` as the repository license text until those crate metadata lines are aligned.

`imessage-ir-exporter` still depends on `imessage-database` / related GPL-licensed crates. Call that out when changing that exporter or anything that links those libraries into new binaries.

## Contribution rules

1. **Keep changes focused.** Prefer small PRs that do one job.
2. **Match existing style.** Follow nearby crates; avoid drive-by renames.
3. **Verify before opening a PR.** Use the checklist in [Test before a pull request](#test-before-a-pull-request).
4. **No secrets or personal data.** Do not commit passwords, vault keys, certificates, credential `.env` files, or real message backups. Use fixtures under `crates/*/tests/fixtures/`.
5. **Respect licenses.** See [License](#license).
6. **Document CLI changes** on the matching page under `docs/src/content/docs/vault/developer/reference/cli/`.
7. **Put design depth in maintainer docs.** Architecture and long format contracts stay under [`docs/maintainers/`](https://github.com/bitrealm-io/message-vault/blob/main/docs/maintainers/README.md).
8. **Use a pull request template.** Default plus feature and bug-fix forms live under [`.github/`](https://github.com/bitrealm-io/message-vault/tree/main/.github).

## Troubleshooting

| Symptom | What to try |
|---------|-------------|
| `webkit2gtk` / `libsoup` not found | Install the packages under [Linux packages](#linux-packages) |
| "Could not find wtsexporter / ffmpeg / ffprobe" | Install the helper, put it on `PATH`, or set `MESSAGE_VAULT_IO_BIN` / `WTSEXPORTER` |
| Windows linker / `link.exe` errors | Install MSVC Build Tools with the C++ desktop workload |
| `cargo tauri` not found | `cargo install tauri-cli --version "^2"` |
| Frontend blank in `cargo tauri dev` | `cd web && npm ci`, then retry |
| Docs links 404 locally | Open `/vault/…` paths (or `/`), not only the old apex article paths |

## Further reading

- [Run from source](/vault/developer/run-from-source/)
- [Operator Docker](/vault/developer/docker-compose/)
- [Formats](/vault/developer/formats/)
- [Maintainer index](https://github.com/bitrealm-io/message-vault/blob/main/docs/maintainers/README.md)
- [User Guide](/vault/user/)
