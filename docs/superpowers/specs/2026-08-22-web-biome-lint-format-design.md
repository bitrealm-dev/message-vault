# Web lint and format with Biome

**Date:** 2026-08-22  
**Status:** Approved for implementation

## Context

The product UI lives in `web/` (Vite + React). ESLint already runs locally (`npm run lint`) and in CI as **Lint (web)**. Lint errors fail the job; warnings do not. There is no formatter: no Prettier, no Biome, and no format-on-save.

Rust already has the split the frontend lacks: `rustfmt` rewrites files locally, and CI runs `cargo fmt -- --check`. `scripts/check-pr.sh` applies `rustfmt` then only lints `web/`. The contributing spec `2026-08-22-contributing-opening-a-pr-design.md` left web auto-format out of scope on purpose. This spec replaces that one point. The contributing page and this spec are the source of truth going forward. That older spec is not rewritten.

`web-next/` is an old UI and keeps its own ESLint. `docs/` has no formatter and stays that way.

## Goal

One tool, Biome, lints and formats `web/`. Mixed quotes, spacing, wrapping, and import order stop showing up as noisy diffs. Local rewrite matches the Rust workflow. After a one-time rewrite of the tree, CI rejects format drift.

## Non-goals

- `docs/` and `web-next/`
- Format-on-save in the editor
- Markdown under `web/`
- Formatting `package-lock.json`, `dist/`, or `node_modules/`
- React Compiler-style lint rules (the current ESLint config leaves those off)
- Tightening lint beyond a close port of today’s rules, except where Biome recommended is a drop-in replacement
- Replacing TypeScript (`tsc`) with Biome as the type checker

## Tooling

`@biomejs/biome` is a `web/` devDependency. Config is `web/biome.json`. Git ignore files are honored so `dist/` and `node_modules/` stay out. `package-lock.json` is ignored: npm owns that file.

ESLint packages and `web/eslint.config.js` remain until the cutover pull request. After cutover they are removed. Biome is the only JavaScript/TypeScript linter and formatter for `web/`.

## Files formatted

TypeScript, JavaScript, CSS, JSON, and HTML under `web/`.

`index.html` contains an inline theme boot script. Format it only if Biome’s HTML output is checked and does not change that script’s behavior. If HTML formatting is still experimental or rewrites the IIFE unsafely, exclude `index.html` and format CSS, JSON, JS, and TS only.

## Formatter settings

Match the current `web/` look, not Biome’s defaults (Biome prefers tabs):

- 2-space indent
- Double quotes
- Semicolons always
- Line width 100 (same number `rustfmt` uses)
- Trailing commas where the language allows them
- LF line endings (already in `.editorconfig`)

Organize imports is on. The format-only pull request reorders imports as well as wrap and space. That stays in the rewrite pull request, not mixed into feature work.

## Lint rule mapping

Port today’s ESLint bar. Do not turn this into a crackdown.

| Today (ESLint) | After (Biome) |
|---|---|
| `js` + `typescript-eslint` recommended | Biome `recommended: true` |
| unused vars; names starting with `_` are ignored | Keep that: unused `_foo` is allowed |
| `react-hooks/rules-of-hooks` error | `useHookAtTopLevel` error |
| `react-hooks/exhaustive-deps` **warn** | `useExhaustiveDependencies` **warn** (Biome’s default is error; keep warn so this is not a new CI gate) |
| `react-refresh/only-export-components` warn, with `allowConstantExport` and the current `allowExportNames` list | Same idea as `useComponentExportOnlyModules` at **warn**, same allow-list. That rule is not in Biome recommended, so it is turned on explicitly |
| React Compiler rules off | Stay off. Do not enable nursery / compiler-style extras |

`biome migrate eslint` may seed `biome.json`. The checked-in file is then edited to match the table, not left as a raw dump.

If Biome recommended flags code that ESLint currently accepts, those new rules are set to `warn` or `off` in the first cutover so CI does not gain a surprise error bar. Tightening is a later change.

