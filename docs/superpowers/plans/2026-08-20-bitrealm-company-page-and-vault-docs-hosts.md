# Bitrealm Company Page and Vault Docs Hosts Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make `https://bitrealm.io/` a Bitrealm company chooser and move every guidebook page under `/user/…` or `/developer/…` for the `vault.bitrealm.io` product host.

**Architecture:** One Astro app. `src/pages/index.astro` owns `/`. Starlight content moves to `docs/src/content/docs/user/` and `docs/src/content/docs/developer/`. `site` becomes `https://vault.bitrealm.io`. In-repo guidebook URLs are rewritten. Cloudflare CNAME plus a root-only redirect are operator steps documented in `CONTRIBUTING.md`, not automated.

**Tech Stack:** Astro 7, Starlight 0.41, `starlight-sidebar-topics` 0.8.

**Spec:** `docs/superpowers/specs/2026-08-20-bitrealm-company-page-and-vault-docs-hosts-design.md`

## Global Constraints

- No HTTP redirects for today’s apex guidebook paths (`/get-started/…`, `/how-to/…`, `/formats/…`, `/reference/…`).
- Do not put `vault.bitrealm.io` in `docs/public/CNAME`. That file stays `bitrealm.io`.
- Do not change `api`, `app`, or R2 DNS from this repo.
- Do not rewrite `docs/superpowers/` historical specs and plans.
- Do not rewrite User Guide chapter copy except links and the splash move.
- Do not add Try/Download CTAs, an About page, or a second product slot on `/`.
- Do not change runtime, exporter, desktop app, or vault-server code except rustdoc URL strings.
- Call it “the vault” and “the desktop app” on User Guide pages.
- After each docs-touching task: `cd docs && npm run check && npm run build`.

## File map

| Area | Create | Move (git mv) | Modify |
|------|--------|---------------|--------|
| Company page | `docs/src/pages/index.astro` | — | `docs/src/styles/custom.css` |
| User Guide | — | `index.mdx` → `user/index.mdx`; `get-started/`, `prepare-a-backup/`, `how-to/`, `import-from-a-backup.md`, `browse-your-messages.md`, `glossary.md` → under `user/` | links inside those files |
| Developer | `developer/index.md` | `formats/` → `developer/formats/`; `reference/` → `developer/reference/` | `developer/run-from-source.md`, `developer/docker-compose.md` links |
| Chrome | — | — | `docs/astro.config.mjs` (`site`, topic `link`s, every sidebar slug) |
| Live citations | — | — | README, CONTRIBUTING, CLAUDE.md, crate READMEs, rustdoc, `docs/maintainers/*.md`, `docker/compose.yml`, `.github/workflows/ci.yml` |

---

### Task 1: Relocate Starlight files and point the sidebar at new slugs

**Files:**
- Move: every User Guide file listed below into `docs/src/content/docs/user/`
- Move: `docs/src/content/docs/formats/` → `docs/src/content/docs/developer/formats/`
- Move: `docs/src/content/docs/reference/` → `docs/src/content/docs/developer/reference/`
- Create: `docs/src/content/docs/developer/index.md`
- Modify: `docs/astro.config.mjs`

**Interfaces:**
- Consumes: current sidebar slugs in `docs/astro.config.mjs`
- Produces: Starlight slugs `user`, `user/get-started/…`, `user/prepare-a-backup/…`, `user/import-from-a-backup`, `user/browse-your-messages`, `user/how-to/…`, `user/glossary`, `developer`, `developer/run-from-source`, `developer/docker-compose`, `developer/reference/…`, `developer/formats/…`

- [ ] **Step 1: Confirm `docs/` dependencies are installed**

Run:

```bash
cd docs && npm ci
```

Expected: lockfile install succeeds.

- [ ] **Step 2: Move User Guide files with git**

Run from the repo root:

