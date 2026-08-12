# Web ESLint Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add ESLint 9 for TypeScript/React under `web/` and run `npm run lint` on every CI push/PR to `main`.

**Architecture:** Flat ESLint config lives only in `web/`. Recommended (non-type-aware) `typescript-eslint` plus React hooks/refresh plugins. A new always-on CI job installs and lints `web/` only. `web-next/` is untouched.

**Tech Stack:** ESLint 9, `@eslint/js`, `typescript-eslint`, `eslint-plugin-react-hooks`, `eslint-plugin-react-refresh`, GitHub Actions, Node 22.

## Global Constraints

- Scope is `web/` and `.github/workflows/ci.yml` only — no `web-next/` changes.
- Use recommended TypeScript rules, not type-checked / `projectService` rules.
- Prefer fixing lint errors over broad rule disables.
- Work on branch `chore/web-eslint` from `main`; do not include unrelated WIP.

## File map

| File | Role |
|------|------|
| `web/package.json` | Add lint script and ESLint-related `devDependencies` |
| `web/package-lock.json` | Lock exact versions from `npm install` |
| `web/eslint.config.js` | Flat config for `*.{ts,tsx}` |
| `.github/workflows/ci.yml` | New `web-lint` job |
| `web/src/**` | Only if first lint run reports fixable errors |

---

### Task 1: Install ESLint and add flat config

**Files:**
- Create: `web/eslint.config.js`
- Modify: `web/package.json`
- Modify: `web/package-lock.json` (via npm)

**Interfaces:**
- Consumes: existing Vite + React 19 + TypeScript setup in `web/`
- Produces: `npm run lint` → runs `eslint .` from `web/`

- [ ] **Step 1: Install packages**

```bash
cd web
npm install -D eslint @eslint/js typescript-eslint eslint-plugin-react-hooks eslint-plugin-react-refresh
```

Expected: `package.json` / `package-lock.json` updated; no Next ESLint packages.

- [ ] **Step 2: Add lint script**

In `web/package.json` `scripts`:

```json
"lint": "eslint ."
```

Keep existing `dev`, `build`, and `preview` scripts.

- [ ] **Step 3: Write `web/eslint.config.js`**

```js
import js from "@eslint/js";
import reactHooks from "eslint-plugin-react-hooks";
import reactRefresh from "eslint-plugin-react-refresh";
import tseslint from "typescript-eslint";

export default tseslint.config(
  { ignores: ["dist"] },
  {
    extends: [js.configs.recommended, ...tseslint.configs.recommended],
    files: ["**/*.{ts,tsx}"],
    plugins: {
      "react-hooks": reactHooks,
      "react-refresh": reactRefresh,
    },
    rules: {
      ...reactHooks.configs.recommended.rules,
      "react-refresh/only-export-components": [
        "warn",
        { allowConstantExport: true },
      ],
    },
  },
);
```

- [ ] **Step 4: Run lint**

```bash
cd web && npm run lint
```

Expected: command runs. If exit non-zero, proceed to Task 2. If exit 0, skip Task 2 fixes.

- [ ] **Step 5: Commit**

```bash
git add web/package.json web/package-lock.json web/eslint.config.js
git commit -m "$(cat <<'EOF'
chore(web): add ESLint for TypeScript and React

EOF
)"
```

---

### Task 2: Fix or narrowly disable first-pass violations

**Files:**
- Modify: `web/src/**` as needed for lint errors from Task 1

**Interfaces:**
- Consumes: ESLint output from `npm run lint`
- Produces: `npm run lint` exits 0

- [ ] **Step 1: Capture errors**

```bash
cd web && npm run lint
```

- [ ] **Step 2: Fix each error**

Prefer real fixes (unused vars, hooks deps, etc.). Use a single-line or file-local disable only when a correct fix would be large or wrong for the pattern.

- [ ] **Step 3: Re-run until clean**

```bash
cd web && npm run lint
```

Expected: exit 0.

- [ ] **Step 4: Commit if any source changes**

```bash
git add web/src
git commit -m "$(cat <<'EOF'
fix(web): resolve ESLint findings for clean lint run

EOF
)"
```

Skip this commit if there were no source changes.

---

### Task 3: Add always-on CI lint job

**Files:**
- Modify: `.github/workflows/ci.yml`

**Interfaces:**
- Consumes: `web/package-lock.json`, `npm run lint`
- Produces: `web-lint` job on push/PR to `main` and `workflow_dispatch`

- [ ] **Step 1: Insert job after the `test` job (before tag-only jobs)**

```yaml
  # ── Always: web ESLint ────────────────────────────────────────────────
  web-lint:
    name: Lint (web)
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v7

      - name: Install Node.js
        uses: actions/setup-node@v4
        with:
          node-version: 22
          cache: npm
          cache-dependency-path: web/package-lock.json

      - name: Lint web frontend
        run: |
          cd web
          npm ci
          npm run lint
```

Do not add steps that touch `web-next/`. Do not change the tag-only Tauri web build block.

- [ ] **Step 2: Sanity-check YAML structure**

Confirm `web-lint` is a sibling of `fmt` / `test` (same indentation as those jobs).

- [ ] **Step 3: Commit**

```bash
git add .github/workflows/ci.yml
git commit -m "$(cat <<'EOF'
ci: run ESLint on web for every push and PR

EOF
)"
```

---

### Task 4: Final verification

- [ ] **Step 1: Confirm lint passes**

```bash
cd web && npm run lint
```

Expected: exit 0.

- [ ] **Step 2: Confirm web-next untouched**

```bash
git diff main -- web-next
```

Expected: empty output.

- [ ] **Step 3: Confirm branch commits**

```bash
git log --oneline main..HEAD
```

Expected: design doc + eslint + optional fixes + ci commits only.
