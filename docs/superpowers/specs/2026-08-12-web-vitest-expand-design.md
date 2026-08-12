# Expand Vitest coverage for the Vite web frontend

**Date:** 2026-08-12  
**Status:** approved  
**Scope:** `web/` only. **No changes to `web-next/`.**

## Problem

Vitest is already wired (`npm test`, CI job `Test (web)`). Nine unit files cover a subset of helpers. After the sequenced cleanup, more pure modules and shared UI primitives exist without tests. There is still no jsdom / Testing Library path for component or hook tests that need a DOM.

## Goals

1. Add critical **unit tests** for high-value pure modules introduced or left untested by cleanup.
2. Add a small **component-test starter kit** (jsdom + Testing Library) with a few smoke tests so future UI tests have a clear pattern.
3. Keep `lib/*.test.ts` on the Node environment; run `*.test.tsx` under jsdom.
4. Leave CI using the same `npm test` command.
5. Never touch `web-next/`.

## Non-goals

- Full screen / Import / MessageView integration tests.
- Playwright or other E2E.
- Coverage percentage gates.
- Snapshot farms.
- Enabling React Compiler ESLint rules.

## Approach

Brainstorm option **C** / design option **1**:

| Layer | Choice |
|-------|--------|
| Unit | Pure helpers + mocked API where needed |
| Hooks | `renderHook` via Testing Library where the hook needs React |
| Components | 2–4 smoke tests on small primitives (`FormField`, `Button`, `ListRangeHeader`) |
| Environments | `environmentMatchGlobs`: `**/*.test.tsx` → `jsdom`; default `node` |

## Package changes (`web/`)

Add as `devDependencies` (versions from `npm install`):

- `jsdom`
- `@testing-library/react`
- `@testing-library/jest-dom`
- `@testing-library/user-event`

Scripts stay:

```json
"test": "vitest run",
"test:watch": "vitest"
```

## Vitest config (`web/vite.config.ts`)

Update the `test` block roughly to:

```ts
test: {
  environment: "node",
  include: ["src/**/*.{test,spec}.{ts,tsx}"],
  environmentMatchGlobs: [
    ["**/*.test.tsx", "jsdom"],
    ["**/*.spec.tsx", "jsdom"],
  ],
  setupFiles: ["src/test/setup.ts"],
},
```

Create `web/src/test/setup.ts` that imports `@testing-library/jest-dom/vitest` (or the package’s Vitest entry).

## Unit test batch (priority)

Add or extend tests for (skip any that are awkward without heavy mocks):

| Module | Focus |
|--------|--------|
| `lib/fetchConversationById.ts` | Finds match across pages; returns null; respects abort |
| `lib/handleService.ts` | Options lists / type narrowing helpers |
| `lib/contactInitials.ts` | Initials from names/handles |
| `lib/nameAliases.ts` | Pure alias logic if present |
| `lib/portaledOverlay.ts` | Outside-click ignore rules |
| `lib/exportSources.ts` | Known source list / labels if pure |
| `lib/system-settings.ts` | Pure staging name helpers if still exported or testable via public API |

Prefer mocking `apiClient` / `fetch` at the module boundary for `fetchConversationById` — do not hit a real server.

## Hook tests

With jsdom + Testing Library:

- `useAsyncAction`: busy flag, error string, `clearError`, success path.
- `useResource`: loading → data; error path; abort on key change / unmount (as practical).

Place as `useAsyncAction.test.tsx` / `useResource.test.tsx` next to the hooks (or under `src/lib/`).

## Component smoke tests

Minimal proofs that RTL works:

1. `FormField` — renders label and child control (`layout` inline and stacked if cheap).
2. `Button` — click invokes handler (primary variant enough).
3. Optional: `ListRangeHeader` — shows range text when given props.

Do **not** mount full screens or React Aria-heavy dialogs in this pass unless a single smoke is trivial.

## Success criteria

- `cd web && npm test` exits 0 with new unit + hook + component tests green.
- `npm run lint` and `npm run build` still exit 0.
- `git diff` against base shows no `web-next/` changes.
- Future contributors can copy a `*.test.tsx` file and get jsdom automatically.

## Follow-ups (out of scope)

- Broader component coverage (Select, ModalShell, bubbles).
- Schema validation on `apiClient` responses.
- Coverage reporting in CI.