```bash
mkdir -p docs/src/content/docs/user
git mv docs/src/content/docs/index.mdx docs/src/content/docs/user/index.mdx
git mv docs/src/content/docs/get-started docs/src/content/docs/user/get-started
git mv docs/src/content/docs/prepare-a-backup docs/src/content/docs/user/prepare-a-backup
git mv docs/src/content/docs/how-to docs/src/content/docs/user/how-to
git mv docs/src/content/docs/import-from-a-backup.md docs/src/content/docs/user/import-from-a-backup.md
git mv docs/src/content/docs/browse-your-messages.md docs/src/content/docs/user/browse-your-messages.md
git mv docs/src/content/docs/glossary.md docs/src/content/docs/user/glossary.md
```

Expected: `git status` shows those paths as renamed.

- [ ] **Step 3: Move formats and reference under developer**

```bash
git mv docs/src/content/docs/formats docs/src/content/docs/developer/formats
git mv docs/src/content/docs/reference docs/src/content/docs/developer/reference
```

Leave `docs/src/content/docs/developer/run-from-source.md` and `docker-compose.md` in place.

- [ ] **Step 4: Write `docs/src/content/docs/developer/index.md`**

```markdown
---
title: Developer
description: Run Message Vault from source, then CLI tools, the HTTP API, formats, and instance internals.
---

These pages are for people who compile the vault, run Compose, or call the HTTP API. The [User Guide](/user/) is the try-it and import path.

- [Run from source](/developer/run-from-source/) — clone, `cargo run`, `cargo tauri dev`
- [Operator Docker](/developer/docker-compose/) — release-shaped Compose from a checkout
- [Command-line tools](/developer/reference/cli/) — exporter binaries, `vault-push`, `vault-pull`
- [HTTP API](/developer/reference/api/)
- [Formats](/developer/formats/) — converter capabilities and mapping tables
- [Config and accounts](/developer/reference/config-and-accounts/) — `config.toml` and local accounts
- [Database](/developer/reference/database/)
- [Export structure](/developer/reference/export-structure/) — JSONL folder layout
- [CSV columns](/developer/reference/csv-columns/)
- [Server CLI](/developer/reference/server-cli/)
```

- [ ] **Step 5: Replace `userGuideItems` and `developerItems` and topic links in `docs/astro.config.mjs`**

Set `site` to `'https://vault.bitrealm.io'`.

User Guide topic: `link: '/user/'`. Developer topic: `link: '/developer/'`.

Replace the two arrays with:

```javascript
const userGuideItems = [
  { label: 'Home', slug: 'user' },
  {
    label: 'Get started',
    items: [
      'user/get-started/what-is-message-vault',
      'user/get-started/why-you-provide-backups',
      'user/get-started/try-the-vault',
      'user/get-started/your-own-messages',
      'user/get-started/install-the-desktop-app',
    ],
  },
  {
    label: 'Prepare a backup',
    items: [
      'user/prepare-a-backup',
      'user/prepare-a-backup/iphone-ipad',
      'user/prepare-a-backup/iphone-whatsapp',
      'user/prepare-a-backup/android-sms',
      'user/prepare-a-backup/android-whatsapp',
    ],
  },
  'user/import-from-a-backup',
  'user/browse-your-messages',
  {
    label: 'How do I…',
    items: [
      'user/how-to/search',
      'user/how-to/contacts-and-labels',
      'user/how-to/saved-searches',
      'user/how-to/trash',
      'user/how-to/settings',
      'user/how-to/convert-formats',
      'user/how-to/extract-to-files',
      'user/how-to/export-from-the-vault',
      'user/how-to/media-and-privacy',
      { slug: 'user/how-to/rescue-imports', badge: limitedBadge },
      'user/how-to/update',
      'user/how-to/troubleshooting',
    ],
  },
  'user/glossary',
];

const developerItems = [
  'developer',
  'developer/run-from-source',
  'developer/docker-compose',
  {
    label: 'CLI tools',
    items: [
      'developer/reference/cli',
      'developer/reference/cli/imessage-ir-exporter',
      'developer/reference/cli/sms-backup-restore-exporter',
      'developer/reference/cli/whatsapp-exporter',
      'developer/reference/cli/message-reexporter',
      'developer/reference/cli/vault-push',
      'developer/reference/cli/vault-pull',
      'developer/reference/cli/go-sms-pro-exporter',
      'developer/reference/cli/imazing-exporter',
      'developer/reference/cli/openextract-exporter',
      'developer/reference/cli/sms-backup-plus-exporter',
    ],
  },
  'developer/reference/api',
  {
    label: 'Formats',
    items: [
      'developer/formats',
      'developer/formats/mail-archive',
      'developer/formats/sms-backup-restore-xml',
      'developer/formats/convert',
      {
        label: 'SMS Backup & Restore',
        items: [
          'developer/formats/sms-backup-restore/input',
          'developer/formats/sms-backup-restore/mapping',
        ],
      },
      {
        label: 'SMS Backup+',
        items: [
          'developer/formats/sms-backup-plus/format',
          'developer/formats/sms-backup-plus/mapping',
        ],
      },
      {
        label: 'GO SMS Pro',
        items: ['developer/formats/go-sms-pro/mapping'],
      },
      {
        label: 'iMazing',
        items: [
          'developer/formats/imazing/input',
          'developer/formats/imazing/design',
        ],
      },
    ],
  },
  {
    label: 'Instance internals',
    collapsed: true,
    items: [
      'developer/reference/config-and-accounts',
      'developer/reference/database',
      'developer/reference/export-structure',
      'developer/reference/csv-columns',
      'developer/reference/server-cli',
    ],
  },
];
```