Prefer a real fix over `biome-ignore`. Unused bindings still use a `_` prefix instead of a suppression.

## Scripts

### `web/package.json`

- `npm run format` — rewrite matching files (`biome format --write .`)
- `npm run format:check` — report format drift, do not write
- `npm run lint` — stays `eslint .` until cutover; after cutover, lint only (`biome lint .`)

Warnings do not fail CI, same as today. After cutover, CI uses `biome ci .`, which fails on lint **errors** and on format drift.

### `scripts/format-all.sh` (new)

From the repository root, in order:

1. `cargo fmt --all`
2. `cargo fmt --manifest-path src-tauri/Cargo.toml`
3. `npm ci` in `web/` if `web/node_modules` is missing
4. `npm run format` in `web/`

Stops on the first failure (`set -euo pipefail`). Rewrites files. Does not lint, test, or build. Same `SCRIPT_DIR` / `REPO_ROOT` pattern as `scripts/run-vault-dev.sh`.

### `scripts/check-pr.sh`

Until the rewrite pull request lands, this script must **not** format `web/`. If it did, the first contributor to run it would dump the whole rewrite onto a random branch.

After cutover, the format steps call `format-all.sh` instead of duplicating `cargo fmt`. Then: workspace build/test, `npm run lint` and `npm run test` in `web/`, then docs check/build. If rustfmt or Biome rewrote files, those changes must be committed before opening the pull request.

## CI

`.github/workflows/ci.yml`:

- The rustfmt job is unchanged
- **Lint (web)** stays ESLint through the tooling and format-only pull requests
- After cutover: that job runs `npm ci` then `biome ci .` (lint + format check)
- The Vitest job is unchanged

## Pull requests

Three pull requests, in this order.

### PR 1 — tools only

Add `@biomejs/biome`, `web/biome.json`, npm `format` / `format:check` scripts, and `scripts/format-all.sh`. Update `AGENTS.md` and the contributing page to describe the new commands, and say CI does not check web format yet. Leave ESLint in place. Do not rewrite the tree. Do not point `check-pr.sh` at web format yet.

### PR 2 — format only

Run the web half of `format-all.sh` (or `npm run format` in `web/`). Commit only the rewritten sources. No logic changes. ESLint still gates CI.

### PR 3 — cutover

Delete `web/eslint.config.js` and ESLint npm packages. Point `npm run lint` at Biome. Point `check-pr.sh` at `format-all.sh`. Switch the CI web job to `biome ci .`. Fix only leftover lint that would fail CI under the ported rule table. Update `AGENTS.md` and the contributing page so the commands match reality.

## Docs

Update these in the pull request that makes the command true:

- `AGENTS.md` — frontend section today says there is no formatter job. After cutover it lists `npm run format`, `npm run lint` as Biome, and `./scripts/format-all.sh`.
- `docs/src/content/docs/vault/developer/contributing.md` — **Opening a PR** currently says `check-pr.sh` does not auto-format `web/`. After cutover it says the script runs `format-all.sh` (Rust + web rewrite), then lint/test.

## Verification

PR 1: `cd web && npm run format:check` reports drift (expected). `npm run lint` still ESLint and still passes. `scripts/format-all.sh` is executable and formats Rust + web when run on purpose.

PR 2: `cd web && npm run format:check` is clean. `npm test` and `npm run build` still pass. Diff is formatting and imports only.

PR 3: ESLint packages are gone. `cd web && npm run lint` and CI `biome ci .` pass. `./scripts/check-pr.sh` formats with `format-all.sh`, then lints and tests. `web-next/` still has its own ESLint.

## Success criteria

- Contributors format Rust and `web/` with one command: `./scripts/format-all.sh`
- After the rewrite, CI fails if `web/` is unformatted, the same way it fails if Rust is unformatted
- ESLint is gone from `web/`; Biome is the linter and formatter
- Lint errors fail CI; warnings do not
- `docs/` and `web-next/` are untouched
