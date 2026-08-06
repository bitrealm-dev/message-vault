# Merge message-vault-rs into message-vault-io — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Merge the `message-vault-rs` repository into `message-vault-io` as a structured monorepo, preserving full git history and fixing all path dependencies.

**Architecture:** `git merge --allow-unrelated-histories` brings in `-rs` history. Then directory restructuring moves crates into namespaced `crates/libs/`, `crates/vault/`, `crates/cli/`. Path deps, Dockerfiles, docs, and CI config are reconciled. No new code — this is a pure restructuring.

**Tech Stack:** Rust workspace (Cargo), Docker (compose), Vite (web UI), git

## Global Constraints

- Full git history of both repos must be preserved
- DockerHub push must continue working post-merge (`Dockerfile.release` + `compose-release.yml`)
- `cargo build --workspace` must succeed after every task
- No version bumps — existing versions carry over
- Repo renamed from `message-vault-io` to `message-vault` (GitHub rename, not local directory)

---

### Task 1: Prepare branches and add -rs as a remote

**Files:**
- No file changes — git operations only

- [ ] **Step 1: Create a merge branch in -io**

```bash
cd ~/repo/message-vault-io
git checkout -b merge/message-vault-rs
```

- [ ] **Step 2: Add message-vault-rs as a remote**

```bash
git remote add message-vault-rs ~/repo/message-vault-rs
git fetch message-vault-rs --tags
```

- [ ] **Step 3: Verify both histories are visible**

```bash
git log --oneline message-vault-rs/main -5
git log --oneline main -5
```

Expected: both histories print without errors.

- [ ] **Step 4: Commit (nothing to commit, just verification)**

No commit needed — this is setup.

---

### Task 2: Merge -rs history into -io

**Files:**
- No file changes — merge operation

- [ ] **Step 1: Merge with unrelated histories**

```bash
cd ~/repo/message-vault-io
git merge message-vault-rs/main \
  --allow-unrelated-histories \
  -m "feat: merge message-vault-rs repository

Preserves full commit history from both repos via
--allow-unrelated-histories.

Co-Authored-By: Claude <noreply@anthropic.com>"
```

Expected: merge succeeds. Conflicts are likely on files that exist in both repos (`.gitignore`, `README.md`, `CLAUDE.md`, `Cargo.toml`, `docs/`, `scripts/`, `.github/`).

- [ ] **Step 2: Resolve conflicts on repo-root metadata files**

For each conflicted file, keep the `-io` version and note the `-rs` additions to reconcile later:

```bash
# Accept -io versions for now; we'll reconcile in later tasks
git checkout --ours .gitignore README.md CLAUDE.md
git add .gitignore README.md CLAUDE.md
```

- [ ] **Step 3: Resolve conflicts on directory overlaps**

```bash
# For docs/, scripts/, .github/ — keep -io versions, reconcile later
git checkout --ours docs/ scripts/ .github/ .cargo/ data/
git add docs/ scripts/ .github/ .cargo/ data/
```

- [ ] **Step 4: Handle Cargo.toml conflict**

```bash
# Keep -io workspace Cargo.toml; we'll add -rs members in Task 4
git checkout --ours Cargo.toml
git add Cargo.toml
```

- [ ] **Step 5: Complete the merge**

```bash
git commit -m "feat: merge message-vault-rs repository

Preserves full commit history from both repos via
--allow-unrelated-histories.

Conflicts resolved by keeping -io versions of overlapping files.
-rs content will be reconciled in subsequent commits.

Co-Authored-By: Claude <noreply@anthropic.com>"
```

- [ ] **Step 6: Verify merge**

```bash
git log --oneline -10
ls -la ~/repo/message-vault-io/
```

Expected: `-rs` files appear alongside `-io` files at repo root (e.g., `compose-dev.yml`, `Dockerfile.dev`, `static/`, `demo/`, `fixtures/`, `schema/`, `.env`).

---

### Task 3: Create new crate directory structure

**Files:**
- Create: `crates/libs/` (move from `crates/message/`)
- Create: `crates/vault/` (new)
- Create: `crates/cli/` (move `vault-push`, `vault-pull` here)

