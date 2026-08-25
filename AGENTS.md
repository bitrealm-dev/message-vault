# Cursor Rules for This Project

## Communication Style

- Write instructions in plain, direct English at high school reading level.
- Avoid jargon, obtuse language, and clever diction.
- Explain what you're doing and why in simple terms.
- If something is complex, break it into clear steps.
- Use concrete examples instead of abstract descriptions.

## Git Workflow

### Always Before Pushing

1. Run `git fetch` to sync with origin
2. Run `git branch -a` to see all local and remote branches
3. Run `gh pr list` to check the status of open PRs
4. Run `gh pr view <number>` to check a specific PR's status before pushing
5. Do not assume the state of any PR - verify it with `gh` commands

### Making Changes

- Create a new branch or worktree for all code changes
- Never commit directly to main/master
- Branch names should be descriptive (e.g., `feature/add-auth`, `fix/parsing-bug`)

### Submitting Work

- Use `gh pr create` to open a pull request
- Do not merge PRs yourself unless explicitly instructed
- Use `gh pr view <number>` to check PR status before any operations
- Include a clear description of what the PR does and why

## Code Changes

- Test code locally before pushing
- Keep commits focused and logical
- Write clear commit messages that explain the change

## Tools to Use

- Use `gh` CLI commands to check branches and PR status (not guesswork)
- Use `gh pr list` to see all open PRs
- Use `gh pr view <PR_NUMBER>` to see specific PR details
- Use `gh pr create` to open new PRs
- Use `gh pr checks <PR_NUMBER>` to see test results
- Use the **GitHub MCP** (`plugin-github-github`) for issues, PR read/search, reviews, and GitHub code search when the server is authenticated; call `mcp_auth` if discovery fails, otherwise fall back to `gh`. See [`.cursor/rules/github-mcp.mdc`](.cursor/rules/github-mcp.mdc).
- Use the **Playwright MCP** (`plugin-playwright-playwright`) to verify browser UI after `web/` changes: navigate to the Vite app (prefer `http://127.0.0.1:5173` with the vault on `:8080`), take a snapshot, then click/type as needed. See [`.cursor/rules/playwright-mcp.mdc`](.cursor/rules/playwright-mcp.mdc). Desktop-only screens gated by `isTauri()` still need the Tauri window or unit tests — Playwright against Vite alone cannot exercise them.

## When Uncertain

- Ask for clarification rather than guessing
- Use `gh` commands to check current state
- Check existing conventions in the codebase before inventing new ones

## Message Vault Repository

This repository is **message-vault**. Cargo package names may still say `message-vault-io`; that is a package namespace, not the repo name. Public docs and GitHub live under `bitrealm-io`.

The product has two pieces:

- **The vault** — `message-vault-server`. Stores messages in SQLite (`data/vault.db`), serves `/v1/*`, and can host the website from `static/`. Run it with `./scripts/run-vault-dev.sh` (http://127.0.0.1:8080) or Docker. Login is a local vault account, not a cloud account.
- **The desktop app** — Tauri v2 around the Vite SPA in `web/`. Reads phone backups, writes JSONL, and imports into a running vault. Browse and search also work in the browser against the vault; importing a backup needs the desktop app.

### Technology stack

| Piece                  | Stack                                                                                                                             |
|------------------------|-----------------------------------------------------------------------------------------------------------------------------------|
| Language (Rust crates) | Rust 1.85+ (edition 2024). CI uses latest stable.                                                                                 |
| Vault server           | Tokio + Axum 0.8 HTTP API. sqlx Any: SQLite (bundled) by default, Postgres via `[database] url`. TOML config. Argon2 passwords, JWT sessions. |
| Database               | SQLite file at `data/vault.db`. Table SQL lives in `schema/sql/`. Schema changes bump `SCHEMA_VERSION` in `db/schema.rs`; old vaults are rebuilt empty and need a fresh import. |
| Desktop app            | Tauri 2 native window. Vite 6 + React 19 + TypeScript SPA in `web/`. React Router 7, React Aria, Tailwind CSS 4. Vitest + ESLint. |
| Website                | Same `web/` SPA. Dev server on port 5173. Production copy in `static/`, served by the vault on port 8080.                         |
| Node                   | Node.js 22+ for `web/`, `docs/`, and Docker frontend builds.                                                                      |
| Docs site              | Astro 7 + Starlight, published to GitHub Pages at bitrealm.io.                                                                    |
| Packaging              | Docker (Node 22 + Rust image). GitHub Actions on `v*` tags builds the image and Tauri installers.                                 |
| Helpers on PATH        | `ffmpeg` / `ffprobe` for media. `wtsexporter` (Python) for WhatsApp. `gh` for GitHub.                                             |
| Not the product path   | Legacy Slint 1.17 GUI (`crates/message-vault-io-gui/`). Restored Next.js 16 browse app (`web-next/`, better-sqlite3).             |

### Directory map (`tree -L 2 message-vault`)

```text
message-vault
├── config/                 # vault server config templates (copy example → config.toml)
├── crates/                 # Rust workspace (src-tauri is excluded)
│   ├── cli/                # vault-push and vault-pull (import/export a running vault)
│   ├── core/               # shared form model, jobs, export.ini
│   ├── exporters/          # backup parsers (iMessage, WhatsApp, SMS, experimental)
│   ├── libs/               # shared libraries (ir, ir-format, reexport, contacts, media, …)
│   ├── message-vault-io-gui/  # legacy Slint desktop GUI (not the product path)
│   └── vault/              # message-vault-server (HTTP API + SQLite) and demo-seed
├── docker/                 # Dockerfile and Compose for a release-shaped vault image
├── docs/                   # Astro Starlight site (bitrealm.io)
│   ├── img/                # images used in README / docs
│   ├── public/             # CNAME and other files copied as-is
│   └── src/                # landing page + User Guide + Developer guidebook
│       └── assets/architecture/  # C4 PlantUML sources and exported SVGs
├── schema/                 # SQLite schema for the vault
│   └── sql/                # CREATE TABLE sources embedded by the server
├── scripts/                # host helpers (run-vault-dev, build-static, schema sync)
│   ├── deprecated/         # old Slint packaging scripts
│   └── test/               # scripted test helpers
├── src-tauri/              # Tauri v2 native shell (not a workspace member)
│   ├── capabilities/       # Tauri permission manifests
│   ├── icons/              # desktop app icons
│   └── src/                # Tauri commands wrapping exporters / push / pull
├── tests/                  # workspace-level tests
│   └── fixtures/           # committed schema/API fixtures (no personal backups)
├── web/                    # Vite + React SPA: website and desktop UI
│   └── src/                # screens, components, vault API client, Tauri wrappers
└── web-next/               # restored historical Next.js browse UI (not the product GUI)
    └── src/                # App Router pages that read vault.db via better-sqlite3
```

```text
# ❌ BAD — crates/message-vault-io-gui or web-next with v2 schema IS NOT the product!
# ✅ GOOD — product UI is web/ + src-tauri/; vault API is crates/vault/server/; schema v3 only
```

### First time setup

Do this once on a new machine. Then follow **Run the vault (development)**.

**1. OS toolchain**

| OS             | What to install                                                                                                                          |
|----------------|------------------------------------------------------------------------------------------------------------------------------------------|
| Linux (Ubuntu) | C compiler, OpenSSL, test libs, WebKit/GTK for Tauri, ffmpeg (commands below)                                                            |
| macOS          | Xcode Command Line Tools: `xcode-select --install`                                                                                       |
| Windows        | [Visual Studio Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/) with the "Desktop development with C++" workload |
| WSL2           | Keep the clone under `~/…`, not `/mnt/c`. Install Rust and Node inside WSL. Prefer WSLg (Windows 11).                                    |

Ubuntu packages:

```bash
sudo apt update
sudo apt install -y curl git build-essential pkg-config libssl-dev
sudo apt install -y libfontconfig1-dev libxkbcommon-dev   # cargo test --workspace
sudo apt install -y \
  libwebkit2gtk-4.1-dev libgtk-3-dev \
  libappindicator3-dev librsvg2-dev patchelf \
  libjavascriptcoregtk-4.1-dev libsoup-3.0-dev
sudo apt install -y ffmpeg
```

**2. Rust 1.85+** (edition 2024). Do not use the distro `apt` package.

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
cargo install tauri-cli --version "^2"
```

**3. Node.js 22+**. Distro Node is usually too old. nvm example:

```bash
curl -fsSL https://raw.githubusercontent.com/nvm-sh/nvm/v0.40.3/install.sh | bash
# new shell, then:
nvm install 22
nvm use 22
```

**4. Optional helpers**

```bash
sudo apt install -y pipx && pipx ensurepath
pipx install 'whatsapp-chat-exporter[android_backup,crypt15]'   # wtsexporter
pipx install sqlite-web                                          # --sqlweb on port 8081
```

**5. Clone and install the frontend**

```bash
git clone https://github.com/bitrealm-io/message-vault.git
cd message-vault
cd web && npm ci && cd ..
```

First `cargo build --workspace` and first `cargo tauri dev` each take several minutes. `config/config.toml` is created from `config/config.toml.example` on the first `./scripts/run-vault-dev.sh` if it is missing.

### Run the vault (development)

Work from the repository root. The vault process must be running before the website or desktop app can sign in. First compile of the server and of Tauri each take several minutes.

**Terminal 1 — vault API** (leave this running)

```bash
./scripts/run-vault-dev.sh                 # keep data/ if present; empty vault if none
./scripts/run-vault-dev.sh --reset-demo    # wipe data/, seed the sample inbox (needs ffmpeg)
./scripts/run-vault-dev.sh --reset         # wipe data/, start empty
./scripts/run-vault-dev.sh --sqlweb        # also SQLite browser at http://127.0.0.1:8081
```

`--reset` and `--reset-demo` cannot be combined. `--reset-demo` also rewrites `config/config.toml` from the example (CORS for Vite `:5173` enabled). Later sessions omit `--reset-demo` so the existing database stays.

API: **http://127.0.0.1:8080**. After `--reset-demo`, sign in as username `demo` with an empty password. Otherwise create an account in the UI.

Restart terminal 1 after edits under `crates/vault/server/` (debug `cargo run`; no hot reload).

**Run on Postgres (optional)** — the server defaults to SQLite at `data/vault.db`; a `postgres://…` (or `sqlite://…`) URL selects the engine instead, passed with `serve --db-url` or set as `[database] url` in `config/config.toml`.

```bash
docker compose -f docker-compose.pg.yml up -d   # dev-only Postgres on 127.0.0.1:5432 (vault/vault/vault)
cargo run -p message-vault-server -- serve --config config/config.toml --db-url postgres://vault:vault@127.0.0.1:5432/vault
```

`docker compose -f docker-compose.pg.yml down` stops the database. Do not run this and `./scripts/run-vault-dev.sh` at once unless one uses a different `[server] bind` — both serve on 127.0.0.1:8080.

**Terminal 2 — UI** (pick one)

```bash
cd web && npm ci && cd ..    # first time, or after web/package-lock.json changes
cargo tauri dev              # desktop window; starts Vite itself
```

Or, browser only (no Tauri):

```bash
cd web && npm run dev        # http://localhost:5173, proxies /v1 to :8080
```

Do not run `npm run dev` and `cargo tauri dev` at the same time. Point the app at **http://127.0.0.1:8080** (not `localhost` — that can resolve to IPv6, which the vault does not listen on). `web/` and `src-tauri/` usually reload; restart `cargo tauri dev` if they do not.

Optional: `./scripts/build-static.sh` copies `web/dist` to `static/` so the vault serves the UI at http://127.0.0.1:8080 without Vite. Do not run `docker compose -f docker/compose.release.yml` and the host script at once; they both use port 8080.

### Build, format, and test

#### Backend

Run these from the repository root unless a `cd` is shown.

Rust formatter is `rustfmt`. CI does not run Clippy. `src-tauri/` is not a workspace member, so format it with `--manifest-path`.

```bash
# Check format (what CI runs)
cargo fmt --all -- --check
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check

# Rewrite Rust (workspace + src-tauri) and web/ (Biome)
./scripts/format-all.sh

# Clippy (workspace except Slint GUI + src-tauri) and web lint (Biome).
# Warnings do not fail.
./scripts/lint-all.sh

cargo build --workspace
cargo test --workspace
cargo test -p sms-backup-restore-exporter   # one crate
cargo build --manifest-path src-tauri/Cargo.toml

# Postgres engine tests (skip unless a dev Postgres is reachable)
docker compose -f docker-compose.pg.yml up -d
MV_TEST_POSTGRES_URL=postgres://vault:vault@127.0.0.1:5432/vault cargo test -p message-vault-server
```

#### Frontend

Frontend (`web/`) — Biome (`web/biome.json`) lints and formats TypeScript, JavaScript, CSS, JSON, and HTML. TypeScript (`npm run build` runs `tsc` then Vite). CI runs `biome ci .` (lint and format drift fail). Prefer a real fix over `biome-ignore`. Prefix unused bindings with `_`.

```bash
cd web
npm ci                    # first time, or after package-lock.json changes
npm run lint              # biome lint .
npm run format            # rewrite format + import order
npm run format:check      # format + import order, no write
npm test                  # vitest run (src/**/*.{test,spec}.{ts,tsx})
npm run test:watch
npm run build             # tsc && vite build
npm run dev               # Vite on http://localhost:5173 (proxies /v1 to :8080)
```

From the repository root, `./scripts/format-all.sh` runs rustfmt then the web formatter. `./scripts/lint-all.sh` runs Clippy (workspace except `message-vault-io-gui`, plus `src-tauri`) then the web linter. `./scripts/check-pr.sh` calls `format-all.sh`, then build/test/lint. CI does not run Clippy.

Do not start a separate `npm run dev` while `cargo tauri dev` is running. Tauri starts Vite itself.

#### Docs

Docs (`docs/`) not the product UI, but CI-adjacent when that tree changes:

```bash
cd docs && npm ci && npm run check && npm run build
```

#### Not gated by CI

Not gated by CI `web-next/` (`npm run lint` / `npm test` there if that tree is edited).

Clippy is not a CI job. Run it locally with `./scripts/lint-all.sh` (`rust-analyzer.check.command` is `clippy` in `.vscode/settings.json`).

### Releases and versions

The product follows [Semantic Versioning](https://semver.org/spec/v2.0.0.html) (`MAJOR.MINOR.PATCH`). Record user-visible changes in `CHANGELOG.md` ([Keep a Changelog](https://keepachangelog.com/en/1.1.0/)) under `[Unreleased]` until a tag ships. Every changelog bullet must start with an ISO date (`YYYY-MM-DD`); released version headings use `## [0.8.0] - 2026-08-24`.

Three version numbers are easy to mix up:

| What            | Example             | Meaning                                                                                |
|-----------------|---------------------|----------------------------------------------------------------------------------------|
| Product version | `0.7.3`             | Desktop app + vault image. Git tag is `v0.7.3`.                                        |
| Docker Hub tag  | `0.7.3` (no `v`)    | `bitrealm/message-vault:0.7.3`. Also `0.7`, `latest`, and `sha-…`.                     |
| JSONL schema    | `schema_version: 3` | Shared chat file format. Independent of the product version. Do not invent v2 readers. |

**Product version files** (keep these in lockstep; current value is `0.7.3`):

- `src-tauri/Cargo.toml` — bump this before tagging (this is the one CI docs call out)
- `src-tauri/tauri.conf.json` — installer version
- `web/package.json` — Vite SPA
- `crates/vault/server/Cargo.toml` — vault server crate

Leave most other `Cargo.toml` files at `0.1.0`. Do not bump `crates/message-vault-io-gui/` (`0.6.0`) or `web-next/` (`0.3.0`) for a product release.

**Ship a release**

1. Merge the work to `main`.
2. Move `[Unreleased]` notes in `CHANGELOG.md` under the new version heading.
3. Set the four product version files to the new number (for example `0.8.0`).
4. Push a git tag `v0.8.0` on that commit. Pushing the tag is what ships. Push/PR to `main` does not.

`.github/workflows/ci.yml` then: runs fmt/test, pushes `bitrealm/message-vault`, builds Tauri installers (Linux `.deb` + AppImage, Windows `.msi`, macOS `.dmg`), and creates a GitHub Release named `Message Vault v0.8.0`.

**Build a release-shaped binary locally (does not publish)**

```bash
cargo tauri build                          # desktop installers under src-tauri/target/release/bundle/
docker compose -f docker/compose.release.yml up --build   # vault image from this checkout
cargo build --workspace --release          # workspace crates only; not the Tauri installer
```

Do not create or push tags unless asked.
