# Contributing: Opening a PR

**Date:** 2026-08-22  
**Status:** Approved for planning

## Context

`docs/src/content/docs/vault/developer/contributing.md` has **Making Code Changes** (issue, branch, commits, push) and a **Test before a pull request** subsection that lists six command blocks. That checklist belongs in **Opening a PR**, as the step before the PR is treated as ready. Contributors should not copy a command list; one script should run the local bar.

The project has one maintainer. GitHub already has a default PR template plus optional feature and bug-fix forms. CI (`.github/workflows/ci.yml`) runs rustfmt (workspace + `src-tauri`), `cargo build --workspace`, `cargo test --workspace`, and `web/` lint + test. Docs check/build is `.github/workflows/docs.yml`.

The [tenthirtyam contributor-flow article](https://tenthirtyam.org/dispatches/2026/03/21/writing-practical-contribution-guidelines-for-github-repositories/#contributor-flow) is still the structural reference for “when opening.” This section also covers the pre-PR check and staying current with `main`.

## Goal

A contributor who has pushed a topic branch can: run one script that matches the local CI bar, update the branch if `main` moved, open a pull request against `main` with the default template, and link the issue.

## Non-goals

- Rewriting **Making Code Changes** body (only move tests out of it)
- Rewriting **Docs site**, **Workspace map**, **License**, or **Contribution rules** except the existing `#test-before-a-pull-request` link
- Changing `.github/` pull request templates or CI workflows
- Requiring a feature or bug-fix template
- Requiring GitHub drafts
- DCO / `--signoff`
- `--fixup` or `--force-with-lease` recipes
- Flags such as `--skip-docs` on the script
- Putting `cargo test -p …` into the script
- Web or docs auto-format (no Prettier, no `eslint --fix`). Web stays lint + test only.
- Runtime, exporter, desktop app, or vault-server product code

## Decisions

1. **Tests live under Opening a PR.** Remove `### Test before a pull request` from **Making Code Changes**. **Making Code Changes** ends after the `upstream` git example.
2. **One script, always everything.** Add `scripts/check-pr.sh`. From the repository root it runs, in order, and stops on the first failure:
   1. `cargo fmt --all` (rewrites files; not `--check`)
   2. `cargo fmt --manifest-path src-tauri/Cargo.toml` (rewrites files; not `--check`)
   3. `cargo build --workspace`
   4. `cargo test --workspace`
   5. `web/`: `npm ci` if `web/node_modules` is missing, then `npm run lint` and `npm test` (no format rewrite)
   6. `docs/`: `npm ci` if `docs/node_modules` is missing, then `npm run check` and `npm run build`
3. **No skip flags.** First run takes several minutes. Later runs skip `npm ci` when `node_modules` already exists. If rustfmt changed files, those changes must be committed before opening the PR. CI still uses `cargo fmt -- --check`.
4. **Single-crate tests stay optional** in the docs only (`cargo test -p …` while iterating). Not in the script.
5. **Open against `main`.** Default GitHub template is enough. Feature and bug-fix templates exist and are not required.
6. **Link the issue.** `Ref: #123`. `Fixes #123` if this change should close that issue.
7. **Light title prefixes.** Prefer `feat:`, `fix:`, or `docs:` when they fit.
8. **Stay current with merge.** `git fetch upstream`, `git merge upstream/main`, `git push`. Rebase is allowed. Merge is enough. Do not force-push unless the branch is only used by that contributor.
9. **Optional `gh`.** One `gh pr create` example.
10. **After open.** GitHub runs checks. Fix failing checks. Reply on the same pull request.

## What changes

1. Create `scripts/check-pr.sh` (executable, `set -euo pipefail`, same `SCRIPT_DIR` / `REPO_ROOT` pattern as `scripts/run-vault-dev.sh`).
2. Edit `docs/src/content/docs/vault/developer/contributing.md`:
   - Delete `### Test before a pull request` and its command blocks (including the CI paragraph that currently sits there).
   - Insert **Opening a PR** after **Making Code Changes** and before **Docs site**.
   - Change Contribution rules item 3 from `#test-before-a-pull-request` to `#opening-a-pr` (or the matching Starlight slug for **Opening a PR**).

Do not edit `.github/pull_request_template.md` or files under `.github/PULL_REQUEST_TEMPLATE/`.

## Script

```bash
#!/usr/bin/env bash
# Local pre-PR check: apply rustfmt, then workspace build/test, web lint/test,
# docs check/build.
#
#   ./scripts/check-pr.sh
#
# Stops on the first failure. rustfmt rewrites files (not --check).
# Web formatting is not applied. Runs npm ci in web/ and docs/ only when
# that tree has no node_modules yet.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
cd "${REPO_ROOT}"

echo "==> cargo fmt (workspace)"
cargo fmt --all

echo "==> cargo fmt (src-tauri)"
cargo fmt --manifest-path src-tauri/Cargo.toml

echo "==> cargo build --workspace"
cargo build --workspace

echo "==> cargo test --workspace"
cargo test --workspace

if [[ ! -d web/node_modules ]]; then
  echo "==> npm ci (web)"
  (cd web && npm ci)
fi
echo "==> web lint"
(cd web && npm run lint)
echo "==> web test"
(cd web && npm test)

if [[ ! -d docs/node_modules ]]; then
  echo "==> npm ci (docs)"
  (cd docs && npm ci)
fi
echo "==> docs check"
(cd docs && npm run check)
echo "==> docs build"
(cd docs && npm run build)

echo "All pre-PR checks passed."
```

## Intended copy

### Opening a PR

Run the checks, then open a pull request against `main`. Do this after **Making Code Changes**. The first compile and the first `npm ci` each take several minutes.

**Before it is ready**

From the repository root:

```bash
./scripts/check-pr.sh
```

That script applies rustfmt to the workspace and to `src-tauri/` (it rewrites files). Then it builds and tests the workspace, lints and tests `web/`, and checks and builds `docs/`. It does not auto-format `web/`. It stops on the first failure. It runs `npm ci` in `web/` or `docs/` only when that tree has no `node_modules` yet. If rustfmt changed files, commit those changes before opening the pull request.

While iterating on one crate, `cargo test -p go-sms-pro-exporter` is enough. Exporter smoke tests use committed fixtures. Personal phone backups are not required.

**Keep the branch current**

If `main` has moved, update the branch before asking for review:

```bash
git fetch upstream
git merge upstream/main
git push
```

Rebase is allowed. Merge is enough. Do not force-push unless the branch is only used by that one contributor.

**Open the pull request**

A pull request asks to merge the branch into `main`. Open it against `main` on [bitrealm-io/message-vault](https://github.com/bitrealm-io/message-vault). Use the GitHub pull request form. GitHub fills in the default template. That default is enough for most changes. Feature and bug-fix templates also exist; they are not required.

Link the issue (`Ref: #123`). Write `Fixes #123` in the description if this change should close that issue.

Prefer `feat:`, `fix:`, or `docs:` at the start of the title when it fits.

From the repository root, this also works:

```bash
gh pr create --base main --title "feat: add support for x" --body "Ref: #123"
```

**After it is open**

GitHub runs checks. Fix failing checks. Reply to review comments on the same pull request.

## Voice

Match the rest of the new contributing page: short sentences, concrete commands, no “we” / “us” / “our”. Do not use GitHub `> [!TIP]` alerts.

## Verification

- **Making Code Changes** ends at the `upstream` git example
- **Opening a PR** sits before **Docs site** and includes **Before it is ready**, **Keep the branch current**, **Open the pull request**, **After it is open**
- `scripts/check-pr.sh` exists, is executable, and matches the script in this spec (rustfmt without `--check`; no web format step)
- Contribution rules no longer link `#test-before-a-pull-request`
- Default template is enough; feature/bug-fix templates optional
- No DCO, draft-PR requirement, or `--force-with-lease` cookbook
- `.github/` templates unchanged
- `cd docs && npm run check && npm run build` still succeeds
- `./scripts/check-pr.sh` is documented as the main pre-PR command (do not require a full run in CI for this docs change if the change is docs+script only; run the script locally when implementing)

## Success criteria

- A first-time contributor runs one script before opening a PR, then opens against `main` with the default template and an issue link
- Feature and bug-fix templates are named as optional
- The six-block command list is not the main path
