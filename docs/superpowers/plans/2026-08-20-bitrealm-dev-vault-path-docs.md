# Guides under bitrealm.io/vault Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Put the User Guide at `https://bitrealm.io/vault/user/` and Developer docs at `https://bitrealm.io/vault/developer/`, keep the company page at `https://bitrealm.io/`, and stop using `vault.bitrealm.io`.

**Architecture:** One Astro app. Starlight files move under `docs/src/content/docs/vault/`. `site` is `https://bitrealm.io`. No Astro `base: '/vault'`. No redirects. No Worker. Operator deletes the Cloudflare `vault` DNS record (not in this repo).

**Tech Stack:** Astro Starlight, GitHub Pages, Markdown/MDX.

## Global Constraints

- No HTTP redirects of any kind.
- Do not set Astro `base` to `/vault`.
- Do not add a Cloudflare Worker or Redirect Rule in the repo.
- Do not put `vault.bitrealm.io` in Pages settings or `docs/public/CNAME`.
- Do not rewrite `docs/superpowers/` historical specs and plans (except this plan file).
- Do not change `api`, `app`, or `cdn` DNS from this repo.
- Do not rewrite User Guide chapter copy except links and file moves.
- After each docs-touching task: `cd docs && npm run check && npm run build`.

## File map

| Area | Move | Modify |
|------|------|--------|
| Starlight | `user/` → `vault/user/`; `developer/` → `vault/developer/` | links inside those files |
| Chrome | — | `docs/astro.config.mjs` (`site`, topic `link`s, every sidebar slug) |
| Company page | — | `docs/src/pages/index.astro` |
| Live citations | — | README, CONTRIBUTING, CLAUDE.md, crate READMEs, rustdoc, maintainers, docker comment, ci.yml |

---

### Task 1: Move Starlight files and point the sidebar at new slugs

**Files:**
- Move: `docs/src/content/docs/user/` → `docs/src/content/docs/vault/user/`
- Move: `docs/src/content/docs/developer/` → `docs/src/content/docs/vault/developer/`
- Modify: `docs/astro.config.mjs`

**Interfaces:**
- Consumes: current slugs `user`, `user/…`, `developer`, `developer/…`
- Produces: slugs `vault/user`, `vault/user/…`, `vault/developer`, `vault/developer/…`

- [ ] **Step 1: git mv**

```bash
mkdir -p docs/src/content/docs/vault
git mv docs/src/content/docs/user docs/src/content/docs/vault/user
git mv docs/src/content/docs/developer docs/src/content/docs/vault/developer
```

- [ ] **Step 2: Update `docs/astro.config.mjs`**

Set `site: 'https://bitrealm.io'`.

User Guide `link: '/vault/user/'`. Developer `link: '/vault/developer/'`.

Prefix every sidebar slug with `vault/`:

- `{ label: 'Home', slug: 'user' }` → `{ label: 'Home', slug: 'vault/user' }`
- `'user/…'` → `'vault/user/…'`
- `'developer'` (index) → `'vault/developer'`
- `'developer/…'` → `'vault/developer/…'`

Do not set `base`.

- [ ] **Step 3: Docs check and build**

```bash
cd docs && npm run check && npm run build
test -f dist/index.html
test -f dist/vault/user/index.html
test -f dist/vault/developer/index.html
test ! -e dist/user
test ! -e dist/developer
```

Expected: all succeed. `dist/index.html` is still the company page.

- [ ] **Step 4: Commit**

```bash
git add docs/src/content/docs docs/astro.config.mjs
git commit -m "docs: move Starlight pages under /vault"
```

---

### Task 2: Rewrite in-content Markdown links

**Files:** every `.md` / `.mdx` under `docs/src/content/docs/vault/` (after the move).

**Interfaces:**
- Consumes: hrefs `/user/…` and `/developer/…`
- Produces: hrefs `/vault/user/…` and `/vault/developer/…`

- [ ] **Step 1: Replace path prefixes in Starlight content**

From the repo root, only under `docs/src/content/docs/vault/`:

```bash
rg -l '\]\(/user/|\]\(/developer/|link: /user/|link: /developer/' docs/src/content/docs/vault
```

Replace:

- `](/user/` → `](/vault/user/`
- `](/developer/` → `](/vault/developer/`
- `link: /user/` → `link: /vault/user/`
- `link: /developer/` → `link: /vault/developer/`

Including `vault/user/index.mdx` hero `link:` values and `vault/developer/index.md` (`[User Guide](/user/)` → `[User Guide](/vault/user/)`).

- [ ] **Step 2: Confirm no leftover root guidebook hrefs in content**

