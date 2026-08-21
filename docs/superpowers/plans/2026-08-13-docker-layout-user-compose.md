# Docker layout and user Compose Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Move Dockerfiles and Compose files under `docker/`, add a curl-able published-image Compose sample, and point live docs and CI at the new paths.

**Architecture:** Build context stays the repository root. Compose files in `docker/` use `context: ..` and bind-mounts like `../data`. The user file `docker/compose.yml` has `image: bitrealm/message-vault` only — no Dockerfile, no `VAULT_AUTH`, no Hanko.

**Tech Stack:** Docker Compose v2, Docker Hub `bitrealm/message-vault`, GitHub Actions `docker/build-push-action`, Astro Starlight docs.

## Global Constraints

- User `docker/compose.yml`: `image:` only; `environment` contains only `DEMO_DATA`; volume `message-vault-data`; `name: message-vault`.
- Checkout Compose: no `HANKO_API_URL`; `VAULT_AUTH: ${VAULT_AUTH:-local}`.
- CI: `context: .` and `file: ./docker/Dockerfile`.
- Do not edit historical `docs/superpowers/plans/` or older specs. Do not resurrect `set-up-the-server/` (not in the live sidebar).
- Server and frontend code do not change.

---

### Task 1: Move Docker files into `docker/` and add the user Compose sample

**Files:**
- Create: `docker/compose.yml`, `docker/compose.dev.yml`, `docker/compose.release.yml`
- Move: `Dockerfile.release` → `docker/Dockerfile`, `Dockerfile.dev` → `docker/Dockerfile.dev`, `scripts/docker-entrypoint-*.sh` → `docker/entrypoint-*.sh`
- Modify: `docker/Dockerfile`, `docker/Dockerfile.dev`, `.env`, `.github/workflows/ci.yml`
- Delete: root `compose-dev.yml`, `compose-release.yml`, `Dockerfile.release`, `Dockerfile.dev`, `scripts/docker-entrypoint-*.sh`

- [ ] **Step 1: Create a branch and move the existing files**

```bash
git checkout -b docker/layout-user-compose
mkdir -p docker
git mv Dockerfile.release docker/Dockerfile
git mv Dockerfile.dev docker/Dockerfile.dev
git mv scripts/docker-entrypoint-release.sh docker/entrypoint-release.sh
git mv scripts/docker-entrypoint-dev.sh docker/entrypoint-dev.sh
```

- [ ] **Step 2: Write `docker/compose.yml` (published image)**

```yaml
# Published vault image. Save this file and run: docker compose up -d
# Docs: https://bitrealm.io/get-started/try-the-vault/
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
    volumes:
      - message-vault-data:/app/data
      - ./staging:/app/staging
    restart: unless-stopped

volumes:
  message-vault-data:
    name: message-vault-data
```

- [ ] **Step 3: Write `docker/compose.dev.yml` and `docker/compose.release.yml`**

Dev: `context: ..`, `dockerfile: docker/Dockerfile.dev`, volumes `..:/app`, `../staging`, `../data`, sqlite-web on `127.0.0.1:8081` with `../data`. `VAULT_AUTH: ${VAULT_AUTH:-local}`. No Hanko. `name: message-vault` so named volumes stay `message-vault_*`.

Release: `context: ..`, `dockerfile: docker/Dockerfile`, volumes `vault-data` and `../staging`. Same env as today minus Hanko.

- [ ] **Step 4: Point Dockerfiles at the new entrypoint paths**

In `docker/Dockerfile`, change comments to `docker/compose.release.yml` / `docker build -f docker/Dockerfile`, and:

```
COPY docker/entrypoint-release.sh /usr/local/bin/docker-entrypoint-release.sh
```

In `docker/Dockerfile.dev`:

```
ENTRYPOINT ["/bin/bash", "/app/docker/entrypoint-dev.sh"]
```

Comment: source is bind-mounted (see `docker/compose.dev.yml`).