- [ ] **Step 6: Run check (expect link failures until Task 2)**

Run:

```bash
cd docs && npm run check
```

Expected: Starlight reports missing links that still use `/get-started/`, `/how-to/`, `/formats/`, `/reference/` without the new prefix. If check unexpectedly passes, grep those prefixes under `docs/src/content/docs/` and fix any silent misses in Task 2 anyway.

- [ ] **Step 7: Commit**

```bash
git add docs/src/content/docs docs/astro.config.mjs
git commit -m "$(cat <<'EOF'
docs: move Starlight pages under /user and /developer

Sidebar slugs and file paths must match before guidebook links and the company page can land on the new URLs.
EOF
)"
```

---

### Task 2: Rewrite in-content Markdown links

**Files:**
- Modify: every `.md` / `.mdx` under `docs/src/content/docs/` that contains `](/get-started/`, `](/how-to/`, `](/prepare-a-backup`, `](/import-from-a-backup`, `](/browse-your-messages`, `](/glossary`, `](/formats/`, `](/reference/`, or `https://bitrealm.io/reference/` / `https://bitrealm.io/formats/`

**Interfaces:**
- Consumes: file tree from Task 1
- Produces: only `/user/…` and `/developer/…` (plus unchanged `/developer/run-from-source/` and `/developer/docker-compose/`) as in-site guidebook hrefs

- [ ] **Step 1: Apply replacements in `docs/src/content/docs` only, longest prefixes first**

Run from the repo root (Python so `/formats/` does not double-prefix):

```bash
python3 <<'PY'
from pathlib import Path

root = Path("docs/src/content/docs")
subs = [
    ("https://bitrealm.io/reference/", "https://vault.bitrealm.io/developer/reference/"),
    ("https://bitrealm.io/formats/", "https://vault.bitrealm.io/developer/formats/"),
    ("](/formats/", "](/developer/formats/"),
    ("](/reference/", "](/developer/reference/"),
    ("](/get-started/", "](/user/get-started/"),
    ("](/how-to/", "](/user/how-to/"),
    ("](/prepare-a-backup", "](/user/prepare-a-backup"),
    ("](/import-from-a-backup", "](/user/import-from-a-backup"),
    ("](/browse-your-messages", "](/user/browse-your-messages"),
    ("](/glossary", "](/user/glossary"),
]
# Splash hero frontmatter uses unbracketed paths:
subs_plain = [
    ("link: /get-started/", "link: /user/get-started/"),
]
for path in list(root.rglob("*.md")) + list(root.rglob("*.mdx")):
    text = path.read_text()
    orig = text
    for a, b in subs + subs_plain:
        text = text.replace(a, b)
    if text != orig:
        path.write_text(text)
        print(path)
PY
```

Do not replace `/developer/run-from-source` or `/developer/docker-compose` (already correct).

- [ ] **Step 2: Grep for leftover root guidebook paths**