```bash
rg -n '\]\(/user/|\]\(/developer/|link: /user/|link: /developer/' docs/src/content/docs
```

Expected: no matches.

- [ ] **Step 3: Docs check and build**

```bash
cd docs && npm run check && npm run build
```

Expected: success.

- [ ] **Step 4: Commit**

```bash
git add docs/src/content/docs
git commit -m "docs: retarget Starlight links under /vault"
```

---

### Task 3: Company page, CONTRIBUTING, and in-repo URLs

**Files:**
- Modify: `docs/src/pages/index.astro`
- Modify: `CONTRIBUTING.md` (replace the Worker / `vault.bitrealm.io` section)
- Modify: `CLAUDE.md`, `README.md`, crate READMEs, rustdoc, `docs/maintainers/*.md`, `docker/compose.yml`, `.github/workflows/ci.yml`, `web-next/README.md`

**Interfaces:**
- Consumes: `https://vault.bitrealm.io/user/` and `/developer/`
- Produces: `https://bitrealm.io/vault/user/` and `https://bitrealm.io/vault/developer/`

- [ ] **Step 1: Company page**

In `docs/src/pages/index.astro`:

```javascript
const userGuide = "https://bitrealm.io/vault/user/";
const developer = "https://bitrealm.io/vault/developer/";
```

Keep `canonical` as `https://bitrealm.io/`.

- [ ] **Step 2: Replace `CONTRIBUTING.md` product-host section**

Delete the whole `### Set up vault.bitrealm.io` section (Worker steps included). After `### Publishing / custom domain`, add:

```markdown
Guides live at:

- User Guide: `https://bitrealm.io/vault/user/`
- Developer docs: `https://bitrealm.io/vault/developer/`

Do not add a DNS name `vault`. GitHub Pages uses one custom domain: `bitrealm.io`. After GitHub shows a valid certificate, turn on **Enforce HTTPS** in the repository Pages settings. Leave the DNS records named `api`, `app`, and `cdn` alone.
```

Also update other `https://vault.bitrealm.io` links in that file (Try the vault, Operator Docker, Converter capabilities) to the matching `https://bitrealm.io/vault/…` URLs. CLI edit path stays `docs/src/content/docs/developer/reference/cli/` until the move in Task 1; after the move it is `docs/src/content/docs/vault/developer/reference/cli/`.

- [ ] **Step 3: Rewrite remaining live `https://vault.bitrealm.io` URLs**

Replace `https://vault.bitrealm.io/user` with `https://bitrealm.io/vault/user` and `https://vault.bitrealm.io/developer` with `https://bitrealm.io/vault/developer` in live files. Exclude `docs/superpowers/`.

Also fix maintainer relative paths that still point at `src/content/docs/user` or `src/content/docs/developer` so they include `vault/`.

```bash
rg -n 'https://vault\.bitrealm\.dev' --glob '!docs/superpowers/**'
```

Expected: no matches.

- [ ] **Step 4: Docs check and build**

```bash
cd docs && npm run check && npm run build
test -f dist/index.html
test -f dist/vault/user/index.html
test -f dist/vault/developer/index.html
test ! -e dist/user
test ! -e dist/developer
! grep -q 'Your messages, your way' dist/index.html
grep -q 'Your messages, your way' dist/vault/user/index.html
grep -q 'bitrealm.io/vault/user' dist/index.html
! grep -q 'vault.bitrealm.io' dist/index.html
```

Expected: all succeed.

- [ ] **Step 5: Commit**

```bash
git add docs/src/pages/index.astro CONTRIBUTING.md CLAUDE.md README.md \
  crates docs/maintainers docker/compose.yml .github/workflows/ci.yml web-next/README.md
git commit -m "docs: publish guides at bitrealm.io/vault"
```

---

### Task 4: Operator note (no code)

Tell the operator: in Cloudflare, delete the `vault` DNS record. Leave `api`, `app`, and `cdn`. After GitHub finishes the `bitrealm.io` certificate, turn on Enforce HTTPS. Do not add a Worker.

---

## Spec coverage

| Spec requirement | Task |
|---|---|
| Company page at `/` | already there; Task 3 buttons |
| `/vault/user/` and `/vault/developer/` | Task 1 |
| `site` = `https://bitrealm.io` | Task 1 |
| No `base: '/vault'` | Task 1 |
| No redirects | all tasks |
| No Worker | Task 3 CONTRIBUTING |
| In-repo link rewrite | Tasks 2–3 |
| Skip `docs/superpowers/` | Task 3 |
| Delete `vault` DNS | Task 4 (operator) |
| `npm run check && npm run build` | Tasks 1–3 |
