# More unit tests for pure web helpers

**Date:** 2026-08-12  
**Status:** approved  
**Scope:** `web/` unit tests only. **No changes to `web-next/`.**

## Problem

Vitest already covers many cleanup helpers (78 tests). Several remaining modules are still pure (or nearly pure) and match a server-side unit-testing style — edge cases, no React tree — but have no tests. Skipping them leaves regressions easy in theme sharing, list range labels, bubble metadata, storage formatting, and the contact detail cache.

## Goals

1. Add unit tests for high-value pure helpers that are still untested.
2. Keep the same Vitest setup (Node by default; jsdom only if a helper needs `document`/`window`).
3. Stay backend-friendly: no new screen/component RTL in this pass.
4. Leave CI on `npm test` unchanged.
5. Never touch `web-next/`.

## Non-goals

- More FormField/Button/ListRangeHeader component tests.
- Full `usePagedList` / `useAccountProfile` / Import/MessageView tests.
- Coverage percentage gates or snapshot farms.
- Tauri invoke integration tests.

## Approach

Brainstorm option **B**:

| Area | What to test |
|------|----------------|
| `lib/theme.ts` | `normalizeHex`, `formatThemeShare`, `parseThemeShare`, `resolveMode` |
| `lib/usePagedList.ts` | `formatVisibleRange` only (not the hook) |
| `components/messages/chatBubbleShared.tsx` | `formatMessageTime`, `senderName`, `isGroupConversation`, `bubbleBody` |
| `screens/settings/storage/storageUtils.ts` | `formatBytes`, `formatImportDate`, `toImportSummaryView` |
| `lib/contactDetailCache.ts` | `getCachedContactDetail` / `invalidateContactDetail` / `clearContactDetailCache` after seeding via public fetch path **or** testing cache ops if a test-only seed is unnecessary — prefer mocking `apiClient` once to populate cache through `fetchContactDetail`, then invalidate/clear |
| `lib/system-settings.ts` | Only if a pure helper is still exported and easy; skip storage-heavy async home-dir paths |

## Constraints

- Branch: `test/web-unit-helpers` from current `main`.
- Prefer `*.test.ts` next to sources.
- Mock `apiClient` / `localStorage` at module boundaries; do not hit a real vault.
- Do not export new APIs solely for tests unless unavoidable; prefer exercising existing public functions.

## Success criteria

- New tests green under `cd web && npm test`.
- `npm run lint` and `npm run build` still exit 0.
- No `web-next/` diff.
- Each new file targets real edge cases (empty input, invalid hex/share string, zero bytes, unknown import status, cache miss after invalidate).

## Follow-ups (out of scope)

- Hook tests for `usePagedList` pagination behavior.
- ThemeProvider / applyTheme DOM integration.
- Broader bubble component RTL.
