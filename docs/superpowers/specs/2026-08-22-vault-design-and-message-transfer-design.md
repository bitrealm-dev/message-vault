# Vault Design, Message Transfer, and contributing placement

**Date:** 2026-08-22  
**Status:** Implemented

## Context

Contributing currently lives in Starlight (`docs/src/content/docs/vault/developer/contributing.md`) and includes a repository directory map. GitHub has no root `CONTRIBUTING.md`. `.github/CONTRIBUTING.md` is a stub that points at the hosted page and at the Starlight source file.

Architecture diagrams already exist, but they are easy to miss:

- C4 PlantUML + SVG: `docs/maintainers/architecture/puml/` (system, container, deployment)
- Session sequences (Mermaid): `docs/maintainers/architecture/sequence_diagram.md`

The Developer sidebar does not list those files. README still calls `docs/maintainers` “not yet ported.” The database page already renders Mermaid in Starlight. C4 drawings are PlantUML; converting them to Mermaid loses layout and C4 notation.

The seam for a developer is:

1. Get a clone compiling (contributing).
2. Learn how Message Vault is put together and how messages move (Starlight Developer pages).

## Goal

A contributor who opens GitHub finds a short `CONTRIBUTING.md` that sends them to the hosted contributing guide. After the vault compiles, the Developer guidebook has two overview pages: **Vault Design** (tree, binaries, C4, session sequences) and **Message Transfer** (exporter → JSONL → import, with supported vs rescue exporters). The directory map leaves contributing. C4 stays PlantUML SVG. Session sequences stay Mermaid.

## Non-goals

- Rewriting C4 diagrams as Mermaid
- Adding PlantUML to `astro build` or CI
- New C4 levels (component or code diagrams)
- Moving `message-ir.md` or GUI notes onto Vault Design
- Replacing [Export structure](/vault/developer/reference/export-structure/) — Message Transfer shows the basics and links there
- Rewriting every exporter CLI or mapping page body
- Changing User Guide import/rescue copy except links if a URL moves
- Runtime, exporter, desktop app, or vault-server product code
- Porting the full contributing guide out of Starlight

## Decisions

1. **Contributing source of truth stays Starlight.** `docs/src/content/docs/vault/developer/contributing.md` keeps environment setup, run the vault, tests, PR rules, and preview-the-guidebook. It drops the directory map. It links to Vault Design (and Message Transfer) for people who already compile.
2. **Root `CONTRIBUTING.md` is a stub.** Move `.github/CONTRIBUTING.md` to the repository root and keep it minimal: hosted URL `https://bitrealm.io/vault/developer/contributing/`, plus a local path to the Starlight source. Delete `.github/CONTRIBUTING.md` so GitHub has one contributing file.
3. **Developer sidebar order** (after the Developer index): Contributing, Vault Design, Message Transfer, then the existing Run from source / Docker / CLI / Formats / internals pages.
4. **Vault Design** is `/vault/developer/vault-design/`. One page. Intro (two processes), directory map, binaries from `cargo build --workspace`, three C4 SVGs with short captions, four Mermaid session sequences (start vault, sign in, import, export).
5. **C4 stays PlantUML.** Commit the existing SVGs into Starlight assets (`docs/src/assets/vault-design/`). Leave `.puml` files in `docs/maintainers/architecture/puml/` as the edit source. Re-export SVG and commit both when a diagram changes. Do not run PlantUML during the docs build.
6. **Session sequences stay Mermaid.** Move the four diagrams from `docs/maintainers/architecture/sequence_diagram.md` onto Vault Design. Replace that maintainer file with a pointer to the published page so there is one reading copy.
7. **Message Transfer** is `/vault/developer/message-transfer/`. Pipeline picture (simple three-box flow, not C4): **Exporter → JSONL folder → Import** (`vault-push` or desktop Import). Also the reverse: **vault-pull → JSONL folder**. JSONL shape in brief (one file per chat, header line then messages, `attachments/`, schema v3) plus a small sample, then a link to Export structure. Link lists: **Supported** (iMessage, SMS Backup & Restore, WhatsApp) and **Rescue / experimental** (GO SMS Pro, iMazing, OpenExtract, SMS Backup+). `message-reexporter`, `vault-push`, and `vault-pull` sit on this page as transfer tools, not in those two backup-source lists.
8. **CLI sidebar matches that split.** Group exporter CLI pages under Supported vs Rescue / experimental. Leave `message-reexporter`, `vault-push`, and `vault-pull` in a third group (vault JSONL tools). The CLI index page uses the same headings. Do not rewrite flag tables on those pages in this change.
9. **Maintainer index.** `docs/maintainers/README.md` points at the published Vault Design and Message Transfer pages instead of treating C4/sequences as the place people read. `message-ir.md` stays under maintainers and keeps being linked from Formats pages.
10. **No extra “you missed contributing” callout** beyond the Developer index already linking Contributing. Contributing stays in the sidebar.

## What changes

| Path | Change |
|------|--------|
| `CONTRIBUTING.md` (repo root, new) | Stub pointing at hosted contributing + Starlight source path |
| `.github/CONTRIBUTING.md` | Delete |
| `docs/src/content/docs/vault/developer/contributing.md` | Remove directory map; link to Vault Design and Message Transfer |
| `docs/src/content/docs/vault/developer/vault-design.md` | New page |
| `docs/src/content/docs/vault/developer/message-transfer.md` | New page |
| `docs/src/assets/vault-design/*.svg` | Copy the three existing C4 SVGs |
| `docs/astro.config.mjs` | Sidebar: Contributing, Vault Design, Message Transfer; CLI groups |
| `docs/src/content/docs/vault/developer/index.md` | Link Vault Design and Message Transfer |
| `docs/src/content/docs/vault/developer/reference/cli/index.md` | Supported vs rescue headings |
| `docs/maintainers/README.md` | Point at published pages |
| `docs/maintainers/architecture/sequence_diagram.md` | Replace body with a pointer |
| `README.md` | Leave the hosted contributing link as it is. Do not add Vault Design or Message Transfer there; the Developer index owns those links. |

