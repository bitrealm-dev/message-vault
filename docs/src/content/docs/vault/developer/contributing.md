---
title: Contributing
description: Set up a development environment, run tests, and open pull requests for Message Vault.
---

## Contributing to the Message Vault

Thank you for considering contributing!
Every contribution - big or small - makes this project better.

## Reporting Bugs or Requesting Features

- **Found a bug?** Open a [bug report](https://github.com/bitrealm-io/message-vault/issues/new?template=bug_report.md) on GitHub with steps to reproduce the issue.
- **Have a feature idea?** Submit a [feature request](https://github.com/bitrealm-io/message-vault/issues/new?template=feature_request.md) on GitHub describing what you'd like to see.

When opening an issue, use the provided issue form to ensure that you provide all the necessary details. These details are important for maintainers to understand and reproduce the issue.

## Environment Setup

Setting up a development environment depends upon your OS.

Here are instructions for Ubuntu Linux:

### Install Apt Packages

```bash title="Install Ubuntu packages"
sudo apt update

# Rust native crates need a C compiler. libssl-dev is for OpenSSL
sudo apt install -y curl git build-essential pkg-config libssl-dev

# Needed for Rust tests `cargo test --workspace`
sudo apt install -y libfontconfig1-dev libxkbcommon-dev

# Desktop app (cargo tauri dev / cargo tauri build) — WebKit/GTK
sudo apt install -y \
  libwebkit2gtk-4.1-dev libgtk-3-dev \
  libappindicator3-dev librsvg2-dev patchelf \
  libjavascriptcoregtk-4.1-dev libsoup-3.0-dev

# Media conversion / compression
sudo apt install -y ffmpeg
```

### Install Rust Packages

**Note** The version available from `apt` is typically to old.

Required minimum version: 1.85

Install rust via `rustup`.

```bash title="Install Rust with rustup"
# Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

```bash title="Install tauri-cli"
# Tauri - Native Window app which bundles web gui
cargo install tauri-cli --version "^2"
```

### Install Node

**Note** The version available from `apt` is typically to old.

Required minimum version: 22

```bash title="Install Node.js 22"
curl -fsSL https://raw.githubusercontent.com/nvm-sh/nvm/v0.40.3/install.sh | bash
source ~/.bashrc   # or open a new terminal
nvm install 22
nvm use 22
node -v            # should print v22.x
npm -v
```

### Additional Packages

`wtsexporter` is used to extract `WhatsApp` messages.

```bash title="Install pipx helpers"
sudo apt install -y pipx
pipx ensurepath

# reopen the shell, or: source ~/.bashrc
pipx install 'whatsapp-chat-exporter[android_backup,crypt15]'

# Used to connect and view the sqlite db
pipx install sqlite-web
```

### Fork and Clone Repo

Fork and clone the [Message Vault repo](https://github.com/bitrealm-io/message-vault) `https://github.com/bitrealm-io/message-vault`. If you've never forked a repo before, see [this guide](https://docs.github.com/en/pull-requests/how-tos/work-with-forks/fork-a-repo).

*Note:* You only need to copy the default "main" branch when forking.

### Build and Run

Two processes have to run at the same time: the vault, and a UI that talks to it. The vault is the HTTP API and the SQLite database. It must be running before anyone can sign in.

Work from the repository root in both terminals. The first compile of the server takes several minutes.

**Terminal 1 — start the vault**

`--reset-demo` deletes `data/` and loads a sample inbox. Use it on the first run, or when a fresh sample inbox is wanted.

```bash title="Start the vault"
./scripts/run-vault-dev.sh --reset-demo
```

Leave this terminal running. The API listens at **http://127.0.0.1:8080**.

To browse tables while developing, add `--sqlweb` (needs `sqlite-web` from the previous step). That UI is **http://127.0.0.1:8081**.

**Terminal 2 — open the website**

Install frontend packages once, then start the Vite UI. Vite is the local web server for `web/`.

```bash title="Start the website"
cd web && npm ci && npm run dev
```

Open **http://localhost:5173**. Sign in as username `demo` with an empty password. That account is read-only. Create a separate account to test import or other writes.

Later sessions, skip `npm ci` unless `web/package-lock.json` changed. Skip `--reset-demo` unless the sample message data should be rebuilt.

**Desktop app**

The desktop app is a native window (Tauri) around the same `web/` UI. Use it when changing `src-tauri/` or testing import from a backup. Do not run `npm run dev` in another terminal at the same time; Tauri starts Vite itself.

```bash title="Start the desktop app"
cd web && npm ci && cd ..
cargo tauri dev
```

When the window opens, point it at **http://127.0.0.1:8080**. The first compile of the desktop app also takes several minutes.

#### Stopping and restarting

Ctrl+C in terminal 1 stops the vault (and the SQLite UI if it was started). Ctrl+C in terminal 2 stops the website or the desktop app.

After edits under `crates/vault/server/`, restart terminal 1. After edits under `web/` or `src-tauri/`, the UI usually reloads. Restart `cargo tauri dev` if it does not.

## Making Code Changes

Open a GitHub issue before starting the work, so the later pull request can link to it. Use the bug report or feature request form. Do not wait for a reply before coding. If the issue has no reply after 5 business days, comment on that same issue.

### Branch

Start from the latest `main`. Do not commit on `main`. Name the branch with a prefix:

- `feat/short-name` — new behavior
- `fix/short-name` — a bug
- `docs/short-name` — documentation only

Keep the branch current with `main` while working (merge or rebase). One pull request should do one job.

### Commits

Each commit should be one idea. Do not mix a bug fix with a rename, or a feature with formatting of unrelated files.

Prefer `feat:`, `fix:`, or `docs:` at the start of the subject when it fits. Other prefixes are optional. The subject should say what changed. Add a short body when the reason is not obvious. Mention the issue (`Ref: #123`).

### Example

After the fork is cloned, from the repository root:

```bash title="Create a feature branch"
git remote add upstream https://github.com/bitrealm-io/message-vault.git
git fetch upstream
git checkout -b feat/short-name upstream/main
git commit -m "feat: add support for x

Ref: #123"
git push -u origin feat/short-name
```

Add `upstream` once. For later branches: `git fetch upstream`, then `git checkout -b … upstream/main`.

After the vault compiles, [Vault Design](/vault/developer/vault-design/) maps the tree, binaries, and processes. [Message Transfer](/vault/developer/message-transfer/) covers exporter → JSONL → import.

### Preview the guidebook

Edit guidebook pages under `docs/src/content/docs/vault/`. Those pages show up at paths like `/vault/user/` and `/vault/developer/`. The home page at `/` comes from `docs/src/pages/index.astro`. The published site is **https://bitrealm.io/**.

To preview locally:

```bash title="Preview the guidebook"
cd docs
npm ci
npm run dev
```

Open **http://localhost:4321/** for the home page, or **http://localhost:4321/vault/developer/** for Developer docs. Before opening a pull request, `./scripts/check-pr.sh` already checks and builds `docs/`.

## Opening a PR

Run the checks, then open a pull request against `main`. Do this after **Making Code Changes**. The first compile and the first `npm ci` each take several minutes.

### Before it is ready

From the repository root:

```bash title="Run pull-request checks"
./scripts/check-pr.sh
```

That script applies rustfmt to the workspace and to `src-tauri/` (it rewrites files). Then it builds and tests the workspace, lints and tests `web/`, and checks and builds `docs/`. It does not auto-format `web/`. It stops on the first failure. It runs `npm ci` in `web/` or `docs/` only when that tree has no `node_modules` yet. If rustfmt changed files, commit those changes before opening the pull request.

While iterating on one crate, `cargo test -p go-sms-pro-exporter` is enough. Exporter smoke tests use committed fixtures. Personal phone backups are not required.

### Keep the branch current

If `main` has moved, update the branch before asking for review:

```bash title="Update the branch from main"
git fetch upstream
git merge upstream/main
git push
```

Rebase is allowed. Merge is enough. Do not force-push unless the branch is only used by that one contributor.

### Open the pull request

A pull request asks to merge the branch into `main`. Open it against `main` on [bitrealm-io/message-vault](https://github.com/bitrealm-io/message-vault). Use the GitHub pull request form. GitHub fills in the default template. That default is enough for most changes. Feature and bug-fix templates also exist; they are not required.

Link the issue (`Ref: #123`). Write `Fixes #123` in the description if this change should close that issue.

Prefer `feat:`, `fix:`, or `docs:` at the start of the title when it fits.

From the repository root, this also works:

```bash title="Open a pull request"
gh pr create --base main --title "feat: add support for x" --body "Ref: #123"
```

### After it is open

GitHub runs checks. Fix failing checks. Reply to review comments on the same pull request.

## Contribution rules

1. **Keep changes focused.** Prefer small PRs that do one job.
2. **Match existing style.** Follow nearby crates; avoid drive-by renames.
3. **Verify before opening a PR.** Use [Opening a PR](#opening-a-pr).
4. **No secrets or personal data.** Do not commit passwords, vault keys, certificates, credential `.env` files, or real message backups. Use fixtures under `crates/*/tests/fixtures/`.
5. **Respect licenses.** See [License](#license).
6. **Document CLI changes** on the matching page under `docs/src/content/docs/vault/developer/reference/cli/`.
7. **Put the product map in Developer docs.** Tree, C4, and message transfer live on [Vault Design](/vault/developer/vault-design/) and [Message Transfer](/vault/developer/message-transfer/). Long format contracts such as [message-ir](https://github.com/bitrealm-io/message-vault/blob/main/docs/maintainers/architecture/message-ir.md) stay under [`docs/maintainers/`](https://github.com/bitrealm-io/message-vault/blob/main/docs/maintainers/README.md).
8. **Use a pull request template.** Default plus feature and bug-fix forms live under [`.github/`](https://github.com/bitrealm-io/message-vault/tree/main/.github).

## License

Distributed under the Fair Core License. See [LICENSE.md](https://github.com/bitrealm-io/message-vault/blob/main/LICENSE.md) for the full text.
