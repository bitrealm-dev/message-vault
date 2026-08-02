---
title: Docker
description: Run Message Vault with Docker Compose — compose-dev for pull-main, compose-release for a slim build.
---

Docker Compose packages the Rust import API and the Next.js UI so you do not
need a host Rust/Node toolchain. Pick one compose file per machine — do not
run two at once (they share ports `3000` / `8080` and the `vault-data` volume
name).

| File | Command | Best for |
|------|---------|----------|
| **compose-dev.yml** (default) | `docker compose up` | Laptop; bind-mounted source; hot reload for the web UI |
| **compose-release.yml** | `docker compose -f compose-release.yml up --build` | Slimmer runtime image built from your checkout |

A committed [`.env`](https://github.com/bitrealm-dev/message-vault-rs/blob/main/.env) sets
`COMPOSE_FILE=compose-dev.yml` so bare `docker compose up` uses the toolchain
file. Override with `-f`, or change `COMPOSE_FILE` in `.env`.

Production Hub + nginx TLS for Bitrealm is maintained in a private ops repo
(`message-vault-ops`), not in this public tree.

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

PowerShell examples:

```powershell
# Slim release image built from this checkout
docker compose -f compose-release.yml up --build

# Empty personal vault instead of the demo seed (optional)
$env:VAULT_MODE = "personal"
docker compose up
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

## Dev (`compose-dev.yml`)

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

Creates an empty `data/` volume. Create an account in the UI, generate a Vault
Import API token under **Settings → Access** (copy from the one-time dialog),
then use it with
[Message Exporters](https://bitrealm-dev.github.io/message-exporters/)
(`message-exporter` Vault tab or `cli/vault-push`) against
<http://127.0.0.1:8080>.

If a `vault.db` already exists on the volume, seeding is skipped.

### Staging drop folder

Compose always bind-mounts the host directory `./staging` to `/app/staging`
inside the container. Copy a JSONL export onto the host — no `docker cp`
required:

```bash
mkdir -p staging/imessage
cp -a /path/to/your-export/. staging/imessage/
```

Then ingest from inside the container:

```bash
# Dev
docker compose exec vault cargo run --release -- ingest imessage \
  --account yourusername \
  --staging-dir staging/imessage

# Release (binary is already in the image)
docker compose -f compose-release.yml exec vault message-vault-rs ingest imessage \
  --account yourusername \
  --staging-dir staging/imessage
```

Contents of `staging/` are gitignored; only the empty directory placeholder is
tracked.

## Release (`compose-release.yml`)

Builds release artifacts from the current checkout into a smaller image (no
source bind mount):

```bash
docker compose -f compose-release.yml up --build
```

Same ports and `VAULT_MODE` behavior. After each `git pull`, run `--build`
again so the image matches your tree. Expect a multi-minute compile when Rust
or web dependencies change.

## Ports and security

Dev and release publish both services on all host interfaces:

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

### Auth mode (`VAULT_AUTH`)

- **`local` (default)** — username/password in the vault UI. Use for self-host
  and laptop demo/personal setups.
- **`hanko`** — Hanko Cloud (or self-hosted Hanko) for identity; the vault still
  uses the `mv_account_id` cookie after `POST /api/auth/hanko/session`. First
  sign-in auto-creates an account and sends the user through name/phone
  onboarding.

```env
VAULT_AUTH=hanko
NEXT_PUBLIC_HANKO_API_URL=https://<your-hanko-project>.hanko.io
HANKO_API_URL=https://<your-hanko-project>.hanko.io
```

`NEXT_PUBLIC_*` is baked at **image build** time for release/Hub images — rebuild
and push when the Hanko URL changes. Dev can rely on runtime env
(`web/.env.local` or process env). For a public HTTPS deployment, set Hanko’s
allowed origin to that exact app URL and prefer `Secure` session cookies
(`NODE_ENV=production`).

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

# Reset demo data (dev)
docker compose exec vault cargo run --release -- reset-demo
docker compose exec vault bash -lc 'cp config/config.docker.toml config/config.toml'
docker compose exec vault cargo run --release -- process-assets

# Convert media after an import (dev)
docker compose exec vault cargo run --release -- process-assets

# Shell in the running container
docker compose exec vault bash
```

For release, pass `-f compose-release.yml` and call `message-vault-rs` instead
of `cargo run --release --`.

## Layout inside the container

```text
/app/
  config/config.toml     # from config.docker.toml ([server] on 0.0.0.0:8080)
  data/                  # named volume vault-data
  staging/               # bind mount of host ./staging (JSONL drop folder)
  demo/                  # committed demo bundle
  web/                   # Next.js app (dev) or standalone + tooling (release)
```

Host security and persistence live in Compose volumes and the host firewall —
the processes inside bind `0.0.0.0` so Docker can publish them on the LAN.

## Next steps

- [Try the demo](/get-started/try-the-demo/) (native install, no Docker)
- [First personal import](/get-started/first-personal-import/)
- [HTTP import API](/import/http-api/)
