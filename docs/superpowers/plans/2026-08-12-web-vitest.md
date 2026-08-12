# Web Vitest Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add Vitest to `web/`, migrate existing unit tests, and run `npm test` on every CI push/PR.

**Architecture:** Vitest shares `web/vite.config.ts`. Existing `node:test` suites move to Vitest APIs. A new always-on CI job runs `npm test` in `web/` only. `web-next/` is never modified.

**Tech Stack:** Vitest, Vite 6, Node 22, GitHub Actions.

## Global Constraints

- Scope is `web/` and `.github/workflows/ci.yml` only — **no `web-next/` changes**.
- No React Testing Library / jsdom in this pass.
- Branch: `chore/web-vitest` from `main`.

## File map

| File | Role |
|------|------|
| `web/package.json` / `package-lock.json` | Add vitest + scripts |
| `web/vite.config.ts` | Add `test` block for Vitest |
| `web/src/lib/*.test.ts` (4 files) | Migrate to Vitest |
| `.github/workflows/ci.yml` | Add `web-test` job |

---

### Task 1: Install Vitest and wire config/scripts

**Files:**
- Modify: `web/package.json`, `web/package-lock.json`, `web/vite.config.ts`

- [ ] **Step 1:** `cd web && npm install -D vitest`
- [ ] **Step 2:** Add scripts `"test": "vitest run"` and `"test:watch": "vitest"`
- [ ] **Step 3:** Update `vite.config.ts`:

```ts
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: {
    port: 5173,
    strictPort: true,
    proxy: {
      "/v1": {
        target: "http://127.0.0.1:8080",
        changeOrigin: true,
      },
    },
  },
  test: {
    environment: "node",
    include: ["src/**/*.test.ts"],
  },
});
```

- [ ] **Step 4:** Commit: `chore(web): add Vitest runner and npm test scripts`

---

### Task 2: Migrate the four unit test files

**Files:**
- Modify: `web/src/lib/assetUrl.test.ts`, `contactRecentSearches.test.ts`, `missingAttachmentLabel.test.ts`, `savedGroups.test.ts`

- [ ] **Step 1:** Replace `node:test` / `node:assert` with `import { describe, it, expect, beforeEach } from "vitest"` and `expect(...)` / `expect(...).toThrow()` equivalents. Keep cases identical. Drop `.ts` from relative imports if Vitest prefers extensionless (keep working form).
- [ ] **Step 2:** `cd web && npm test` — expect exit 0
- [ ] **Step 3:** Commit: `test(web): migrate lib unit tests to Vitest`

---

### Task 3: CI job

**Files:**
- Modify: `.github/workflows/ci.yml`

- [ ] **Step 1:** After `web-lint`, add:

```yaml
  web-test:
    name: Test (web)
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v7

      - name: Install Node.js
        uses: actions/setup-node@v4
        with:
          node-version: 22
          cache: npm
          cache-dependency-path: web/package-lock.json

      - name: Test web frontend
        run: |
          cd web
          npm ci
          npm test
```

- [ ] **Step 2:** Commit: `ci: run Vitest on web for every push and PR`

---

### Task 4: Verify

- [ ] `cd web && npm test` exits 0
- [ ] `git diff main -- web-next` is empty
