---
title: Install the vault server
description: Run the Message Vault server — Docker image, Compose, or build from source.
---

Message Vault has two components: a Rust server (import/export API and CLI) and a Next.js web UI (browse and search). You can run both with Docker, or build them from source.

## Docker (recommended)

The published Docker image bundles the server, web UI, and FFmpeg into one container. No Rust or Node toolchain needed.

### Quick start (prebuilt image)

```bash
docker run -d --name message-vault \
  -p 3000:3000 -p 8080:8080 \
  -e VAULT_MODE=demo \
  -v message-vault-data:/app/data \
  mbeisser1/message-vault:latest
```

Open http://localhost:3000/login. See the [Quick start](/get-started/quick-start/) page for the full walkthrough.

### Compose (build from checkout)

Clone the repo and use the bundled Compose file:

```bash
git clone https://github.com/bitrealm-dev/message-vault.git
cd message-vault
docker compose up
```

This uses `compose-dev.yml` (bind-mounts your checkout so edits take effect live). For the slim release image, set `COMPOSE_FILE=compose-release.yml` in `.env` or use `docker compose -f compose-release.yml up`.

See the [Docker guide](/get-started/docker/) for Windows/Linux setup, volume details, sqlite-web on port 8081, and troubleshooting.

## Build from source

If you prefer to run without containers, you need the Rust and Node toolchains.

### Requirements

- Rust 1.95 or newer (edition 2024; required by `rusqlite` / `libsqlite3-sys`)
- Node.js 20.9 or newer and npm
- A native C/C++ build toolchain
- Optional: FFmpeg for video/audio/HEIC conversion

Verify:

```text
rustc --version
cargo --version
node --version
npm --version
ffmpeg -version
```

Full OS-specific prerequisite steps (Visual Studio workloads on Windows, `apt` packages on Linux) are in the maintainer guide: [`docs/maintainers/development.md`](https://github.com/bitrealm-dev/message-vault/blob/main/docs/maintainers/development.md).

### Build

```bash
git clone https://github.com/bitrealm-dev/message-vault.git
cd message-vault
cargo build --workspace --release
```

For the web UI:

```bash
cd web
npm ci
npm run build
```

### Start

Start the import API:

```bash
cargo run --release -- serve
```

In a second terminal, start the web UI:

```bash
cd web
npm run dev
```

Open http://localhost:3000. Create an account and generate an Import API token under **Settings → Access**.

There is no standalone prebuilt binary for the vault server yet. The Docker image is the closest thing to a one-step install. The desktop app (Message Exporters) does ship as a prebuilt binary — see its [install guide](https://bitrealm-dev.github.io/message-exporters/get-started/install/).

## Next steps

- No real backup yet? [Try the demo](/get-started/try-the-demo/).
- [Quick start with Docker](/get-started/quick-start/) — 5-minute setup.
- [Docker guide](/get-started/docker/) — all Compose and volume options.
- Ready to import your own messages? [First personal import](/get-started/first-personal-import/).
