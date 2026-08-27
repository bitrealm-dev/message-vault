# HTTP API route catalog next to rustdoc Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Host the vault HTTP route catalog at `/vault/developer/rustdoc/http/` using Scalar installed as a `docs/` npm package, and remove the Starlight OpenAPI pages.

**Architecture:** `dump-openapi` and `docs/src/assets/openapi.json` stay. Docs CI copies rustdoc, runs `npm ci`, then copies Scalar’s browser bundle plus a small HTML shell into `docs/public/vault/developer/rustdoc/http/`. Astro keeps sidebar and index links to crate rustdoc; the HTTP sidebar item becomes a single link to the catalog.

**Tech Stack:** GitHub Actions docs job, `@scalar/api-reference`, Astro Starlight, existing utoipa JSON dump.

## Global Constraints

- Catalog URL is `/vault/developer/rustdoc/http/` (not Starlight `/vault/developer/reference/http/`).
- Scalar comes from npm (`@scalar/api-reference`); do not load it from a CDN.
- Do not commit Scalar JavaScript; copy `dist/browser/standalone.js` after `npm ci`.
- HTML loads `./openapi.json` with a relative path; no Scalar proxy; this page is not “try it” against a live vault.
- Keep `dump-openapi` and `docs/src/assets/openapi.json`; do not change server `openapi_ui`.
- Keep Astro links to `/vault/developer/rustdoc/` (sidebar + Developer index).
- No redirect or stub at the old HTTP catalog URL.
- Copy HTTP files **after** rustdoc copy and **after** `npm ci`.
- `cd docs && npm run check && npm run build` must still work without the copy step.

---

### Task 1: HTML shell and copy script

**Files:**
- Create: `docs/src/assets/http-api-reference.html`
- Create: `docs/scripts/copy-http-api-reference.sh`

**Interfaces:**
- Consumes: `docs/node_modules/@scalar/api-reference/dist/browser/standalone.js`, `docs/src/assets/openapi.json`, `docs/src/assets/http-api-reference.html`
- Produces: `docs/public/vault/developer/rustdoc/http/{index.html,openapi.json,standalone.js}`

- [ ] **Step 1: Write the HTML shell**

Create `docs/src/assets/http-api-reference.html`:

```html
<!doctype html>
<html lang="en">
  <head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <title>Message Vault HTTP API</title>
  </head>
  <body>
    <div id="app"></div>
    <script src="./standalone.js"></script>
    <script>
      Scalar.createApiReference('#app', {
        url: './openapi.json',
        hideClientButton: true,
      });
    </script>
  </body>
</html>
```

- [ ] **Step 2: Write the copy script**

Create `docs/scripts/copy-http-api-reference.sh` (executable):

```bash
#!/usr/bin/env bash
set -euo pipefail

docs_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
src_js="${docs_root}/node_modules/@scalar/api-reference/dist/browser/standalone.js"
src_json="${docs_root}/src/assets/openapi.json"
src_html="${docs_root}/src/assets/http-api-reference.html"
dest="${docs_root}/public/vault/developer/rustdoc/http"

if [[ ! -f "${src_js}" ]]; then
  printf '%s\n' "missing ${src_js}; run npm ci in docs/" >&2
  exit 1
fi
if [[ ! -f "${src_json}" ]]; then
  printf '%s\n' "missing ${src_json}" >&2
  exit 1
fi
if [[ ! -f "${src_html}" ]]; then
  printf '%s\n' "missing ${src_html}" >&2
  exit 1
fi

mkdir -p "${dest}"
cp "${src_js}" "${dest}/standalone.js"
cp "${src_json}" "${dest}/openapi.json"
cp "${src_html}" "${dest}/index.html"

for name in index.html openapi.json standalone.js; do
  if [[ ! -f "${dest}/${name}" ]]; then
    printf '%s\n' "copy failed: ${dest}/${name}" >&2
    exit 1
  fi
done
```

- [ ] **Step 3: Verify the script fails without Scalar**

Run from repo root (do not install `@scalar/api-reference` yet, or temporarily rename `node_modules/@scalar` if already present):

```bash
bash docs/scripts/copy-http-api-reference.sh
```

Expected: exit 1, stderr mentions missing `standalone.js`.

- [ ] **Step 4: Commit**

```bash
git add docs/src/assets/http-api-reference.html docs/scripts/copy-http-api-reference.sh
git commit -m "docs: add HTTP catalog copy script and Scalar shell"
```

---

### Task 2: npm dependency swap

**Files:**
- Modify: `docs/package.json`
- Modify: `docs/package-lock.json` (via npm)

**Interfaces:**
- Produces: `@scalar/api-reference` installed; `starlight-openapi` gone

- [ ] **Step 1: Replace the packages**

From `docs/`:

```bash
npm uninstall starlight-openapi
npm install @scalar/api-reference
```

Confirm `docs/package.json` has `@scalar/api-reference` and does not have `starlight-openapi`.

- [ ] **Step 2: Verify the copy script succeeds**

```bash
bash docs/scripts/copy-http-api-reference.sh
test -f docs/public/vault/developer/rustdoc/http/index.html
test -f docs/public/vault/developer/rustdoc/http/openapi.json
test -f docs/public/vault/developer/rustdoc/http/standalone.js
```

Expected: exit 0 for each command.

- [ ] **Step 3: Commit**

