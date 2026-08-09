# Dual-source demo Implementation Plan

> **For agentic workers:** Implement task-by-task. Steps use checkbox syntax.

**Goal:** Demo vault has `imessage` + `sms-backup-restore` sources with light overlap; header shows `imessage` or `SMS/MMS`; Sources API works.

**Architecture:** `demo-seed` writes two staging trees; `reset-demo` imports both; conversation list derives service from `messages.source`; add sources endpoint.

**Tech Stack:** Rust (demo-seed, message-vault-server), SQLite, existing IR JSONL.

## Global Constraints

- Source ids: `imessage`, `sms-backup-restore` only for this work
- Header: imessage-only → `imessage`; any sms-backup-restore → `SMS/MMS`
- Light overlap; no raw XML; no service-pill redesign beyond derivation

---

### Task 1: Config + dual-tree generation

- [x] Add `[sources]` to `demo_seed.toml` and `SourcesConfig` in config.rs
- [x] Partition 1:1 contacts; write iMessage and Android trees; overlap shared fingerprints
- [x] Update `lib.rs` paths; copy attachment blobs into both trees
- [x] `cargo test -p demo-seed`

### Task 2: reset-demo dual import

- [x] Import imessage Replace then sms-backup-restore Append
- [x] Bundle completeness checks both staging dirs
- [x] `cargo test -p message-vault-server` (related)

### Task 3: Header service + sources API

- [x] Derive conversation `service` from message sources
- [x] Implement `GET /v1/export/conversations/:id/sources`
- [x] Tests for derivation + sources query
- [x] Update try-the-demo.md briefly

### Task 4: Regenerate committed demo + verify

- [x] `cargo run -p demo-seed` into `demo/`
- [x] Spot-check headers/sources / overlap fingerprints