- [ ] **Step 5: Update `.env` and CI; delete root Compose files**

`.env`: `COMPOSE_FILE=docker/compose.dev.yml`. Comments name `docker/compose.dev.yml` and `docker compose -f docker/compose.release.yml up --build`. Keep `VAULT_AUTH=local` and existing commented Hanko lines.

CI: `file: ./docker/Dockerfile`.

```bash
git rm compose-dev.yml compose-release.yml
```

- [ ] **Step 6: Verify Compose resolution and commit**

From the repository root:

```bash
docker compose config --quiet
docker compose -f docker/compose.release.yml config --quiet
docker compose -f docker/compose.yml config --quiet
```

Expected: no error. Dev config bind-mounts the repo root (`..` resolved), not `docker/`. User config has `image: bitrealm/message-vault` and no `VAULT_AUTH`.

```bash
git add docker .env .github/workflows/ci.yml
git commit -m "chore(docker): move Compose and Dockerfiles under docker/"
```

---

### Task 2: Update live docs and in-repo pointers

**Files:**
- Modify: `docs/src/content/docs/get-started/try-the-vault.md`
- Modify: `README.md`
- Modify: `docs/src/content/docs/how-to/update.md`
- Modify: `docs/src/content/docs/developer/docker-compose.md`
- Modify: `CLAUDE.md`, `CONTRIBUTING.md`, `crates/vault/server/README.md`
- Modify: `scripts/build-static.sh`, `web-next/README.md`, `docs/maintainers/roadmap.md`

- [ ] **Step 1: Try the vault + README — keep `docker run`, add curl Compose**

After the `docker run` block, add Compose:

```bash
mkdir message-vault && cd message-vault
curl -fsSL -o compose.yml \
  https://raw.githubusercontent.com/bitrealm-io/message-vault/main/docker/compose.yml
docker compose up -d
```

Link the GitHub file `docker/compose.yml`. Same volume `message-vault-data` as `docker run`.

- [ ] **Step 2: Update the vault — Compose pull path**

For people using the sample file:

```bash
docker compose pull
docker compose up -d
```

Keep the `docker run` replace steps. Checkout rebuilds still point at Operator Docker. Hub pin example: `bitrealm/message-vault:0.7.3` (no `v`; matches CI semver tags).

- [ ] **Step 3: Operator Docker + remaining pointers**

Table: `docker/compose.dev.yml` (default via `.env`) and `docker/compose.release.yml`. State that `docker/compose.yml` is the published-image sample and is not used from a clone.

`CLAUDE.md`: `docker compose -f docker/compose.release.yml up --build`. Image built from `docker/Dockerfile`.

`CONTRIBUTING.md` vault section: `docker compose up` still works from the repo root; mention the `docker/` folder.

- [ ] **Step 4: Grep live pointers and commit**

```bash
rg 'Dockerfile\.release|compose-dev\.yml|scripts/docker-entrypoint' \
  README.md CLAUDE.md CONTRIBUTING.md docs/src/content/docs \
  .github/workflows/ci.yml crates/vault/server/README.md
```

Expected: no matches in those live files (historical specs/plans may still mention old paths).

```bash
git add README.md CLAUDE.md CONTRIBUTING.md docs scripts/build-static.sh \
  web-next/README.md crates/vault/server/README.md
git commit -m "docs: point Docker instructions at docker/"
```

---

### Task 3: Verify the layout

- [ ] **Step 1: Confirm user Compose and CI path**

```bash
rg -n 'VAULT_AUTH|HANKO' docker/compose.yml
rg -n 'file:' .github/workflows/ci.yml
```

Expected: no auth vars in the user file; CI `file: ./docker/Dockerfile`.

- [ ] **Step 2: Optional image build if Docker is available**

```bash
docker build -f docker/Dockerfile -t message-vault:layout-test .
```

Expected: success. Skip only if the daemon is down; still require Compose `config` from Task 1.

- [ ] **Step 3: Commit any remaining fixes**