```bash
rg -n '\]\(/((get-started|how-to|formats|reference|prepare-a-backup|import-from-a-backup|browse-your-messages|glossary)(/|\)))' docs/src/content/docs
```

Expected: no matches. (`/developer/` links are allowed.)

- [ ] **Step 3: Run check and build**

```bash
cd docs && npm run check && npm run build
```

Expected: both succeed. `docs/dist/user/index.html` exists. `docs/dist/developer/index.html` exists. `docs/dist/formats/` and `docs/dist/get-started/` do not exist (aside from nothing).

- [ ] **Step 4: Commit**

```bash
git add docs/src/content/docs
git commit -m "$(cat <<'EOF'
docs: retarget Starlight in-page links to /user and /developer

Moved pages 404 at the old root paths; in-content hrefs have to match the new slugs or the guidebook is a dead graph.
EOF
)"
```

---

### Task 3: Company page at `/`

**Files:**
- Create: `docs/src/pages/index.astro`
- Modify: `docs/src/styles/custom.css` (append company-page rules only)

**Interfaces:**
- Consumes: `--sl-color-*` variables already in `custom.css`; splash tagline from the old `index.mdx` hero
- Produces: `/` HTML with canonical `https://bitrealm.io/` and three absolute docs links on `https://vault.bitrealm.io`

- [ ] **Step 1: Write `docs/src/pages/index.astro`**

```astro
---
const userGuide = "https://vault.bitrealm.io/user/";
const developer = "https://vault.bitrealm.io/developer/";
const github = "https://github.com/bitrealm-io/message-vault";
---

<html lang="en">
  <head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <title>Bitrealm</title>
    <meta
      name="description"
      content="Local software you run. Not a cloud account."
    />
    <link rel="canonical" href="https://bitrealm.io/" />
    <link rel="stylesheet" href="/src/styles/custom.css" />
  </head>
  <body class="bitrealm-home">
    <header class="bitrealm-header">
      <a class="bitrealm-mark" href="/">Bitrealm</a>
      <nav>
        <a href={userGuide}>User Guide</a>
        <a href={developer}>Developer</a>
        <a href={github}>GitHub</a>
      </nav>
    </header>
    <main class="bitrealm-main">
      <h1>Bitrealm</h1>
      <p class="bitrealm-lede">
        Local software you run on a machine you control. Not a cloud account.
      </p>
      <article class="bitrealm-card">
        <h2>Message Vault</h2>
        <p>
          Extract messages from Apple and Android phone backups. Import them
          into a local SQLite vault. Browse, search, and export in formats you
          can keep.
        </p>
        <p class="bitrealm-card-links">
          <a href={userGuide}>User Guide</a>
          <a href={developer}>Developer</a>
          <a href={github}>GitHub</a>
        </p>
      </article>
    </main>
  </body>
</html>
```

Starlight’s Vite pipeline may not resolve `/src/styles/custom.css` on a raw `pages/` file. If `npm run build` 404s that stylesheet, switch the page to:

```astro
---
import "../styles/custom.css";
---
```

and drop the `<link rel="stylesheet">`. Prefer the import if the link fails.

- [ ] **Step 2: Append company-page CSS to `docs/src/styles/custom.css`**

```css
.bitrealm-home {
  margin: 0;
  min-height: 100vh;
  font-family: var(--sl-font);
  background: var(--sl-color-black);
  color: var(--sl-color-white);
}

.bitrealm-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 1.25rem 1.75rem;
}

.bitrealm-header nav {
  display: flex;
  gap: 1.25rem;
}

.bitrealm-header a,
.bitrealm-card-links a {
  color: var(--sl-color-text-accent);
  text-decoration: none;
  font-weight: 550;
}

.bitrealm-mark {
  color: var(--sl-color-white);
  font-weight: 650;
  letter-spacing: -0.03em;
}

.bitrealm-main {
  max-width: 40rem;
  margin: 4rem auto;
  padding: 0 1.5rem;
}

.bitrealm-lede {
  color: var(--sl-color-gray-2);
  max-width: 36ch;
}

.bitrealm-card {
  margin-top: 2.5rem;
  padding: 1.5rem 1.5rem 1.25rem;
  border: 1px solid var(--sl-color-gray-5);
  border-top: 3px solid var(--sl-color-accent);
  border-radius: 0.5rem;
}

.bitrealm-card-links {
  display: flex;
  gap: 1.25rem;
}
```