```bash
git add docs/package.json docs/package-lock.json
git commit -m "docs: use Scalar npm package for HTTP catalog"
```

---

### Task 3: Remove Starlight OpenAPI and retarget sidebar

**Files:**
- Modify: `docs/astro.config.mjs`

**Interfaces:**
- Consumes: no `starlight-openapi` import
- Produces: sidebar link `/vault/developer/rustdoc/http/`; rustdoc sidebar link unchanged

- [ ] **Step 1: Edit `docs/astro.config.mjs`**

Remove:

```js
import starlightOpenAPI, { openAPISidebarGroups } from 'starlight-openapi';
```

Replace the HTTP sidebar block with:

```js
  {
    label: 'HTTP API reference',
    link: '/vault/developer/rustdoc/http/',
    attrs: { target: '_self' },
  },
```

Keep the existing “Rust crate docs” link to `/vault/developer/rustdoc/`.

Remove the `starlightOpenAPI([...])` plugin entry. Leave `starlightSidebarTopics`.

In `topics.developer`, remove `/vault/developer/reference/http` and `/vault/developer/reference/http/**/*`. Keep `/vault/developer/rustdoc` and `/vault/developer/rustdoc/**`.

- [ ] **Step 2: Verify Astro check without requiring rustdoc copy**

```bash
cd docs && npm run check
```

Expected: exit 0.

- [ ] **Step 3: Commit**

```bash
git add docs/astro.config.mjs
git commit -m "docs: drop Starlight OpenAPI plugin for HTTP catalog"
```

---

### Task 4: Update Astro copy and changelog

**Files:**
- Modify: `docs/src/content/docs/vault/developer/index.md`
- Modify: `docs/src/content/docs/vault/developer/reference/api.md`
- Modify: `CHANGELOG.md`

**Interfaces:**
- Produces: in-repo links to `/vault/developer/rustdoc/http/`; crate rustdoc links kept

- [ ] **Step 1: Update Developer index**

HTTP line:

```markdown
- [HTTP API](/vault/developer/reference/api/) — tokens and import flow; [route reference](/vault/developer/rustdoc/http/)
- [Rust crate docs](/vault/developer/rustdoc/) — `cargo doc` HTML for workspace crates (not the HTTP route list)
```

Keep the rustdoc bullet. Do not delete it.

- [ ] **Step 2: Update `api.md` first paragraph**

```markdown
Route schemas, status codes, and JSON fields live in the generated [HTTP API reference](/vault/developer/rustdoc/http/). Crate types and functions live in [Rust crate docs](/vault/developer/rustdoc/). This page is the prose those tools need that is not a JSON schema.
```

- [ ] **Step 3: Update `CHANGELOG.md` `[Unreleased]`**

Change the existing “Generated OpenAPI reference… on the docs site” added line so it does not claim Starlight pages. Use:

```markdown
- Generated HTTP API route catalog at `/vault/developer/rustdoc/http/`, plus an optional explorer at `/docs` when `[server] openapi_ui` is true
```

- [ ] **Step 4: Grep for leftover catalog URLs in product docs**

```bash
rg 'reference/http' docs --glob '!docs/superpowers/**'
```

Expected: no matches in live docs content (specs under `docs/superpowers/` may still mention the old URL as history). Fix any remaining live links.

- [ ] **Step 5: Commit**

```bash
git add docs/src/content/docs/vault/developer/index.md docs/src/content/docs/vault/developer/reference/api.md CHANGELOG.md
git commit -m "docs: point HTTP catalog links at rustdoc"
```

---

### Task 5: Docs CI copy step

**Files:**
- Modify: `.github/workflows/docs.yml`

**Interfaces:**
- Consumes: `docs/scripts/copy-http-api-reference.sh` after `npm ci`
- Produces: `docs/public/vault/developer/rustdoc/http/` in the Pages artifact

- [ ] **Step 1: Insert copy step after Install dependencies**

After the `npm ci` step, before Check:

```yaml
      - name: Copy HTTP API catalog into rustdoc
        run: bash docs/scripts/copy-http-api-reference.sh
```

Do not run this script from `npm run build`. Local check/build without copy must still work.

- [ ] **Step 2: Commit**

```bash
git add .github/workflows/docs.yml
git commit -m "ci(docs): copy HTTP catalog next to rustdoc"
```

---

### Task 6: Verify docs build

**Files:** none new

- [ ] **Step 1: Astro check and build without claiming rustdoc/http exists**

```bash
cd docs && npm run check && npm run build
```

Expected: exit 0.

- [ ] **Step 2: Confirm Starlight no longer emits the old catalog**

```bash
test ! -e docs/dist/vault/developer/reference/http
```

Expected: exit 0 (path absent).

- [ ] **Step 3: Copy then rebuild and confirm catalog files**

```bash
bash docs/scripts/copy-http-api-reference.sh
cd docs && npm run build
test -f docs/dist/vault/developer/rustdoc/http/index.html
test -f docs/dist/vault/developer/rustdoc/http/openapi.json
test -f docs/dist/vault/developer/rustdoc/http/standalone.js
```

Expected: exit 0. (If rustdoc crate HTML is missing locally, only the `http/` subtree needs to exist under `public/` / `dist/`.)

- [ ] **Step 4: Confirm `starlight-openapi` is gone**

```bash
rg starlight-openapi docs/package.json
```

Expected: no matches.
