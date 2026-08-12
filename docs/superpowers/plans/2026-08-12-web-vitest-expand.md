# Expand Web Vitest Coverage Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans or subagent-driven-development.

**Goal:** Add unit tests for cleanup helpers plus jsdom/Testing Library starter kit for hooks and small components.

**Architecture:** Node env for `*.test.ts`; jsdom for `*.test.tsx` via `environmentMatchGlobs`. Mock `apiClient` for fetchConversationById.

**Tech Stack:** Vitest 4, jsdom, Testing Library React/jest-dom/user-event.

**Spec:** `docs/superpowers/specs/2026-08-12-web-vitest-expand-design.md`

## Global Constraints

- `web/` only — never `web-next/`
- Branch: `test/web-vitest-expand`
- `npm run lint && npm test && npm run build` green

---

### Task 1: Install RTL + configure Vitest
### Task 2: Unit tests (fetchConversationById, handleService, contactInitials, nameAliases, exportSources, portaledOverlay)
### Task 3: Hook tests (useAsyncAction, useResource) + component smokes (FormField, Button, ListRangeHeader)
### Task 4: Verify and commit