- [ ] **Step 3: Confirm `/` is the company page, not Starlight**

```bash
cd docs && npm run check && npm run build
```

Expected: `docs/dist/index.html` contains `Bitrealm` and `vault.bitrealm.io/user`. It does not contain `Your messages, your way`. `docs/dist/user/index.html` contains `Your messages, your way`.

- [ ] **Step 4: Commit**

```bash
git add docs/src/pages/index.astro docs/src/styles/custom.css
git commit -m "$(cat <<'EOF'
docs: add Bitrealm company page at /

The apex must be a chooser, not the Message Vault splash, so User Guide and Developer are explicit clicks.
EOF
)"
```

---

### Task 4: Rewrite live in-repo guidebook URLs and document DNS

**Files:**
- Modify: `README.md` (User Guide / Developer / Explore the docs links)
- Modify: `CONTRIBUTING.md` (Try the vault, Operator Docker, formats, publishing section)
- Modify: `CLAUDE.md` (formats URL; docs-site sentence)
- Modify: crate READMEs and rustdoc listed below
- Modify: `docs/maintainers/README.md`, `docs/maintainers/architecture/message-ir.md`, `docs/maintainers/developing.md`, `docs/maintainers/gui.md`
- Modify: `docker/compose.yml` comment
- Modify: `.github/workflows/ci.yml` release-notes docs line
- Modify: `web-next/README.md`

**Do not modify:** `docs/superpowers/**`

**Interfaces:**
- Consumes: URL map in the spec
- Produces: no remaining `https://bitrealm.io/get-started/`, `/how-to/`, `/formats/`, or `/reference/` strings outside `docs/superpowers/`

Exact replacements (apply with search-and-replace per file, or one Python walk that skips `docs/superpowers`):

| Find | Replace |
|------|---------|
| `https://bitrealm.io/get-started/` | `https://vault.bitrealm.io/user/get-started/` |
| `https://bitrealm.io/how-to/` | `https://vault.bitrealm.io/user/how-to/` |
| `https://bitrealm.io/browse-your-messages/` | `https://vault.bitrealm.io/user/browse-your-messages/` |
| `https://bitrealm.io/formats/` | `https://vault.bitrealm.io/developer/formats/` |
| `https://bitrealm.io/reference/` | `https://vault.bitrealm.io/developer/reference/` |
| `https://bitrealm.io/developer/docker-compose/` | `https://vault.bitrealm.io/developer/docker-compose/` |
| `https://bitrealm.io/developer/run-from-source/` | `https://vault.bitrealm.io/developer/run-from-source/` |
| `https://bitrealm.io/developer` (README, no trailing path) | `https://vault.bitrealm.io/developer/` |

`README.md` “Explore the docs” may stay `https://bitrealm.io/` (chooser) or become `https://vault.bitrealm.io/user/`. Use `https://vault.bitrealm.io/user/` so that button opens the guidebook.

`docs/maintainers/developing.md` line `https://bitrealm.io/vault/` becomes `https://vault.bitrealm.io/user/`.

**CONTRIBUTING.md publishing section** — keep the existing apex/`CNAME` paragraph, then append:

```markdown
### Product hostname (`vault.bitrealm.io`)

The same GitHub Pages deploy answers on `vault.bitrealm.io` through Cloudflare.
Pages still has one custom domain: `bitrealm.io` (`docs/public/CNAME`). Do not
put `vault.bitrealm.io` in that file.

1. Cloudflare: CNAME `vault` → `bitrealm.io`, proxied (orange cloud). Leave
   `api`, `app`, and R2 alone. A grey-cloud CNAME to `*.github.io` will not
   work — Pages does not bind that hostname.
2. Cloudflare Redirect Rule: `https://vault.bitrealm.io/` →
   `https://bitrealm.io/`. Match the hostname root only. Do not match
   `/user` or `/developer`.
