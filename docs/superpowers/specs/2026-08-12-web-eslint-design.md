# ESLint for the Vite web frontend

**Date:** 2026-08-12  
**Status:** approved  
**Scope:** `web/` (Vite React SPA) and CI for that package only. No changes to `web-next/`.

## Problem

The Vite SPA under `web/` ships TypeScript and React, but has no JavaScript/TypeScript linter. The only static check today is `tsc` during `npm run build`. That catches type errors and misses common React/hooks mistakes that ESLint finds.

`web-next/` already runs ESLint via `eslint-config-next`. That stack is Next-specific and must not be applied to the Vite app.

## Goals

1. Add ESLint 9 (flat config) for TypeScript and React in `web/` only.
2. Expose `npm run lint` so developers and CI can run the same command.
3. Run that lint step on every push/PR to `main` so regressions fail before merge.
4. Leave `web-next/` unchanged.

## Non-goals

- Type-aware ESLint rules (`projectService` / typed linting). Those can wait until the basic pass is stable.
- Replacing or changing Prettier / formatters (none is required for this change).
- Linting `web-next/`, docs, or other packages.
- Changing product behavior in the SPA beyond fixes required to make lint pass.

## Approach

Use the recommended lightweight stack (brainstorm option **1** / delivery option **B**):

| Piece | Choice |
|-------|--------|
| ESLint | 9.x, flat config file in `web/` |
| TypeScript | `typescript-eslint` recommended (not type-checked) |
| React | `eslint-plugin-react-hooks` and `eslint-plugin-react-refresh` |
| Entry command | `"lint": "eslint ."` in `web/package.json` |
| CI | New always-on job: Node 22 → `cd web && npm ci && npm run lint` |

## Package changes (`web/`)

Add these `devDependencies` (exact versions pinned by `npm install` / lockfile update):

- `eslint`
- `@eslint/js`
- `typescript-eslint`
- `eslint-plugin-react-hooks`
- `eslint-plugin-react-refresh`

Add script:

```json
"lint": "eslint ."
```

Do not add Next ESLint packages.

## Config (`web/eslint.config.js`)

Flat config that:

1. Applies recommended JS + TypeScript + React hooks/refresh rules to `**/*.{ts,tsx}`.
2. Ignores build output and dependencies (`dist`, `node_modules`, and similar).
3. Lives entirely under `web/` so nothing under `web-next/` is scanned when lint runs from `web/`.

No shared monorepo ESLint config at the repo root for this change.

## CI (`.github/workflows/ci.yml`)

Add a job that runs on the same triggers as the existing always-on jobs (`push`/`pull_request` to `main`, plus `workflow_dispatch`):

1. Checkout
2. Setup Node.js 22 with npm cache keyed on `web/package-lock.json`
3. `cd web && npm ci && npm run lint`

This job does not install or lint `web-next/`.

Existing tag-only Tauri steps that already build `web/` stay as they are.

## Handling existing violations

On the first `npm run lint` run:

1. Prefer fixing real issues in `web/src`.
2. Use targeted rule disables only when a fix would be large or incorrect for the current pattern.
3. Avoid turning off whole rule sets globally unless a rule is clearly wrong for this codebase.

CI must pass with zero unresolved errors before merge.

## Success criteria

- From `web/`, `npm run lint` exits 0 on a clean tree.
- A PR that introduces a hooks violation fails the new CI job.
- `web-next/` package files and its lint setup are unchanged.
- Work lands on a branch created from `main` without unrelated local WIP.

## Follow-ups (out of scope)

- Enable type-aware `typescript-eslint` rules once the recommended set is green.
- Optionally add `npm run build` to the same always-on web job if frontend compile checks should gate every PR (today the Vite build mainly runs on the tag/Tauri path).