`docs/src/content/docs/vault/developer/run-from-source.md` already points at Contributing. Leave that link. It does not need to grow a directory map.

## Vault Design page outline

1. **Intro** — Message Vault is two processes: the vault (HTTP API + SQLite) and the UI that talks to it. This page is the map for someone who already compiles.
2. **Directory map** — move the tree and “where first PRs go” block from contributing. Same skip warning for `crates/message-vault-io-gui/` and `web-next/`.
3. **Binaries** — table:

   | Binary | Comes from | Job |
   |--------|------------|-----|
   | `message-vault-server` | `crates/vault/server/` | Vault process |
   | `imessage-ir-exporter`, `sms-backup-restore-exporter`, `whatsapp-exporter` | `crates/exporters/` | Supported extract → JSONL |
   | `go-sms-pro-exporter`, `imazing-exporter`, `openextract-exporter`, `sms-backup-plus-exporter` | `crates/exporters/` | Rescue / experimental extract |
   | `message-reexporter` | `crates/libs/reexport/` | Convert an existing export folder |
   | `vault-push` / `vault-pull` | `crates/cli/` | JSONL → running vault / vault → JSONL |

   `src-tauri/` is the desktop window. It is not a workspace member. The same exporter libraries run inside that app.
4. **System context** — `vault_1_system_diagram.svg` plus one or two sentences.
5. **Containers** — `vault_2_container_diagram.svg` plus one or two sentences.
6. **Deployment (from source)** — `vault_4_deployment_diagram.svg` plus one or two sentences.
7. **Session sequences** — the four existing Mermaid diagrams, with their current participant names (Desktop App, Vite `:5173`, Vault `:8080`).

Link out to Database, Export structure, HTTP API, and `message-ir.md` rather than inlining those contracts.

## Message Transfer page outline

1. **Intro** — backups are not the vault. An exporter writes a JSONL folder; import loads that folder into a running vault. Export from the vault writes JSONL again (`vault-pull`).
2. **Block diagram** — simple flow (Mermaid flowchart is allowed here; this is not a C4 drawing):

   `Backup files → Exporter CLI or desktop Extract → JSONL folder → vault-push / Import screen → Vault`

   and

   `Vault → vault-pull / Export → JSONL folder`
3. **JSONL basics** — one `*.jsonl` per conversation; line 1 is the conversation header (`schema_version` 3); following lines are messages; media under `attachments/`. Short sample. Then link to Export structure.
4. **Supported exporters** — iMessage (`imessage-ir-exporter`), SMS Backup & Restore, WhatsApp. Link each CLI page (and mapping page when one exists).
5. **Rescue / experimental** — GO SMS Pro, iMazing, OpenExtract, SMS Backup+. Same link style. One sentence that these are incomplete or reverse-engineered sources (same idea as the User Guide rescue page).
6. **Vault JSONL tools** — `vault-push`, `vault-pull`, `message-reexporter`, with links to their CLI pages.

Do not paste the converter capabilities tables here. Link [Converter capabilities](/vault/developer/formats/) for that.

## Root CONTRIBUTING.md (intended copy)

```markdown
# How to Contribute

Contribution guidelines live in the Developer docs:

- Hosted: https://bitrealm.io/vault/developer/contributing/
- In this clone: `docs/src/content/docs/vault/developer/contributing.md`

After the vault compiles, [Vault Design](https://bitrealm.io/vault/developer/vault-design/) maps the tree and processes. [Message Transfer](https://bitrealm.io/vault/developer/message-transfer/) covers exporter → JSONL → import.
```

Keep the Starlight contributing page’s existing setup/PR prose except the directory-map subsection.

## Sidebar (Developer)

```text
vault/developer
vault/developer/contributing
vault/developer/vault-design
vault/developer/message-transfer
vault/developer/run-from-source
vault/developer/docker-compose
CLI tools
  vault/developer/reference/cli          (index)
  Supported
    imessage-ir-exporter
    sms-backup-restore-exporter
    whatsapp-exporter
  Rescue / experimental
    go-sms-pro-exporter
    imazing-exporter
    openextract-exporter
    sms-backup-plus-exporter
  Vault JSONL
    message-reexporter
    vault-push
    vault-pull
Formats (unchanged grouping)
Instance internals (unchanged)
```

Formats mapping pages stay under Formats. This change does not split that tree.

## Testing

From `docs/`: `npm run check` and `npm run build`. Confirm the new pages render, C4 SVGs display, Mermaid sequences render, and sidebar links resolve. `./scripts/check-pr.sh` already includes the docs check/build.

No new Rust or `web/` tests.

## Risks

- Root `CONTRIBUTING.md` will 404 the Vault Design / Message Transfer URLs until this change is on `main` and the docs site has deployed. That is acceptable; the contributing URL already exists.
- Two copies of C4 SVGs (maintainer `puml/img/` and Starlight assets) can drift. The rule is: edit `.puml`, export SVG, copy into `docs/src/assets/vault-design/` in the same change.
- `docs/maintainers/gui.md` is still Slint-era in places. Out of scope.