```

Files that currently contain the old host paths (update each):

- `README.md`
- `CONTRIBUTING.md`
- `CLAUDE.md`
- `docker/compose.yml`
- `.github/workflows/ci.yml`
- `web-next/README.md`
- `docs/maintainers/README.md`
- `docs/maintainers/architecture/message-ir.md`
- `docs/maintainers/developing.md`
- `docs/maintainers/gui.md`
- `crates/vault/server/README.md`
- `crates/cli/vault-push/README.md`
- `crates/cli/vault-pull/README.md`
- `crates/exporters/imessage-ir-exporter/README.md`
- `crates/exporters/whatsapp-exporter/README.md`
- `crates/exporters/sms-backup-restore-exporter/README.md`
- `crates/exporters/go-sms-pro-exporter/README.md`
- `crates/exporters/imazing-exporter/README.md`
- `crates/exporters/openextract-exporter/README.md`
- `crates/exporters/sms-backup-plus-exporter/README.md`
- `crates/libs/mail/README.md`
- `crates/libs/mail/src/lib.rs`
- `crates/libs/sbr/README.md`
- `crates/libs/sbr/src/lib.rs`
- `crates/libs/sbr/src/read.rs`
- `crates/libs/go-sms-mms/README.md`
- `crates/libs/go-sms-mms/src/mms_enc.rs`
- `crates/libs/ir/README.md`
- `crates/libs/ir-format/README.md`
- `crates/libs/reexport/README.md`
- `crates/libs/csv/README.md`
- `crates/libs/contacts/README.md`
- `crates/libs/media/README.md`
- `crates/libs/obfuscate/README.md`
- `crates/core/message-vault-io-core/src/config.rs`

- [ ] **Step 1: Apply the table to those files (skip `docs/superpowers/`)**

- [ ] **Step 2: Fail if old guidebook hosts remain**

```bash
rg -n 'https://bitrealm\.dev/(get-started|how-to|formats|reference|browse-your-messages|developer)' \
  --glob '!docs/superpowers/**'
```

Expected: no matches. (`https://bitrealm.io/` alone is allowed.)

- [ ] **Step 3: Commit**

```bash
git add README.md CONTRIBUTING.md CLAUDE.md docker/compose.yml .github/workflows/ci.yml web-next/README.md docs/maintainers crates
git commit -m "$(cat <<'EOF'
docs: point in-repo guidebook URLs at vault.bitrealm.io

Old apex paths will 404; crate READMEs and rustdoc have to cite /user and /developer on the product host.
EOF
)"
```

---

### Task 5: Final verification

**Files:** none new.

- [ ] **Step 1: Docs check and build**

```bash
cd docs && npm run check && npm run build
test -f dist/index.html
test -f dist/user/index.html
test -f dist/developer/index.html
test ! -e dist/get-started
test ! -e dist/formats
! grep -q 'Your messages, your way' dist/index.html
grep -q 'Your messages, your way' dist/user/index.html
```

Expected: all commands succeed.

- [ ] **Step 2: Repeat the leftover-URL grep from Task 4 Step 2**

Expected: no matches.

- [ ] **Step 3: Tell the operator the two Cloudflare steps are still manual**

Do not run Cloudflare from this repo. After merge and Pages deploy, the human adds the `vault` CNAME and the root Redirect Rule as written in `CONTRIBUTING.md`.

---

## Spec coverage

| Spec requirement | Task |
|---|---|
| Custom `/` company page | Task 3 |
| Delete Starlight ownership of `/` | Task 1 (`git mv` splash) |
| `/user/` is today’s splash | Task 1 + Task 2 (hero links) |
| `/developer/` short index | Task 1 Step 4 |
| Prefix every guidebook page | Task 1 |
| `site` = `https://vault.bitrealm.io` | Task 1 |
| Canonical company URL `https://bitrealm.io/` | Task 3 |
| Absolute vault. host on company buttons | Task 3 |
| No old-path redirects | all tasks |
| In-repo link rewrite | Task 2 + Task 4 |
| Skip `docs/superpowers/` | Task 4 |
| CONTRIBUTING DNS + root redirect | Task 4 |
| `CNAME` stays `bitrealm.io` | Task 4 (do not edit `docs/public/CNAME`) |
| `npm run check && npm run build` | Tasks 2, 3, 5 |
