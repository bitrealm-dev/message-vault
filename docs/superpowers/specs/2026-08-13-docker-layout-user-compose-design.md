# Docker layout and published-image Compose file

## Context

The vault ships as the Docker Hub image `bitrealm/message-vault`. People who want to try it today copy a `docker run` command from the README or [Try the vault](https://bitrealm.dev/get-started/try-the-vault/). Docker Compose lives only in this repository, and both files **build** from the checkout:

| File today | What it does |
|---|---|
| `compose-dev.yml` | Bind-mounts the repo, hot reload, SQLite browser on port 8081 |
| `compose-release.yml` | `build:` `Dockerfile.release` — production-shaped image from this tree |

`compose-release.yml` is the wrong file to send users. It requires `Dockerfile.release` and a git clone. Ports, environment variables, and volumes are what a self-hoster actually wants to edit, and those belong in a Compose file that **pulls** the published image.

Dockerfiles and Compose files currently sit at the repository root next to Cargo, Tauri, and the docs site. That is valid Docker convention for a single-service repo. It is noisy in this monorepo.

Hanko (passkeys via Hanko Cloud, `VAULT_AUTH=hanko` + `HANKO_API_URL`) is how the Bitrealm VPS authenticates. The published-image path is a local service: username and password in SQLite. The server already treats any `VAULT_AUTH` other than `hanko` as local (`AuthMode::from_env` in `crates/vault/server/src/config.rs`). The sample Compose file should not advertise a third-party identity provider.

## Goals

- One Compose file people can `curl` without cloning. It uses `image: bitrealm/message-vault` and exposes port, data volume, optional staging directory, and `DEMO_DATA`.
- All Docker build and Compose files live under `docker/`.
- Contributors still run `docker compose up` from the repository root (dev stack).
- CI still builds `bitrealm/message-vault` from the repository root as context.
- Auth on the published-image stack is local username/password. Compose does not pass Hanko variables.
- `docker run -v message-vault-data:…` and the user Compose file share the same Docker volume name so switching methods does not look like data loss.

## Non-goals

- Native `message-vault-server` release archives (SQLite would support them; this change does not ship them).
- Moving Hanko out of the server or the Vite login screen. Hanko stays in the product for the VPS and for checkout testing.
- Changing Hub image name, tags, or the entrypoint seed logic (`DEMO_DATA` on an empty volume).
- HTTP redirects, a new docs information architecture, or rewriting historical specs/plans under `docs/superpowers/`.
- Publishing Compose on Docker Hub (Hub does not host Compose files).

## Layout

```
docker/
  compose.yml              # users: pull Hub image
  compose.release.yml      # checkout: build production-shaped image
  compose.dev.yml          # checkout: bind-mount + hot reload
  Dockerfile               # today’s Dockerfile.release (CI + Hub)
  Dockerfile.dev
  entrypoint-release.sh    # today’s scripts/docker-entrypoint-release.sh
  entrypoint-dev.sh        # today’s scripts/docker-entrypoint-dev.sh
```

Delete the root copies: `compose-dev.yml`, `compose-release.yml`, `Dockerfile.release`, `Dockerfile.dev`, `scripts/docker-entrypoint-release.sh`, `scripts/docker-entrypoint-dev.sh`.

`.dockerignore` stays at the repository root. The image build context is still the repo root (`COPY web/`, `crates/`, `schema/`, `config/`). Ignore rules apply to that context, not to the Dockerfile’s directory.

No stub Dockerfiles or Compose files at the root. Update every live pointer (CI, README, CONTRIBUTING, CLAUDE.md, developer docs) instead.

## User Compose (`docker/compose.yml`)

People save this file in a folder they own. They do not need this repository.

```yaml
# Published vault image. Save this file and run: docker compose up -d
# Docs: https://bitrealm.dev/get-started/try-the-vault/
#
# Pin a release: change :latest to :0.7.3 (no "v").
# Empty vault: DEMO_DATA=false docker compose up -d
# JSONL drop: copy files into ./staging

name: message-vault

services:
  vault:
    image: bitrealm/message-vault:latest
    ports:
      - "8080:8080"
    environment:
      DEMO_DATA: ${DEMO_DATA:-true}
      VAULT_AUTH: local
    volumes:
      - message-vault-data:/app/data
      - ./staging:/app/staging
    restart: unless-stopped

volumes:
  message-vault-data:
```

Rules for this file:

- `image:` only. No `build:`, no Dockerfile.
- `VAULT_AUTH: local` is a literal, not `${VAULT_AUTH:-local}`. A leftover `VAULT_AUTH=hanko` in the user’s shell must not switch a laptop vault onto Hanko Cloud.
- No `HANKO_API_URL`. The container default is already local if the variable is absent; the Compose file still sets `VAULT_AUTH` so the file documents the mode.
- Volume name `message-vault-data` matches `docker run -v message-vault-data:/app/data`.
- `name: message-vault` keeps the Compose project name stable if the folder is not called `message-vault`.
- `./staging` is optional JSONL drop, same idea as today’s release Compose. Compose creates the host directory if missing.
- Tag `latest` in the committed file. Docs mention pinning `:0.7.3` (Hub semver tags have no `v` prefix; git tags do).

How users start it (docs + README, after the existing `docker run` one-liner):

```bash
mkdir message-vault && cd message-vault
curl -fsSL -o compose.yml \
  https://raw.githubusercontent.com/bitrealm-dev/message-vault/main/docker/compose.yml
docker compose up -d
```

Open http://localhost:8080. Sign in as `demo` with an empty password when `DEMO_DATA` seeded the volume.

## Checkout Compose

Run these from the **repository root**. Relative paths are from `docker/`, so bind-mounts and build context use `..`.

`.env` (repo root, still read by Compose):

```
COMPOSE_FILE=docker/compose.dev.yml
```

Bare `docker compose up` from a clone keeps today’s laptop stack.

### `docker/compose.dev.yml`

Same services as today’s `compose-dev.yml` (vault + sqlite-web on `127.0.0.1:8081`), with paths adjusted:

```yaml
build:
  context: ..
  dockerfile: docker/Dockerfile.dev
environment:
  DEMO_DATA: ${DEMO_DATA:-true}
  VAULT_AUTH: ${VAULT_AUTH:-local}
volumes:
  - ..:/app
  - ../staging:/app/staging
  - ../data:/app/data
```

SQLite browser mounts `../data`. No `HANKO_API_URL` in `environment:`.

### `docker/compose.release.yml`

```yaml
build:
  context: ..
  dockerfile: docker/Dockerfile
environment:
  DEMO_DATA: ${DEMO_DATA:-true}
  VAULT_AUTH: ${VAULT_AUTH:-local}
volumes:
  - vault-data:/app/data
  - ../staging:/app/staging
```

Named volume stays `vault-data` (project-prefixed as today for anyone already using the checkout release stack). No `image:`. No Hanko variables.

Checkout files use `${VAULT_AUTH:-local}` so a contributor can export `VAULT_AUTH=hanko` **and** add `HANKO_API_URL` back to that file when testing Hanko. The committed YAML does not include Hanko.

## Dockerfiles and entrypoints

`docker/Dockerfile` is today’s `Dockerfile.release` with:

- Comments that name `docker/compose.release.yml` and `docker build -f docker/Dockerfile`.
- `COPY docker/entrypoint-release.sh` into the image (same destination path `/usr/local/bin/docker-entrypoint-release.sh` is fine).

Build context remains `.` (repository root). CI:

```yaml
context: .
file: ./docker/Dockerfile
```

`docker/Dockerfile.dev` entrypoint becomes `/app/docker/entrypoint-dev.sh` because the dev container still bind-mounts the repo root at `/app`.

Entrypoint scripts keep the same seed/`DEMO_DATA` behavior. Only their on-disk path changes.

## Auth

| Audience | `VAULT_AUTH` | Hanko in Compose |
|---|---|---|
| User `docker/compose.yml` | literal `local` | no |
| Checkout Compose | `${VAULT_AUTH:-local}` | no |
| Server if unset | local | n/a |
| Bitrealm VPS | set in private `message-vault-ops` | yes, there |

Do not document Hanko on Try the vault, README, or Operator Docker. Local username and password only, same as the User Guide rework spec.

`.env` in this repo keeps `VAULT_AUTH=local`. Remove `HANKO_API_URL` from Compose `environment:` blocks. Commented Hanko lines in `.env` may remain as a contributor hint; they do nothing until someone maps the variable into a Compose file again.

## Docs and in-repo pointers

Live pages and files to update (paths as of this spec):

| Place | Change |
|---|---|
| `docs/src/content/docs/get-started/try-the-vault.md` | Keep `docker run`. Add the `curl` + `docker compose up -d` path. Link the GitHub file `docker/compose.yml`. |
| `README.md` | Same: `docker run` first, then Compose `curl`. |
| `docs/src/content/docs/how-to/update.md` | For Compose users: `docker compose pull && docker compose up -d` in the folder that holds their `compose.yml`. Keep the `docker run` replace steps. Point checkout rebuilds at Operator Docker. |
| `docs/src/content/docs/developer/docker-compose.md` | Table uses `docker/compose.dev.yml` and `docker/compose.release.yml`. State that `docker/compose.yml` is the published-image sample, not used from a clone. |
| `CLAUDE.md`, `CONTRIBUTING.md`, crate README that names Compose | New paths. |
| `.github/workflows/ci.yml` | `file: ./docker/Dockerfile`. |

Do not resurrect deleted User Guide paths under `set-up-the-server/`. If a leftover file there still names root Compose paths, fix it only if it is still in the live Starlight sidebar; otherwise leave it for the existing docs-rework deletion.

Historical `docs/superpowers/plans/` and older specs are not updated.

## Verification

- `docker build -f docker/Dockerfile -t message-vault:layout-test .` from the repo root succeeds (same as today’s release image).
- From repo root, `docker compose config` (with the committed `.env`) resolves `docker/compose.dev.yml` and shows bind-mounts of the repo root, not of `docker/`.
- `docker compose -f docker/compose.release.yml config` shows `context` as the repo root and `dockerfile` `docker/Dockerfile`.
- User file `docker/compose.yml` has `image: bitrealm/message-vault`, `VAULT_AUTH: local`, no `HANKO_API_URL`, volume `message-vault-data`.
- Grep of live docs, README, CI, CLAUDE.md, CONTRIBUTING.md has no `Dockerfile.release`, root `compose-dev.yml`, or `scripts/docker-entrypoint-*.sh`.
- No server or frontend code change is required for this layout.

## Out of scope follow-ups

- GitHub Release archives of `message-vault-server` plus `static/` and a config template, for people who do not want Docker. SQLite does not require a container; CI does not ship that archive today.
