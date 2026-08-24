# Retire Run from source Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Contributing **Build and Run** is the only published page for compile-and-run from a git checkout; the Run from source page is gone.

**Architecture:** Keep first-run copy on Contributing (`--reset-demo`, Vite, `cargo tauri dev`). Add the four leftover commands from Run from source into that same section. Rewrite every live href that pointed at `/vault/developer/run-from-source/` so it points at Contributing `#build-and-run` (glossary → Developer index). Then delete the page and its sidebar slug. No HTTP redirect.

**Tech Stack:** Astro Starlight (`docs/`), Markdown under `docs/src/content/docs/vault/`, sidebar in `docs/astro.config.mjs`.

## Global Constraints

- Voice: short sentences, concrete commands, no “we” / “us” / “our”.
- Starlight code fences use `title=""`. Do not use GitHub `> [!TIP]` alerts.
- No HTTP redirect from `/vault/developer/run-from-source/`.
- Do not add a WSL section or a second Ubuntu package list.
- Do not name the `message-vault-io-tauri` Cargo package.
- Do not edit `AGENTS.md`, `README.md`, `docs/src/pages/index.astro`, FAQ, or files under `docs/superpowers/specs/` except the existing spec.
- Do not add a Contributing heading above **Making Code Changes**. New material stays under **Build and Run**.
- `cargo tauri build` is an alternate, not the day-to-day command.

**Spec:** `docs/superpowers/specs/2026-08-22-retire-run-from-source-design.md`

---

## File map

| File | Role |
|------|------|
| `docs/src/content/docs/vault/developer/contributing.md` | Gains vault flags, optional `build-static.sh`, and `cargo tauri build` |
| `docs/src/content/docs/vault/developer/index.md` | Drops Run from source bullet; description no longer names that page |
| `docs/src/content/docs/vault/developer/docker-compose.md` | Three hrefs → Contributing `#build-and-run` |
| `docs/src/content/docs/vault/user/get-started/try-the-vault.md` | “Build from source instead” → Contributing |
| `docs/src/content/docs/vault/user/get-started/install-the-desktop-app.md` | “Build from source” → one Contributing link |
| `docs/src/content/docs/vault/user/glossary.md` | Intro link → `/vault/developer/` |
| `docs/astro.config.mjs` | Remove `vault/developer/run-from-source` from `developerItems` |
| `docs/src/content/docs/vault/developer/run-from-source.md` | Delete |

Do not create new files.

---

### Task 1: Add leftover commands to Contributing Build and Run

**Files:**
- Modify: `docs/src/content/docs/vault/developer/contributing.md` (section `### Build and Run`, after the later-sessions sentence, and after the `cargo tauri dev` block)

**Interfaces:**
- Consumes: existing **Build and Run** first-run copy (`--reset-demo`, Vite, `cargo tauri dev`)
- Produces: headings **Vault flags** and **Serve the website from the vault (optional)**; `cargo tauri build` paragraph before **Stopping and restarting**; Starlight id `#build-and-run` unchanged

- [ ] **Step 1: Insert vault flags and optional static build**

In `docs/src/content/docs/vault/developer/contributing.md`, find this exact block (later-sessions sentence, then **Desktop app**):

````markdown
Later sessions, skip `npm ci` unless `web/package-lock.json` changed. Skip `--reset-demo` unless the sample message data should be rebuilt.

**Desktop app**
````

Replace it with:

````markdown
Later sessions, skip `npm ci` unless `web/package-lock.json` changed. Skip `--reset-demo` unless the sample message data should be rebuilt.

### Vault flags

First run uses `--reset-demo`. Later sessions, start without flags so `data/` stays:

```bash title="Start the vault, keep data"
./scripts/run-vault-dev.sh
```

`--reset` wipes `data/` and starts empty (no sample inbox). Do not combine `--reset` and `--reset-demo`. `--sqlweb` still works with any of these.

### Serve the website from the vault (optional)

Vite is the usual UI. To have the vault itself serve the website at **http://127.0.0.1:8080**:

```bash title="Build the website into static/"
./scripts/build-static.sh
```

