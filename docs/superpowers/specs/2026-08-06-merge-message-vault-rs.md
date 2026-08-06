# Merge message-vault-rs into message-vault-io

## Motivation

Three pain points drive this merge:

1. **Path-dependency fragility** — `message-vault-rs` depends on shared crates
   (`message-ir`, `contacts`, `phone`) via `path =
   "../message-vault-io/crates/message/..."`. This breaks Docker builds
   (requiring `../message-vault-io` as build context) and forces the two repos
   to be sibling directories on disk. A recent commit (`522f59c`) was
   specifically "resolve Docker compile errors — path deps, build context, and
   missing crates."

2. **Coordination overhead** — two repos means two CI pipelines, two sets of
   docs that feed one website, two CLAUDE.md files, and changes that span both
   repos require two orchestrated PRs. This has been fragile and broken.

3. **UI duplication** — the Tauri desktop app UI (Vite, in `-io` `web/`) is
   shared with the vault server. Currently the Next.js code in `-rs` is already
   stale/deleted; the single source of truth is `-io` `web/`.

## Design

### Approach

**Structured monorepo.** Merge `-rs` into `-io` (the larger repo, already
owner of the shared crates), reorganize into namespaced directories, preserve
full git history.

### Target repository layout

```
message-vault/                    # renamed from message-vault-io
  Cargo.toml                      # single workspace, all members
  crates/
    libs/                         # was crates/message/
      ir/
      contacts/
      phone/
      csv/
      ir-format/
      mail/
      sbr/
      obfuscate/
      media/
      go-sms-mms/
      reexport/
    exporters/                    # unchanged: lib + CLI binary
      go-sms-pro-exporter/
      imazing-exporter/
      imessage-ir-exporter/
      openextract-exporter/
      sms-backup-plus-exporter/
      sms-backup-restore-exporter/
      whatsapp-exporter/
    core/                         # was message-vault-io-core
    vault/
      server/                     # was message-vault-rs root crate (src/)
      demo-seed/                  # was crates/demo-seed
    cli/
      vault-push/
      vault-pull/
  src-tauri/                      # desktop app (unchanged)
  web/                            # shared UI (Vite, from -io)
  config/                         # from -rs
  docs/                           # unified docs
  scripts/                        # merged scripts
  Dockerfile.dev                  # from -rs
  Dockerfile.release              # from -rs
  compose-dev.yml                 # from -rs
  compose-release.yml             # from -rs
  static/                         # from -rs
  schema/                         # from -rs
  fixtures/                       # from -rs
  data/                           # merged
  demo/                           # from -rs
```

### History preservation

`git merge --allow-unrelated-histories` from `-rs` into `-io`, preserving the
full commit DAG of both repos.

### Release model

One tag triggers one CI pipeline producing three artifact types:

| Artifact | Crate/binary | Destination |
|----------|-------------|-------------|
| Docker image | `crates/vault/server/` | DockerHub |
| Desktop app | `src-tauri/` | GitHub Release (zip/tgz per platform) |
| CLI binaries | `crates/cli/vault-push/`, `crates/cli/vault-pull/` | GitHub Release (standalone) |

Exporter CLIs are bundled inside the desktop app zip; they may also get
standalone releases later.

### Files moved from message-vault-rs

| Source | Destination | Notes |
|--------|-------------|-------|
| `src/` | `crates/vault/server/src/` | main vault binary |
| `Cargo.toml` (root package) | `crates/vault/server/Cargo.toml` | extract `[package]` section |
| `crates/demo-seed/` | `crates/vault/demo-seed/` | |
| `config/` | `config/` | |
| `docs/` | merge into `docs/` | |
| `scripts/` | merge into `scripts/` | |
| `Dockerfile.dev` | repo root | |
| `Dockerfile.release` | repo root | |
| `compose-dev.yml` | repo root | |
| `compose-release.yml` | repo root | |
| `static/` | repo root | |
| `schema/` | repo root | |
| `fixtures/` | repo root | |
| `data/` | merge into `data/` | |
| `demo/` | repo root | |
| `.cargo/` | merge into `.cargo/` | |
| `.env` | repo root | |
| `.dockerignore` | merge into `.dockerignore` | |
| `.github/` | merge into `.github/` | |
| `web/` | **dropped** | 3 stale files; real UI is `-io` `web/` |

### Post-merge dependency fixes

All `path = "../message-vault-io/..."` deps in the vault server's `Cargo.toml`
become normal workspace paths:

```toml
# Before (in -rs Cargo.toml)
message-ir = { path = "../message-vault-io/crates/message/ir", package = "message-ir" }
contacts   = { path = "../message-vault-io/crates/message/contacts", package = "contacts" }
phone      = { path = "../message-vault-io/crates/message/phone", package = "phone" }

# After (in crates/vault/server/Cargo.toml)
message-ir = { path = "../libs/ir" }
contacts   = { path = "../libs/contacts" }
phone      = { path = "../libs/phone" }
```

Dockerfiles must be updated to remove the `../message-vault-io` build-context
requirement — everything is now under one repo root.

### Crate renames

| Old name | New name |
|----------|----------|
| `message-vault-rs` | `message-vault-server` |
| `message-vault-io-core` | `message-vault-core` (or keep) |

### Constraints

- Full git history must be preserved (`--allow-unrelated-histories`)
- DockerHub push must continue working post-merge
- Single CI workflow replacing two
- `crates/libs/` are pure libraries (no binaries)
- `crates/exporters/` are dual-purpose (lib + CLI binary)
- `crates/cli/` are pure binaries
