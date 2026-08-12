# Vitest for the Vite web frontend

**Date:** 2026-08-12  
**Status:** approved  
**Scope:** `web/` (Vite React SPA) and CI for that package only. **No changes to `web-next/` under any circumstances.**

## Problem

`web/` has four unit tests under `src/lib/` written with Node’s built-in test runner (`node:test`). There is no `npm test` script and CI never runs those files. The package needs a durable test runner so more tests can be added as the SPA grows, and so every push/PR fails when those tests fail.

## Goals

1. Add Vitest as the test runner for `web/`.
2. Migrate the existing four `*.test.ts` files from `node:test` / `node:assert` to Vitest.
3. Expose `npm test` (CI-safe, non-watch) so local and CI use the same command.
4. Run tests on every push/PR to `main`.
5. Leave `web-next/` completely untouched.

## Non-goals

- React Testing Library, jsdom, or component/DOM tests in this pass.
- Coverage thresholds or coverage reporting gates.
- Any file, script, dependency, or CI step under `web-next/`.
- Rewriting product code except where a test migration requires a tiny fix.

## Approach

Brainstorm option **1**:

| Piece | Choice |
|-------|--------|
| Runner | Vitest, sharing the Vite config in `web/` |
| Existing tests | Migrate to `describe` / `it` / `expect` |
| Local/CI command | `"test": "vitest run"` |
| Watch (local only) | `"test:watch": "vitest"` |
| CI | Always-on job: `cd web && npm ci && npm test` |

## Package changes (`web/`)

Add `vitest` as a `devDependency` (version pinned by `npm install` / lockfile).

Add scripts:

```json
"test": "vitest run",
"test:watch": "vitest"
```

Do not add packages that only exist for Next.js or for `web-next/`.

## Config

Extend `web/vite.config.ts` (or add `web/vitest.config.ts` that references the Vite setup) so Vitest uses the same Vite plugins/transforms as the app.

Use the default Node environment for the current `src/lib` unit tests. Do not enable a browser/jsdom environment until component tests are in scope.

## Test migration

Rewrite these files to Vitest APIs:

- `web/src/lib/assetUrl.test.ts`
- `web/src/lib/contactRecentSearches.test.ts`
- `web/src/lib/missingAttachmentLabel.test.ts`
- `web/src/lib/savedGroups.test.ts`

Replace `node:test` imports and `node:assert/strict` with Vitest globals or explicit `vitest` imports. Keep the same behaviors and cases.

After migration, `cd web && npm test` must exit 0.

## CI (`.github/workflows/ci.yml`)

Add an always-on job (sibling of `web-lint`), for example `web-test`:

1. Checkout
2. Setup Node.js 22 with npm cache on `web/package-lock.json`
3. `cd web && npm ci && npm test`

Do not install or test `web-next/`. Do not change tag-only Tauri/Docker jobs beyond what is required for job naming consistency.

Optional later (out of scope): fold lint + test into one web job to save a Node install; separate jobs are fine for this pass.

## Success criteria

- From `web/`, `npm test` exits 0 and runs all four migrated suites.
- A failing assertion fails the new CI job.
- `git diff` against the base branch shows no changes under `web-next/`.
- Work lands on a branch created from `main` without unrelated local WIP.

## Follow-ups (out of scope)

- Add React Testing Library + jsdom for component tests.
- Enable coverage reporting once the suite is larger.
- Document `npm test` / `npm run test:watch` in maintainer docs if that page is updated for frontend tooling.
