---
title: Run from source
description: Build and run the vault and the desktop app from a git checkout.
---

This page is enough to run Message Vault from a clone. Linux packages, WSL, tests, and pull-request rules stay in [CONTRIBUTING.md](https://github.com/bitrealm-dev/message-vault/blob/main/CONTRIBUTING.md).

## Prerequisites

- **Rust 1.85+** (edition 2024) via [rustup](https://rustup.rs/)
- **Node.js 22+** for the desktop app frontend (`web/`)
- [tauri-cli](https://v2.tauri.app/start/prerequisites/) 2.x: `cargo install tauri-cli --version "^2"`

Optional while developing WhatsApp and media convert: Python for `wtsexporter`, and `ffmpeg` / `ffprobe` on `PATH`.

## Clone

```bash
git clone https://github.com/bitrealm-dev/message-vault.git
cd message-vault
```

## Run the vault

```bash
./scripts/run-vault-dev.sh                 # API at http://127.0.0.1:8080; keep data/ if present
./scripts/run-vault-dev.sh --reset-demo    # wipe data/, seed sample inbox
cd web && npm run dev                      # website at http://localhost:5173 (proxies /v1)
```

Open **http://localhost:5173** for the Vite UI, or **http://127.0.0.1:8080** if `static/` was built with `./scripts/build-static.sh`. `--reset` wipes `data/` without seeding. `--sqlweb` starts a SQLite browser on port **8081** when `sqlite_web` is on `PATH`.

A release-shaped Docker image from this checkout is on [Operator Docker](/developer/docker-compose/). Trying the published image without compiling is on [Try the vault](/get-started/try-the-vault/).

## Run the desktop app

One-time frontend install:

```bash
cd web && npm ci && cd ..
```

Dev window (hot reload):

```bash
cargo tauri dev
```

Point the app at **http://127.0.0.1:8080** (or the URL where the vault is listening).

A release-shaped binary (faster on real backups):

```bash
cargo build --release --workspace
./target/release/message-vault
```

On Linux the desktop app also needs WebKit2GTK and GTK3 at build and runtime. The package lists are in CONTRIBUTING.md.

## Next

- [CONTRIBUTING.md](https://github.com/bitrealm-dev/message-vault/blob/main/CONTRIBUTING.md) — tests, formatting, Linux packages, WSL
- [Command-line tools](/reference/cli/) — exporter and vault-push/pull binaries
- [Install the desktop app](/get-started/install-the-desktop-app/) — GitHub Releases instead of compiling