- [ ] **Step 1: Move shared libraries from crates/message/ to crates/libs/**

```bash
cd ~/repo/message-vault-io

# Create the target directory
mkdir -p crates/libs

# Move each library crate
for crate in ir contacts phone csv ir-format mail sbr obfuscate media go-sms-mms reexport; do
  git mv crates/message/$crate crates/libs/$crate
done
```

- [ ] **Step 2: Remove the now-empty crates/message/ directory**

```bash
rmdir crates/message
```

- [ ] **Step 3: Move vault-push and vault-pull to crates/cli/**

```bash
mkdir -p crates/cli
git mv crates/vault-push crates/cli/vault-push
git mv crates/vault-pull crates/cli/vault-pull
```

- [ ] **Step 4: Move message-vault-io-core to crates/core/**

```bash
git mv crates/message-vault-io-core crates/core/message-vault-io-core
```

- [ ] **Step 5: Create vault server crate directory**

```bash
mkdir -p crates/vault/server
```

The `src/` directory and root-level `Cargo.toml` `[package]` section from `-rs` will be moved here in Task 5.

- [ ] **Step 6: Move demo-seed to crates/vault/**

```bash
mkdir -p crates/vault
git mv crates/demo-seed crates/vault/demo-seed
```

- [ ] **Step 7: Commit the directory restructuring**

```bash
git add -A
git commit -m "refactor: restructure crates into namespaced directories

libs/    — shared libraries (was crates/message/)
core/    — message-vault-io-core
vault/   — server + demo-seed
cli/     — vault-push + vault-pull
exporters/ — unchanged

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

### Task 4: Fix all internal Cargo.toml path dependencies

**Files:**
- Modify: `Cargo.toml` (workspace root)
- Modify: `crates/libs/*/Cargo.toml` (any that reference `../message/` paths)
- Modify: `crates/core/message-vault-io-core/Cargo.toml`
- Modify: `crates/exporters/*/Cargo.toml`
- Modify: `crates/cli/vault-push/Cargo.toml`
- Modify: `crates/cli/vault-pull/Cargo.toml`
- Modify: `src-tauri/Cargo.toml`

- [ ] **Step 1: Update workspace members in root Cargo.toml**

Read the current `Cargo.toml` and update the `[workspace] members` array.

Old members referencing moved crates:
```toml
"crates/message/phone",
"crates/message/csv",
"crates/message/ir",
"crates/message/ir-format",
"crates/message/reexport",
"crates/message/mail",
"crates/message/sbr",
"crates/message/contacts",
"crates/message/obfuscate",
"crates/message/media",
"crates/message/go-sms-mms",
"crates/message-vault-io-core",
"crates/vault-push",
"crates/vault-pull",
```

New members:
```toml
"crates/libs/phone",
"crates/libs/csv",
"crates/libs/ir",
"crates/libs/ir-format",
"crates/libs/reexport",
"crates/libs/mail",
"crates/libs/sbr",
"crates/libs/contacts",
"crates/libs/obfuscate",
"crates/libs/media",
"crates/libs/go-sms-mms",
"crates/core/message-vault-io-core",
"crates/cli/vault-push",
"crates/cli/vault-pull",
```

Also add new members from `-rs`:
```toml
"crates/vault/server",
"crates/vault/demo-seed",
```

The `[workspace.dependencies]` section may also exist in `-io` `Cargo.toml` — if so, update any path values there too.

- [ ] **Step 2: Find and fix all internal path deps pointing to old locations**

```bash
cd ~/repo/message-vault-io
grep -rn 'path\s*=\s*"' crates/ src-tauri/ --include='Cargo.toml' | grep -v target | grep -v node_modules
```

For each match pointing to a moved crate, update the path. Expected patterns:

| Old path | New path |
|----------|----------|
| `../message/ir` | `../libs/ir` |
| `../message/contacts` | `../libs/contacts` |
| `../message/phone` | `../libs/phone` |
| `../message/csv` | `../libs/csv` |
| `../message/ir-format` | `../libs/ir-format` |
| `../message/reexport` | `../libs/reexport` |
| `../message/mail` | `../libs/mail` |
| `../message/sbr` | `../libs/sbr` |
| `../message/obfuscate` | `../libs/obfuscate` |
| `../message/media` | `../libs/media` |
| `../message/go-sms-mms` | `../libs/go-sms-mms` |
| `../message-vault-io-core` | `../core/message-vault-io-core` |
| `../vault-push` | `../cli/vault-push` |
| `../vault-pull` | `../cli/vault-pull` |

Fix each one with `Edit` or by rewriting the file.

- [ ] **Step 3: Run cargo check to verify no broken paths**

```bash
cargo check --workspace 2>&1
```

Expected: compilation succeeds (or fails only on missing `crates/vault/server` — that's Task 5). Fix any "can't find crate" errors.

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "fix: update all Cargo.toml path deps for new crate layout

Updated workspace members and internal path = \"...\" references
across all crates to match the new namespaced structure.

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

### Task 5: Integrate the vault server crate

**Files:**
- Create: `crates/vault/server/Cargo.toml`
- Move: `src/` → `crates/vault/server/src/`
- Modify: `Cargo.toml` (workspace root — add workspace dependencies if needed)
- Modify: `crates/vault/demo-seed/Cargo.toml` (fix path deps)

- [ ] **Step 1: Move the vault server source into place**

```bash
cd ~/repo/message-vault-io
git mv src/ crates/vault/server/src/
```

Note: `src/` was the `-rs` main binary source. If `-io` also has a top-level `src/`, there may be a conflict from the merge. Check:

```bash
ls -la src/ 2>&1
```

If `src/` is from `-io` (unlikely since it's a workspace with no root package), leave it. If from `-rs`, the `git mv` above is correct.

- [ ] **Step 2: Create crates/vault/server/Cargo.toml**

Extract the `[package]` section from the `-rs` root `Cargo.toml`. The root `Cargo.toml` from `-rs` has both `[workspace]` and `[package]` — we only want `[package]` for the server crate.

Read the current root `Cargo.toml` to understand which sections belong to the workspace and which to the package, then write `crates/vault/server/Cargo.toml`:

```toml
[package]
name = "message-vault-server"
version = "0.3.0"
edition = "2024"

[dependencies]
anyhow = "1.0.103"
argon2 = "0.5.3"
axum = { version = "0.8.9", features = ["multipart"] }
chrono = { version = "0.4.44", default-features = false, features = ["clock", "std"] }
clap = { version = "4.6.1", features = ["derive"] }
contacts = { path = "../libs/contacts" }
demo-seed = { path = "../demo-seed" }
futures-util = { version = "0.3.31", default-features = false }
jsonwebtoken = "9.3.1"
message-ir = { path = "../libs/ir" }
phone = { path = "../libs/phone" }
rand = "0.8.5"
reqwest = { version = "0.12.25", default-features = false, features = ["blocking", "rustls-tls", "json"] }
rusqlite = { version = "0.40.0", features = ["bundled"] }
serde = { version = "1.0.228", features = ["derive"] }
serde_json = "1.0.150"
sha2 = "0.11.0"
tempfile = "3.27.0"
tokio = { version = "1.53.0", features = ["macros", "rt-multi-thread", "net", "signal", "fs", "io-util"] }
toml = "1.1.2"
tower-http = { version = "0.7.0", features = ["limit", "cors", "fs"] }
uuid = { version = "1.18.1", features = ["v4"] }
```

Key changes from the old `-rs` root `Cargo.toml`:
- `contacts`, `message-ir`, `phone`: path deps changed from `../message-vault-io/crates/message/...` to `../libs/...`
- `demo-seed`: path changed from `crates/demo-seed` to `../demo-seed`
- Package name changed from `message-vault-rs` to `message-vault-server`

- [ ] **Step 3: Clean up the root Cargo.toml**

Remove any `[package]` section from the root `Cargo.toml` (it should only have `[workspace]`). If `-rs` brought over workspace-level `[dependencies]`, remove those too — the root is workspace-only.

- [ ] **Step 4: Fix demo-seed Cargo.toml path deps**

Read `crates/vault/demo-seed/Cargo.toml` and update any path dependencies that reference old locations (e.g., `../message/ir` → `../../libs/ir`).

- [ ] **Step 5: Run cargo check**

```bash
cargo check --workspace 2>&1
```

Expected: all crates compile. Fix any remaining path errors.

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "feat: integrate vault server crate into workspace

- Moved -rs src/ → crates/vault/server/src/
- Created crates/vault/server/Cargo.toml with corrected path deps
- Renamed package message-vault-rs → message-vault-server
- Cleaned root Cargo.toml to workspace-only

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

### Task 6: Fix Dockerfiles and compose files

**Files:**
- Modify: `Dockerfile.dev`
- Modify: `Dockerfile.release`
- Modify: `compose-dev.yml`
- Modify: `compose-release.yml`

- [ ] **Step 1: Read current Dockerfiles**

```bash
cat ~/repo/message-vault-io/Dockerfile.dev
cat ~/repo/message-vault-io/Dockerfile.release
```

Look for:
- `COPY` or `ADD` instructions that reference `../message-vault-io`
- `WORKDIR` or path references assuming sibling-repo layout
- Build context paths

- [ ] **Step 2: Update Dockerfile.dev**

Remove any `../message-vault-io` references. The build context is now the repo root. Update `COPY` instructions that needed shared crates — they're now all under `crates/libs/`. Example changes:

If the old Dockerfile had:
```dockerfile
COPY ../message-vault-io/crates/message/ir /app/crates/message/ir
```

It becomes:
```dockerfile
COPY crates/libs/ir /app/crates/libs/ir
```

Also update the binary path if it referenced `message-vault-rs`:
```dockerfile
# Old
RUN cargo build --release -p message-vault-rs
# New
RUN cargo build --release -p message-vault-server
```

- [ ] **Step 3: Update Dockerfile.release**

Same changes as Dockerfile.dev. Pay special attention to multi-stage builds that may copy between stages.

- [ ] **Step 4: Update compose-dev.yml and compose-release.yml**

Check:
- `build.context` — should be `.` (repo root), not `../message-vault-io`
- Service names — may reference old paths
- Volume mounts — may reference `../message-vault-io`

```bash
grep -n 'message-vault-io\|\.\./' compose-dev.yml compose-release.yml
```

Fix any references.

- [ ] **Step 5: Verify Docker build (dev)**

```bash
docker build -f Dockerfile.dev -t message-vault-server:dev .
```

Expected: build succeeds. If it fails on COPY paths, fix and retry.

- [ ] **Step 6: Commit**

```bash
git add Dockerfile.dev Dockerfile.release compose-dev.yml compose-release.yml
git commit -m "fix: update Dockerfiles for monorepo layout

Removed ../message-vault-io references. Build context is now repo root.
Updated crate paths and binary names for new structure.

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

### Task 7: Reconcile docs, scripts, config, and CI

**Files:**
- Merge: `docs/`
- Merge: `scripts/`
- Merge: `.github/`
- Merge: `.cargo/`
- Modify: `README.md`
- Modify: `CLAUDE.md`
- Modify: `.gitignore`
- Modify: `.dockerignore`

- [ ] **Step 1: Merge docs/ directories**

Check what `-rs` has in `docs/` that `-io` doesn't:

```bash
diff -rq ~/repo/message-vault-io/docs/ docs/ 2>&1 | grep "Only in" | head -20
```

Copy any `-rs`-only doc files into the `-io` `docs/` tree. Since both repos feed the same Starlight site, there may be content overlap — prefer the `-io` version when in doubt.

- [ ] **Step 2: Merge scripts/**

```bash
# List -rs scripts not in -io
ls ~/repo/message-vault-io/scripts/
ls scripts/  # the -rs scripts brought over by merge
```

Copy `-rs` scripts (docker entrypoints, setup-demo.sh, smoke tests) into `scripts/`. If a script exists in both, check if they differ:

```bash
diff scripts/setup-demo.sh ~/repo/message-vault-io/scripts/setup-demo.sh 2>&1
```

Prefer the `-rs` version for scripts that came from `-rs`.

- [ ] **Step 3: Merge GitHub Actions workflows**

```bash
ls .github/workflows/ 2>&1
ls ~/repo/message-vault-io/.github/workflows/ 2>&1
```

Copy `-rs` workflow files into `.github/workflows/`. Update any references to `message-vault-rs` → `message-vault-server` and fix paths.

- [ ] **Step 4: Update README.md**

The `-io` README should now document the full project. Merge key sections from the `-rs` README into the `-io` README:
- Quick start for the vault server (Docker + native)
- Repository layout section showing new structure
- Import instructions linking to vault-push

- [ ] **Step 5: Update CLAUDE.md**

Combine the two CLAUDE.md files. The `-io` version is the base; add `-rs`-specific build instructions, Docker workflows, and the vault server architecture.

- [ ] **Step 6: Update .gitignore and .dockerignore**

Merge any `-rs`-specific entries:

```bash
diff .gitignore ~/repo/message-vault-io/.gitignore 2>&1
diff .dockerignore ~/repo/message-vault-io/.dockerignore 2>&1
```

- [ ] **Step 7: Commit**

```bash
git add -A
git commit -m "docs: reconcile docs, scripts, CI, and config from -rs merge

Unified README, CLAUDE.md, GitHub Actions, and scripts.
Single docs/ tree for the Starlight site.

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

### Task 8: Drop stale -rs web/ directory

**Files:**
- Delete: `web/` (the `-rs` web directory with 3 stale files)

- [ ] **Step 1: Verify -io web/ is intact**

```bash
ls -la ~/repo/message-vault-io/web/
```

Expected: full Vite project with `package.json`, `vite.config.ts`, `src/`, `dist/`.

- [ ] **Step 2: Verify -rs web/ is just the 3 stale files**

```bash
find web/ -type f
```

Expected output:
```
web/src/components/HandleTypeBadge.tsx
web/src/lib/handleKind.test.ts
web/src/app/api/contacts/handles-body.ts
```

- [ ] **Step 3: Delete the stale -rs web/ directory**

```bash
rm -rf web/
```

Note: `git rm -r web/` only if it's tracked. Check with `git ls-files web/` first.

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "chore: remove stale -rs web/ directory

The -rs Next.js web code was already replaced by the shared
Vite UI in -io web/. These 3 leftover files are no longer needed.

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

### Task 9: Full build verification

**Files:**
- No file changes — verification only

- [ ] **Step 1: cargo build --workspace**

```bash
cd ~/repo/message-vault-io
cargo build --workspace 2>&1
```

Expected: all crates compile without errors.

- [ ] **Step 2: cargo test --workspace**

```bash
cargo test --workspace 2>&1
```

Expected: all tests pass.

- [ ] **Step 3: Docker dev build**

```bash
docker build -f Dockerfile.dev -t message-vault-server:dev .
```

Expected: image builds successfully.

- [ ] **Step 4: Docker release build**

```bash
docker build -f Dockerfile.release -t message-vault-server:latest .
```

Expected: slim image builds successfully.

- [ ] **Step 5: Verify web/ builds**

```bash
cd web && npm ci && npm run build
```

Expected: Vite build succeeds, `dist/` populated.

- [ ] **Step 6: Verify Tauri builds (optional, may need GUI libs)**

```bash
cargo build --release -p message-vault-io-gui 2>&1
```

This may require system GUI libraries. Skip if not available in this environment.

- [ ] **Step 7: Commit any fixes**

If any step above required fixes, commit them:

```bash
git add -A
git commit -m "fix: build verification fixes

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

### Task 10: Final cleanup and rename

**Files:**
- Modify: various — final references to old names
- Modify: `.github/workflows/` — ensure CI works

- [ ] **Step 1: Check for remaining references to old names**

```bash
cd ~/repo/message-vault-io
grep -rn 'message-vault-rs' --include='*.md' --include='*.toml' --include='*.yml' --include='*.yaml' --include='*.json' --include='*.rs' --include='*.ts' --include='*.tsx' . 2>/dev/null | grep -v target | grep -v node_modules | grep -v .git | grep -v docs/superpowers
```

Update any remaining references that should now say `message-vault-server` or just `message-vault`.

- [ ] **Step 2: Check for remaining ../message-vault-io path references**

```bash
grep -rn 'message-vault-io' --include='*.toml' --include='*.yml' --include='*.yaml' --include='*.md' --include='*.json' . 2>/dev/null | grep -v target | grep -v node_modules | grep -v .git | grep -v docs/superpowers
```

Fix any that should be updated.

- [ ] **Step 3: Update CI workflow to single pipeline**

Read `.github/workflows/` and ensure there's one workflow that:
- Runs `cargo build --workspace` and `cargo test --workspace`
- Builds Docker images from `Dockerfile.release`
- Pushes to DockerHub on tag
- Attaches desktop app and CLI binaries to GitHub Release on tag

If the `-rs` and `-io` workflows are still separate files, merge them.

- [ ] **Step 4: Final commit on merge branch**

```bash
git add -A
git commit -m "chore: final cleanup — old name references, unified CI

Co-Authored-By: Claude <noreply@anthropic.com>"
```

- [ ] **Step 5: Push merge branch**

```bash
git push origin merge/message-vault-rs
```

- [ ] **Step 6: (Manual, post-merge) Rename GitHub repo**

This step cannot be automated — it's a GitHub settings action:
1. Go to `github.com/bitrealm-dev/message-vault-io` → Settings
2. Rename to `message-vault`
3. Update local remotes: `git remote set-url origin git@github.com:bitrealm-dev/message-vault.git`

---

### Post-Merge Verification Checklist

After the merge branch is merged to main:

- [ ] `cargo build --workspace` passes on main
- [ ] `cargo test --workspace` passes on main
- [ ] DockerHub push works from CI on tag
- [ ] Desktop app release attaches to GitHub Release
- [ ] Docs site builds from unified `docs/` tree
- [ ] Local dev workflow documented in README works end-to-end
