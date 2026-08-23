# Generate CLI pages from clap and host rustdoc on bitrealm.io

**Date:** 2026-08-22  
**Status:** Approved for planning

## Context

The docs site is Astro Starlight on GitHub Pages at [bitrealm.io](https://bitrealm.io). Developer docs already generate the HTTP API from server code: utoipa, committed `docs/src/assets/openapi.json`, `starlight-openapi` at `/vault/developer/reference/http/`, and a short guide at `/vault/developer/reference/api/` (merged in [#85](https://github.com/bitrealm-io/message-vault/pull/85)). This project does not change that path.

CLI flag pages are still typed by hand in man-page form (`NAME`, `SYNOPSIS`, `OPTIONS`) under `docs/src/content/docs/vault/developer/reference/cli/` and `server-cli.md`. The same flags already live in clap `///` comments in the binaries. Those pages drift when someone adds a flag and forgets the docs.

Crate API comments (`///`, `//!`) are not published. There is no `cargo doc` in CI. Crates are not on crates.io, so docs.rs is not an option. `message-vault-server` is a binary crate (`main.rs` only), so rustdoc would show almost nothing until the code lives in a library.

Docs CI (`.github/workflows/docs.yml`) is Node-only today. It runs on push to `main` when `docs/` changes, not on pull requests. A recent docs run was about one minute. The Rust test job on the same push was about seven minutes. Public GitHub Actions minutes are free for this repo.

## Goal

Clap is the source of truth for command-line flags. `cargo doc` is the source of truth for crate APIs. Both appear on bitrealm.io. Readers never talk to a running vault.

- Replace the hand-written CLI man pages with markdown dumped from clap. Keep the same URLs.
- Keep one hand-written index at `/vault/developer/reference/cli/` that says which tool to pick.
- Publish rustdoc HTML at `https://bitrealm.io/vault/developer/rustdoc/` for the Cargo workspace except the old Slint GUI.
- Commit the clap markdown (small). Do not commit rustdoc HTML (large). Build rustdoc in docs CI.

## Non-goals

- Replacing or extending the utoipa HTTP API docs
- Generating TypeScript from OpenAPI
- Hosting rustdoc on docs.rs
- Publishing Unix `.1` man files on the developer’s machine
- New Starlight pages for helper binaries that have no docs today (`contacts_validate`, `demo-seed`, `imazing_obfuscate`)
- Documenting `src-tauri` (not a workspace member)
- Documenting the Slint GUI crate
- Turning rustdoc comments into Starlight markdown (rustdoc stays as `cargo doc` HTML)
- Generating clap markdown at docs-build time (it is committed, like `openapi.json`)
- `cargo doc` on pull requests
- Failing CI on rustdoc warnings in this work (`-D warnings` is out)
- Browser tests of rustdoc search
- User Guide pages (those stay hand-written)

## Decisions

1. **Hybrid hosting.** Clap pages are Starlight markdown. Rustdoc is `cargo doc` HTML under `/vault/developer/rustdoc/`, linked from the Developer sidebar. Same hostname, two chrome styles.

2. **Commit clap markdown; build rustdoc in CI.** `dump-cli-docs` writes the markdown. `cargo test` fails if those files are stale. Docs CI installs Rust, runs `cargo doc`, copies HTML into `docs/public/vault/developer/rustdoc/`, then builds Starlight. Cargo cache on. Timeout about 20 minutes.

3. **Docs CI on `main` only.** Trigger when `docs/` **or** crate files change (`crates/**`, root `Cargo.toml`, `Cargo.lock`, the workflow file). Always rebuild rustdoc on those deploys so a docs-only edit cannot wipe crate-API pages off GitHub Pages. Do not run this job on PRs.

4. **One dump command.** A small workspace crate `dump-cli-docs` writes every documented CLI page. We do not add `--dump-markdown` to each tool. The server crate does not depend on every exporter.

5. **Replace each man page entirely.** Extra story (limits, workflow) belongs in clap `about` / `long_about` or in the User Guide, not in a second hand-written copy of the flags.

6. **Only the commands we already document.** Index stays authored. Generated files:

   | Command | Output file |
   |---------|-------------|
   | `imessage-ir-exporter` | `docs/src/content/docs/vault/developer/reference/cli/imessage-ir-exporter.md` |
   | `sms-backup-restore-exporter` | `…/cli/sms-backup-restore-exporter.md` |
   | `whatsapp-exporter` | `…/cli/whatsapp-exporter.md` |
   | `go-sms-pro-exporter` | `…/cli/go-sms-pro-exporter.md` |
   | `imazing-exporter` | `…/cli/imazing-exporter.md` |
   | `openextract-exporter` | `…/cli/openextract-exporter.md` |
   | `sms-backup-plus-exporter` | `…/cli/sms-backup-plus-exporter.md` |
   | `message-reexporter` | `…/cli/message-reexporter.md` |
   | `vault-push` | `…/cli/vault-push.md` |
   | `vault-pull` | `…/cli/vault-pull.md` |
   | `message-vault-server` | `docs/src/content/docs/vault/developer/reference/server-cli.md` |

7. **Each documented crate exports `pub fn clap_command() -> clap::Command`.** That function is `Cli::command()` (clap `CommandFactory`) from the existing `Parser` type. `main.rs` stays a thin wrapper. Needed so `dump-cli-docs` can see the command without running the tool, and so rustdoc can see server types.

8. **Starlight titles stay readable.** `dump-cli-docs` owns a small table of command → `{ title, description }` matching today’s page titles (for example “Push to Message Vault”, not only `vault-push`). Flag text comes from clap. Sidebar slugs and Limited badges stay in `astro.config.mjs`.

9. **Split `message-vault-server` into `lib.rs` + `main.rs`.** Behavior does not change. Rustdoc and `dump-cli-docs` both use the library.

10. **Public items only.** `cargo doc --workspace --no-deps --exclude message-vault-io-gui`. No `--document-private-items`. Crates.io dependency pages are absent (`--no-deps`); workspace crates still link to each other.

11. **Generated files are not edited by hand.** Each dumped page has a short “generated by dump-cli-docs; do not edit” note and Starlight `editUrl: false`. A hand edit on a docs-only PR may land (Rust tests skip `docs/**`, same as OpenAPI). The next dump or `cargo test` on a crate PR overwrites or fails.

12. **Do not drop useful non-flag prose.** `server-cli.md` today has more than flags (for example process-assets counter meanings). That material moves into clap `long_about` or onto the hand-written CLI index / HTTP guide before the man page is deleted. It does not disappear.

## Architecture

Two pipelines. Both start from Rust. Both end on bitrealm.io.

**CLI flag pages.** Each documented tool exposes its clap `Command` from the library (enable the crate’s `cli` feature where clap is optional). `dump-cli-docs` depends on those libraries, runs [clap-markdown](https://crates.io/crates/clap-markdown), prepends Starlight frontmatter (title, description, `editUrl: false`), and writes the files in the table above. `cargo test -p dump-cli-docs` compares dump output to the committed files. Starlight publishes them at the current URLs.

**Crate API pages.** Docs CI on `main` runs `cargo doc --workspace --no-deps --exclude message-vault-io-gui`, copies `target/doc/` to `docs/public/vault/developer/rustdoc/`, then `npm ci` / `npm run check` / `npm run build` in `docs/`. GitHub Pages deploys `docs/dist`. Opening `/vault/developer/rustdoc/` shows the usual rustdoc crate list.

Local `astro dev` does not include rustdoc unless you ran the copy step yourself. That is fine. Publish is what matters.

HTTP OpenAPI stays on the utoipa path. User Guide stays authored.

## Components

| Piece | Role |
|-------|------|
| `crates/cli/dump-cli-docs` | New small binary crate. `dump-cli-docs --output-dir docs/src/content/docs/vault/developer/reference` |
| `clap-markdown` | Markdown body from a clap `Command`. If it cannot produce a usable page, wrap `Command::render_help()` in a fenced block and still add Starlight frontmatter. Prefer clap-markdown. |
| Exporter / `vault-push` / `vault-pull` / `reexport` / server libraries | `pub fn clap_command() -> clap::Command` via `Cli::command()` |
| Title table in `dump-cli-docs` | Starlight `title` and `description` per command (not generated from clap name) |
| `crates/vault/server` lib split | Modules move to `lib.rs`; `main.rs` only parses CLI and calls the library |
| Committed markdown | Replaces the current man-style pages; URLs unchanged |
| `docs/src/content/docs/vault/developer/reference/cli/index.md` | Hand-written chooser. Not generated |
| `.github/workflows/docs.yml` | Node 24 + Rust stable, Cargo cache, rustdoc copy, timeout 20 minutes, path filters include crates |
| `docs/public/vault/developer/rustdoc/` | CI-only output. Add to `.gitignore` if a local copy appears. Do not commit |
| `docs/astro.config.mjs` | Sidebar link “Rust crate docs” → `/vault/developer/rustdoc/` |
| Developer index | One line pointing at rustdoc and reminding people that CLI flags come from clap |
| `CHANGELOG.md` | Unreleased note |

## Data flow

**CLI pages (when a flag changes).** Edit the clap comment in the crate. From the repository root:

```bash
cargo run -p dump-cli-docs -- --output-dir docs/src/content/docs/vault/developer/reference
```

Commit the rewritten markdown in the same PR. `cargo test -p dump-cli-docs` must pass. Docs CI builds Starlight from `docs/`. Readers keep using `/vault/developer/reference/cli/<tool>/`.

**Crate API pages (on merge to `main`).** Docs CI compiles rustdoc, copies HTML, builds Starlight, deploys Pages. Readers use `/vault/developer/rustdoc/`.

**Optional local rustdoc** (not required to publish):

```bash
cargo doc --workspace --no-deps --exclude message-vault-io-gui --open
```

`dump-cli-docs` does not open SQLite, bind a port, or read `config.toml`.

## Error handling

- **`dump-cli-docs`:** If a clap command cannot be built or a file cannot be written, print a plain stderr message and exit non-zero. CI fails.
- **Stale markdown:** Test failure tells the developer to run the dump command. No silent fallback to old flag pages.
- **Rustdoc CI:** If `cargo doc` fails, the docs job fails and Pages is not updated. The previous site stays up.
- **`--no-deps`:** Types from crates.io have no local rustdoc page. Expected.

## Testing

Existing crate tests still pass after moving `Cli` into libraries and splitting the server. Command-line behavior does not change.

New checks:

- Dump output has valid Starlight frontmatter and a body for every file in the table above.
- Dump output equals the committed files (string compare, same idea as OpenAPI).
- `cargo doc --workspace --no-deps --exclude message-vault-io-gui` succeeds in docs CI.
- After a docs CI run, `docs/dist/vault/developer/rustdoc/index.html` exists.
- Starlight still builds the CLI index and the generated command URLs.
- Sidebar contains the rustdoc link.

The stale-markdown test runs in the existing Rust CI job (`cargo test --workspace`). Docs-only PRs still skip that job. A clap change without a dump fails on the crate PR.

Not in scope: browser tests of rustdoc; generating `web/` types.

## What changes

| Path | Change |
|------|--------|
| `Cargo.toml` (workspace) | Add `crates/cli/dump-cli-docs` |
| `crates/cli/dump-cli-docs/` | New crate |
| Each documented CLI crate | Export clap `Command` from the library; thin `main.rs` |
| `crates/vault/server/` | `lib.rs` + `main.rs`; export server clap command |
| CLI markdown files in the table above | Generated; replace man-page prose |
| `docs/src/content/docs/vault/developer/reference/cli/index.md` | Unchanged role; maybe one line that flags are generated |
| `docs/src/content/docs/vault/developer/index.md` | Link to rustdoc |
| `docs/astro.config.mjs` | Sidebar link to `/vault/developer/rustdoc/` |
| `.github/workflows/docs.yml` | Rust toolchain, cache, rustdoc, broader path filters, timeout 20 |
| `.gitignore` | Ignore local `docs/public/vault/developer/rustdoc/` if needed |
| `CHANGELOG.md` | Unreleased: generated CLI reference and rustdoc on bitrealm.io |

Limited badges on experimental exporter sidebar entries stay in `astro.config.mjs`. They are not generated from clap.

## Verification

- `cargo test -p dump-cli-docs` passes, including dump ≡ committed markdown
- `cargo test --workspace` still passes
- `dump-cli-docs` runs without `config.toml` or a database
- Hand-written CLI index still explains which tool to use
- Old `NAME` / `SYNOPSIS` man layout is gone from the generated command pages
- `cd docs && npm run check && npm run build` still works without rustdoc copied in (local docs-only)
- Docs CI on `main` produces `/vault/developer/rustdoc/` in the Pages artifact
- Docker / `serve` behavior is unchanged
- Product version bump does not require a rustdoc commit (HTML is not in git)

## Success criteria

- A developer looking up a flag finds it on bitrealm.io without copying `--help` into markdown
- Changing a clap flag without running `dump-cli-docs` fails `cargo test`
- A developer looking up a workspace type finds rustdoc on bitrealm.io without reading the `.rs` file in GitHub
- HTTP API docs remain the utoipa pages already on main
- Docs CI stays free (public repo) and usually finishes in well under 20 minutes with cache