That copies `web/dist` into `static/`. Do not run the host vault and `docker compose -f docker/compose.release.yml` at the same time; both use port 8080.

**Desktop app**
````

Keep Terminal 1, `--sqlweb`, Terminal 2, and the demo login paragraphs unchanged.

- [ ] **Step 2: Add `cargo tauri build` as an alternate**

Find this exact block after the desktop `cargo tauri dev` fence:

````markdown
When the window opens, point it at **http://127.0.0.1:8080**. The first compile of the desktop app also takes several minutes.

#### Stopping and restarting
````

Replace it with:

````markdown
When the window opens, point it at **http://127.0.0.1:8080**. The first compile of the desktop app also takes several minutes.

For a release-shaped desktop binary (faster on real backups, or when packaging installers):

```bash title="Build a release-shaped desktop app"
cargo tauri build
```

Do not use `cargo tauri build` for day-to-day UI work. `cargo tauri dev` reloads. The build command does not.

#### Stopping and restarting
````

Do not add `message-vault-io-tauri`. Do not change **Making Code Changes**.

- [ ] **Step 3: Check the new copy is present and first-run copy is intact**

Run from the repository root:

```bash
rg -n "Start the vault, keep data|--reset wipes|build-static.sh|Build a release-shaped desktop app" docs/src/content/docs/vault/developer/contributing.md
rg -n "run-vault-dev.sh --reset-demo|npm run dev|cargo tauri dev" docs/src/content/docs/vault/developer/contributing.md
```

Expected: first command matches the new titles and `--reset` / `build-static.sh` sentences. Second command still matches `--reset-demo`, `npm run dev`, and `cargo tauri dev`.

- [ ] **Step 4: Commit**

```bash
git add docs/src/content/docs/vault/developer/contributing.md
git commit -m "$(cat <<'EOF'
docs: add vault flags to contributing build-and-run

Leftover run-from-source commands belong on Contributing so that
page is enough to compile and run from a checkout.
EOF
)"
```

---

### Task 2: Retarget live Run from source links

**Files:**
- Modify: `docs/src/content/docs/vault/developer/index.md`
- Modify: `docs/src/content/docs/vault/developer/docker-compose.md`
- Modify: `docs/src/content/docs/vault/user/get-started/try-the-vault.md`
- Modify: `docs/src/content/docs/vault/user/get-started/install-the-desktop-app.md`
- Modify: `docs/src/content/docs/vault/user/glossary.md`

**Interfaces:**
- Consumes: Contributing `#build-and-run` from Task 1
- Produces: no remaining hrefs to `/vault/developer/run-from-source/` in these five files

- [ ] **Step 1: Update the Developer index**

Replace the frontmatter description:

```yaml
description: Run Message Vault from source, then vault design, message transfer, CLI tools, the HTTP API, formats, and instance internals.
```

with:

```yaml
description: Set up a development environment, then vault design, message transfer, CLI tools, the HTTP API, formats, and instance internals.
```

Delete this bullet (leave the Architecture bullet and Operator Docker bullet as neighbors):

```markdown
- [Run from source](/vault/developer/run-from-source/) — clone, `cargo run`, `cargo tauri dev`
```

Leave the Contributing bullet as:

```markdown
- [Contributing](/vault/developer/contributing/) — environment setup, tests, pull requests
```

- [ ] **Step 2: Update Operator Docker (three places)**

Replace the intro sentence:

```markdown
Day-to-day work from a clone uses [`./scripts/run-vault-dev.sh`](https://github.com/bitrealm-io/message-vault/blob/main/scripts/run-vault-dev.sh) on the host — see [Run from source](/vault/developer/run-from-source/). This page is Docker: a checkout that should look like a shipped install, or the published Hub image without compiling.
```

with:

```markdown
Day-to-day work from a clone uses [`./scripts/run-vault-dev.sh`](https://github.com/bitrealm-io/message-vault/blob/main/scripts/run-vault-dev.sh) on the host — see [Contributing](/vault/developer/contributing/#build-and-run). This page is Docker: a checkout that should look like a shipped install, or the published Hub image without compiling.
```

