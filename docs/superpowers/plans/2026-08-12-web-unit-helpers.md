# Pure Web Unit Helpers Implementation Plan

> **For agentic workers:** Use executing-plans or implement directly.

**Goal:** Add Vitest unit tests for theme, formatVisibleRange, chat bubble helpers, storageUtils, contactDetailCache.

**Spec:** `docs/superpowers/specs/2026-08-12-web-unit-helpers-design.md`

**Constraints:** `web/` only; skip system-settings (storage-heavy); no new RTL screens.

### Tasks
1. Write `theme.test.ts`, `usePagedList` range test (or `formatVisibleRange.test.ts` colocated), `chatBubbleShared.test.ts`, `storageUtils.test.ts`, `contactDetailCache.test.ts`
2. `cd web && npm test && npm run lint && npm run build`
3. Commit
