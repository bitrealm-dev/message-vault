# Name aliases Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans or subagent-driven-development. Steps use checkbox (`- [ ]`) syntax.

**Goal:** Rename `name_hint` → `name_alias`, seed aliases on import (first wins), Appearance toggle for display, Alias column on Contact Identity.

**Architecture:** Alias lives on `contact_handles.name_alias`. Toggle in localStorage (like theme). Conversation/message label enrichment reads alias when toggle is on (client and/or server — prefer server enrichment keyed by account pref if API exists; else client uses returned `name_alias` + preferred name). Spec: no backward compatibility; wipe DBs.

**Tech Stack:** SQLite DDL, Rust vault server, React web app

## Global Constraints

- No dual-read of `name_hint`
- First wins for alias seed
- Toggle default off; Contacts list title always preferred name
- Historical `docs/superpowers/plans/*` may keep old spelling

---

### Task 1: Schema + mechanical rename

**Files:** `schema/sql/*.sql`, vault server, web, web-next generated schema, fixtures, `database.md` (not historical plans)

- [x] Replace `name_hint` → `name_alias` in schema SQL
- [x] Sync/regenerate web-next vaultSchema if scripted
- [x] Rename in Rust/TS code (fields, SQL strings, JSON)
- [x] `cargo test -p message-vault-server` relevant modules
- [ ] Commit

### Task 2: Seed `contact_handles.name_alias` on import

**Files:** `crates/vault/server/src/import.rs` (+ tests)

- [x] When linking participant handle to contact, if `contact_handles.name_alias` empty and import display name non-empty, UPDATE set alias
- [x] Test first-wins
- [ ] Commit

### Task 3: Display enrichment + API fields

**Files:** `conversations_api.rs`, contact detail GET for handles, web types

- [x] Return `name_alias` on contact handle detail
- [x] Enrichment: when aliases enabled… (Task 4 wires toggle; enrichment can always return both preferred + alias and let client choose, OR pass header/query — prefer client chooses from API fields for v1 localStorage toggle)
- [ ] Commit

### Task 4: Appearance toggle + Contact Identity Alias column

**Files:** `AppearanceSection`, theme-adjacent pref, `ContactDrawerHandles`, conversation/message label helpers

- [x] `mv-use-name-aliases` localStorage, default false, UI above Theme
- [x] Alias column view-only
- [x] Thread/message labels respect toggle
- [ ] Commit pending Contact Identity / Threads label UI if still dirty
- [ ] Commit