Replace the in-body sentence:

```markdown
The vault process runs inside the image. The desktop app stays on the host. For local development without Docker, use [Run from source](/vault/developer/run-from-source/).
```

with:

```markdown
The vault process runs inside the image. The desktop app stays on the host. For local development without Docker, use [Contributing](/vault/developer/contributing/#build-and-run).
```

Replace the Related list item:

```markdown
- [Run from source](/vault/developer/run-from-source/)
```

with:

```markdown
- [Contributing](/vault/developer/contributing/#build-and-run)
```

Leave the HTTP API and Config and accounts Related items unchanged.

- [ ] **Step 3: Update Try the vault**

Replace:

```markdown
Compiling the vault and the desktop app: [Run from source](/vault/developer/run-from-source/). A release-shaped image from a git checkout: [Operator Docker](/vault/developer/docker-compose/).
```

with:

```markdown
Compiling the vault and the desktop app: [Contributing](/vault/developer/contributing/#build-and-run). A release-shaped image from a git checkout: [Operator Docker](/vault/developer/docker-compose/).
```

Keep the heading `## Build from source instead`.

- [ ] **Step 4: Update Install the desktop app**

Replace:

```markdown
Compiling the app and the vault from a git checkout: [Run from source](/vault/developer/run-from-source/). Linux system libraries and WSL notes are on [Contributing](/vault/developer/contributing/).
```

with:

```markdown
Compiling the app and the vault from a git checkout: [Contributing](/vault/developer/contributing/#build-and-run).
```

Keep the heading `## Build from source`.

- [ ] **Step 5: Update the glossary intro**

Replace:

```markdown
Short definitions of terms used in the User Guide. Command flags and vendor field names live under [Developer](/vault/developer/run-from-source/).
```

with:

```markdown
Short definitions of terms used in the User Guide. Command flags and vendor field names live under [Developer](/vault/developer/).
```

- [ ] **Step 6: Confirm these five files no longer cite the old slug**

```bash
rg -n "run-from-source" \
  docs/src/content/docs/vault/developer/index.md \
  docs/src/content/docs/vault/developer/docker-compose.md \
  docs/src/content/docs/vault/user/get-started/try-the-vault.md \
  docs/src/content/docs/vault/user/get-started/install-the-desktop-app.md \
  docs/src/content/docs/vault/user/glossary.md
```

Expected: no matches.

```bash
rg -n "contributing/#build-and-run" \
  docs/src/content/docs/vault/developer/docker-compose.md \
  docs/src/content/docs/vault/user/get-started/try-the-vault.md \
  docs/src/content/docs/vault/user/get-started/install-the-desktop-app.md
```

Expected: docker-compose.md three times; try-the-vault.md once; install-the-desktop-app.md once.

```bash
rg -n "\]\(/vault/developer/\)" docs/src/content/docs/vault/user/glossary.md
```

Expected: the intro line links to `/vault/developer/`.

- [ ] **Step 7: Commit**

```bash
git add \
  docs/src/content/docs/vault/developer/index.md \
  docs/src/content/docs/vault/developer/docker-compose.md \
  docs/src/content/docs/vault/user/get-started/try-the-vault.md \
  docs/src/content/docs/vault/user/get-started/install-the-desktop-app.md \
  docs/src/content/docs/vault/user/glossary.md
git commit -m "$(cat <<'EOF'
docs: point run-from-source links at contributing

Compile-and-run now lives on Contributing. These pages should not
send readers to a page that is about to be removed.
EOF
)"
```

---

### Task 3: Delete the Run from source page and sidebar entry

**Files:**
- Delete: `docs/src/content/docs/vault/developer/run-from-source.md`
- Modify: `docs/astro.config.mjs` (`developerItems`, the `vault/developer/run-from-source` string)

**Interfaces:**
- Consumes: Task 2 already removed hrefs to this slug from live guidebook pages
- Produces: no Starlight page at `/vault/developer/run-from-source/`; sidebar jumps from Architecture to Operator Docker

