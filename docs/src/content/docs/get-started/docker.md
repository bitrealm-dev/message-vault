---
title: Docker
description: Run Message Vault with Docker Compose — dev profile for pull-main, release profile for a slim build.
---

Docker Compose packages the Rust import API and the Next.js UI so you do not
need a host Rust/Node toolchain. Two profiles share the same ports and data
layout; neither commits binaries to git or publishes a release image.

| Profile | Command | Best for |
|---------|---------|----------|
| **dev** (default) | `docker compose up` | Pull `main` often; bind-mounted source; hot reload for the web UI |
| **release** | `COMPOSE_PROFILES=release docker compose up --build` | Slimmer runtime image built from your checkout |

A committed [`.env`](https://github.com/bitrealm-dev/message-vault-rs/blob/main/.env) sets
`COMPOSE_PROFILES=dev` so bare `docker compose up` starts the toolchain service.
Enable only one profile at a time — both publish ports `3000` and `8080`.

## Install Docker

Message Vault’s images are **Linux containers**. You need Docker Engine or
Docker Desktop with the Compose v2 plugin (`docker compose`).

### Windows

1. Install [Docker Desktop for Windows](https://docs.docker.com/desktop/setup/install/windows-install/).
2. Leave **Linux containers** and the **WSL 2** engine enabled (Docker Desktop’s
   default). Docker Desktop runs Linux images in a small WSL 2 (or Hyper-V) VM;
   you do not need a separate Linux install or a Windows-container image.
3. Start Docker Desktop and wait until it reports that the engine is running.
4. In PowerShell, verify:

```powershell
docker version
docker compose version
```

PowerShell environment variables for this repo (set in the same session before
`docker compose`):

```powershell
# Prefer the slim release image (optional; default profile is "dev")
$env:COMPOSE_PROFILES = "release"

# Empty personal vault instead of the demo seed (optional)
$env:VAULT_MODE = "personal"

docker compose up --build
```

### Linux

1. Install Docker Engine and the Compose plugin for your distribution:
   [Install Docker Engine](https://docs.docker.com/engine/install/).
2. Follow Docker’s
   [post-install steps](https://docs.docker.com/engine/install/linux-postinstall/)
   so your user can run `docker` without `sudo` (typically membership in the
   `docker` group, then a new login session).
3. Verify:

```bash
docker version
docker compose version
```

[Docker Desktop for Linux](https://docs.docker.com/desktop/setup/install/linux/)
is an alternative if you prefer a GUI; Engine + Compose is enough for this
project.

## Dev profile (default)

```bash
git clone https://github.com/bitrealm-dev/message-vault-rs.git
cd message-vault-rs
docker compose up
```

First start builds a toolchain image (Rust + Node + FFmpeg) and may take several
minutes. Later starts reuse named volumes for `target/`, Cargo registry, and
`web/node_modules`.

Open <http://localhost:3000/login>. With the default `VAULT_MODE=demo`, sign in
as **`demo`**.

### After you pull `main`

```bash
git pull origin main
docker compose up
```

Web changes hot-reload. Restart the stack after Rust changes so `cargo run`
recompiles. Rebuild the toolchain image only when `Dockerfile.dev` or OS
packages change:

```bash
docker compose up --build
```

### Personal vault

```bash
VAULT_MODE=personal docker compose up
```

Creates an empty `data/` volume. Create an account in the UI, then use the
Import API token from **Settings → Account** with
[Message Exporters](https://bitrealm-dev.github.io/message-exporters/) /
`vault-push` against <http://127.0.0.1:8080>.

If a `vault.db` already exists on the volume, seeding is skipped.

## Release profile

Builds release artifacts from the current checkout into a smaller image (no
source bind mount). Override the default profile so only `vault-release` starts:

```bash
COMPOSE_PROFILES=release docker compose up --build
```

Same ports and `VAULT_MODE` behavior. After each `git pull`, run `--build`
again so the image matches your tree. Expect a multi-minute compile when Rust
or web dependencies change.

## Ports and security

Compose publishes both services on all host interfaces:

| Port | Service |
|------|---------|
| `3000` | Web UI |
| `8080` | Import API (`serve`) |

On the host machine use <http://localhost:3000/login> and
<http://127.0.0.1:8080/health>. From another device on the same LAN, use the
host’s LAN IP instead, for example:

```text
http://192.168.50.100:3000/login
http://192.168.50.100:8080/health
```

Import auth uses per-account tokens, not a host-wide secret. Limit inbound
TCP `3000` and `8080` on the host firewall to your trusted LAN/VPN subnet, and
do **not** add router port-forwarding unless you intentionally expose the
stack beyond the LAN. Example with `ufw` (adjust the subnet to match yours):

```bash
sudo ufw allow from 192.168.50.0/24 to any port 3000 proto tcp
sudo ufw allow from 192.168.50.0/24 to any port 8080 proto tcp
```

If `ufw` is inactive and other host services already rely on a different
firewall or network isolation, keep those controls in place rather than
enabling `ufw` globally without reviewing existing rules.

## Useful commands

```bash
# Health check (import API)
curl -sS http://127.0.0.1:8080/health

# Reset demo data (dev profile)
docker compose exec vault cargo run --release -- reset-demo
docker compose exec vault bash -lc 'cp config/config.docker.toml config/config.toml'
docker compose exec vault bash -lc 'cd web && npm run process-assets'

# Convert media after an import (dev)
docker compose exec vault bash -lc 'cd web && npm run process-assets'

# Shell in the running container
docker compose exec vault bash
```

For the release profile (`COMPOSE_PROFILES=release`), replace `vault` with
`vault-release` and call `message-vault-rs` instead of `cargo run --release --`.

## Layout inside the container

```text
/app/
  config/config.toml     # from config.docker.toml ([server] on 0.0.0.0:8080)
  data/                  # named volume vault-data
  demo/                  # committed demo bundle
  web/                   # Next.js app (dev) or standalone + tooling (release)
```

Host security and persistence live in Compose volumes and the host firewall —
the processes inside bind `0.0.0.0` so Docker can publish them on the LAN.

## Next steps

- [Try the demo](/get-started/try-the-demo/) (native install, no Docker)
- [First personal import](/get-started/first-personal-import/)
- [HTTP import API](/import/http-api/)