- [ ] **Step 1: Remove the sidebar slug**

In `docs/astro.config.mjs`, inside `developerItems`, find:

```javascript
  'vault/developer/run-from-source',
  'vault/developer/docker-compose',
```

Replace with:

```javascript
  'vault/developer/docker-compose',
```

Do not reorder Contributing or Architecture.

- [ ] **Step 2: Delete the page**

```bash
git rm docs/src/content/docs/vault/developer/run-from-source.md
```

- [ ] **Step 3: Confirm the slug is gone from live docs (not specs or dist)**

```bash
rg -n "run-from-source" docs/src docs/astro.config.mjs
```

Expected: no matches under `docs/src/content/docs/` or `docs/astro.config.mjs`. Matches under `docs/superpowers/` are allowed (this plan and the spec).

- [ ] **Step 4: Commit**

```bash
git add docs/astro.config.mjs
git commit -m "$(cat <<'EOF'
docs: remove run-from-source page

Contributing already covers clone and run. A second page for the
same path would keep two sources of truth.
EOF
)"
```

(`git rm` already staged the deleted file; `git add docs/astro.config.mjs` stages the sidebar edit. If `git status` still shows the deleted page as staged from `git rm`, include it in this commit — do not leave the delete uncommitted.)

---

### Task 4: Verify the docs site

**Files:**
- Test: `docs/` (`npm run check`, `npm run build`)
- Do not modify: `AGENTS.md`, `README.md`, `docs/src/pages/index.astro`, `docs/src/pages/faq.astro`

**Interfaces:**
- Consumes: Tasks 1–3 complete
- Produces: green Astro check/build; no `dist/vault/developer/run-from-source/`

- [ ] **Step 1: Grep live files for the old slug**

From the repository root:

```bash
rg -n "/vault/developer/run-from-source/" --glob '!docs/superpowers/**' --glob '!docs/dist/**'
```

Expected: no matches.

- [ ] **Step 2: Confirm out-of-scope files are unchanged**

```bash
git diff --name-only HEAD
git log --oneline origin/main..HEAD
```

Expected: commits only touch the files in the File map. `AGENTS.md`, `README.md`, `docs/src/pages/index.astro`, and `docs/src/pages/faq.astro` are not in the diff.

- [ ] **Step 3: Run Astro check and build**

```bash
cd docs
npm run check
npm run build
```

Expected: both succeed (exit 0). First run may take a minute.

- [ ] **Step 4: Confirm the built page is gone**

```bash
test ! -e dist/vault/developer/run-from-source
ls dist/vault/developer/contributing/index.html
ls dist/vault/developer/docker-compose/index.html
```

Expected: `test` succeeds (path absent). The two `ls` commands print those index.html paths.

- [ ] **Step 5: Commit only if Step 3 or 4 forced a fix**

If check/build failed, fix the docs, re-run Steps 1–4, then commit that fix:

```bash
git add -u docs/src docs/astro.config.mjs
git commit -m "$(cat <<'EOF'
docs: fix leftover run-from-source references after page removal
EOF
)"
```

If check/build already passed with no extra edits, do not create an empty commit.

---

## Self-review (against the spec)

| Spec requirement | Task |
|------------------|------|
| Fold no-flag vault command, `--reset`, `build-static.sh`, `cargo tauri build` into Build and Run | Task 1 |
| `cargo tauri build` marked as not day-to-day | Task 1 |
| First-run `--reset-demo` / Vite / `cargo tauri dev` kept | Task 1 |
| Delete `run-from-source.md` and sidebar slug | Task 3 |
| No redirect | Tasks 3–4 (page gone, no astro redirect config) |
| Developer index bullet + description | Task 2 |
| Operator Docker three links | Task 2 |
| Try the vault / Install the desktop app | Task 2 |
| Glossary → `/vault/developer/` | Task 2 |
| Install page drops extra WSL sentence | Task 2 Step 4 |
| `npm run check` / `npm run build`; no dist page; grep live files | Task 4 |
| Do not edit AGENTS.md, README, landing, FAQ | Task 4 Step 2 |
