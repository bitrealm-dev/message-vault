# Bitrealm company page and vault.bitrealm.io docs hosts

## Context

https://bitrealm.io/ is one Astro Starlight app under `docs/`. The homepage is a Message Vault splash (`docs/src/content/docs/index.mdx`). The sidebar has two topics (User Guide and Developer), but almost every User Guide URL sits at the site root (`/get-started/…`, `/how-to/…`). Developer pages are split: two files under `/developer/…`, and the rest under `/formats/…` and `/reference/…`.

GitHub Pages deploys that app from `.github/workflows/docs.yml`. The custom domain is `bitrealm.io` (`docs/public/CNAME`). Cloudflare already fronts the apex. Subdomains `api` and `app` exist and stay untouched.

The org has one product. The apex should read as Bitrealm, not as the first page of the guidebook. Message Vault docs should live on a product hostname with a `/user/` or `/developer/` prefix on every page.

## Goals

- `https://bitrealm.io/` is a short Bitrealm company page. The visitor chooses User Guide, Developer, or GitHub.
- Every guidebook page is under `/user/…` or `/developer/…` on `https://vault.bitrealm.io`.
- `/user/` keeps today’s splash. `/developer/` is a new short index.
- In-repo links that cite `https://bitrealm.io/…` guidebook URLs are rewritten in the same change.
- Old apex paths (`/get-started/…`, `/formats/…`, `/reference/…`) are not redirected. Bookmarks to those URLs 404.

## Non-goals

- HTTP redirects from today’s guidebook paths
- A second GitHub Pages site, a second docs workflow, or a Cloudflare Worker
- Try / Download buttons, an About page, a blog, or a second product slot on the company page
- Rewriting User Guide chapter copy (move files and fix links only)
- Moving `docs/maintainers/` onto the public site
- Changing `api`, `app`, or R2 DNS
- Runtime, exporter, desktop app, or vault-server code

## Architecture

One Astro project, one GitHub Pages deploy, two visual systems, two hostnames.

```text
https://bitrealm.io/                    Custom company page (src/pages/index.astro)
                                         No Starlight sidebar

https://vault.bitrealm.io/              Cloudflare Redirect Rule → https://bitrealm.io/
                                         Visitor must pick User Guide or Developer

https://vault.bitrealm.io/user/…        Starlight User Guide
https://vault.bitrealm.io/developer/…   Starlight Developer docs
```

Delete `docs/src/content/docs/index.mdx` so Starlight does not own `/`. The company page is `docs/src/pages/index.astro`. Starlight content lives under `docs/src/content/docs/user/` and `docs/src/content/docs/developer/`.

`starlight-sidebar-topics` stays. User Guide `link` is `/user/`. Developer `link` is `/developer/`.

`astro.config.mjs` `site` is `https://vault.bitrealm.io`. Each Starlight page’s canonical URL is `https://vault.bitrealm.io` plus its path. The company page’s canonical URL is `https://bitrealm.io/`.

Because this is one static build, the same files also answer on the other host (`bitrealm.io/user/…` works). Company-page buttons use absolute `https://vault.bitrealm.io/…` URLs so a click leaves the apex.

## Company page (`bitrealm.io/`)

Custom Astro page. Colors from `docs/src/styles/custom.css`. Not Starlight’s splash template.

**Header:** Bitrealm on the left. Three text links on the right:

- User Guide → `https://vault.bitrealm.io/user/`
- Developer → `https://vault.bitrealm.io/developer/`
- GitHub → `https://github.com/bitrealm-io/message-vault`

**Body, in order:**

1. Title: Bitrealm
2. One sentence for the org (local software the visitor runs, not a cloud account). Final wording is set during implementation.
3. One Message Vault card: product name, one-line pitch taken from the current splash tagline, and the same three links.

No Try / Download CTAs. No empty slots for a second product.

## Docs homes and sidebar

On `vault.bitrealm.io`, Starlight still titles the site Message Vault. GitHub social link stays.

**`/user/`** is today’s splash, moved as-is (Try the vault, Use your own messages, chapter cards). Links inside that file are rewritten to `/user/…` or `/developer/…`.

**`/developer/`** is a new short page: one paragraph, then links to Run from source, Operator Docker, CLI, HTTP API, Formats, and instance internals.

**Sidebar slugs**

- User Guide: `user`, `user/get-started/…`, `user/prepare-a-backup/…`, `user/import-from-a-backup`, `user/browse-your-messages`, `user/how-to/…`, `user/glossary`
- Developer: `developer` (new index), `developer/run-from-source`, `developer/docker-compose`, `developer/reference/…`, `developer/formats/…`

## URL map

No HTTP redirects for the “Today” column. In-repo citations are rewritten.

### User Guide

| Today | After |
|---|---|
| `bitrealm.io/` (current splash) | `vault.bitrealm.io/user/` |
| `bitrealm.io/get-started/…` | `vault.bitrealm.io/user/get-started/…` |
| `bitrealm.io/prepare-a-backup/…` | `vault.bitrealm.io/user/prepare-a-backup/…` |
| `bitrealm.io/import-from-a-backup/` | `vault.bitrealm.io/user/import-from-a-backup/` |
| `bitrealm.io/browse-your-messages/` | `vault.bitrealm.io/user/browse-your-messages/` |
| `bitrealm.io/how-to/…` | `vault.bitrealm.io/user/how-to/…` |
| `bitrealm.io/glossary/` | `vault.bitrealm.io/user/glossary/` |

### Developer

| Today | After |
|---|---|
| *(none)* | `vault.bitrealm.io/developer/` |
| `bitrealm.io/developer/run-from-source/` | `vault.bitrealm.io/developer/run-from-source/` |
| `bitrealm.io/developer/docker-compose/` | `vault.bitrealm.io/developer/docker-compose/` |
| `bitrealm.io/reference/…` | `vault.bitrealm.io/developer/reference/…` |
| `bitrealm.io/formats/…` | `vault.bitrealm.io/developer/formats/…` |

## DNS and the product-host root

GitHub Pages still has one custom domain: `bitrealm.io` (`docs/public/CNAME` is unchanged). A second hostname is Cloudflare in front of that same deploy.

**Operator steps** (documented in `CONTRIBUTING.md` next to the existing domain notes; not automated):

1. Cloudflare: CNAME `vault` → `bitrealm.io`, **proxied** (orange cloud). Leave `api`, `app`, and R2 alone. Do not grey-cloud a CNAME to `*.github.io` — Pages will not bind `vault.bitrealm.io`.
2. Cloudflare Redirect Rule (or Bulk Redirect): `https://vault.bitrealm.io/` → `https://bitrealm.io/`. Match the hostname root only (`/` or empty path). Do not match `/user` or `/developer`.
3. GitHub Pages settings stay Actions + custom domain `bitrealm.io`. Do not put `vault.bitrealm.io` in `docs/public/CNAME`.

The site can ship on `bitrealm.io/user/…` before the CNAME exists. Company-page buttons and canonical tags already use `https://vault.bitrealm.io/…`. Those URLs work after the CNAME and the root redirect are in place.

There is no meta-refresh page on `vault.bitrealm.io/`. The Redirect Rule is what forces the chooser onto the apex.

## In-repo link rewrite

Update these in the same change:

- Markdown and MDX under `docs/src/content/docs/`
- `docs/astro.config.mjs` sidebar slugs
- Root `README.md`, `CONTRIBUTING.md`, `CLAUDE.md`
- Crate README files and rustdoc comments that cite `https://bitrealm.io/…` guidebook URLs
- `docs/maintainers/*.md` (not historical `docs/superpowers/` specs and plans)
- `docker/compose.yml` comment
- `.github/workflows/ci.yml` release-notes text that cites the docs URL

Leave `docs/superpowers/` history as written.

## Success criteria

- `cd docs && npm run check && npm run build` succeeds.
- `https://bitrealm.io/` (after deploy) is the company page, not the Message Vault splash.
- User Guide pages exist at `/user/…` and Developer pages at `/developer/…`.
- Grep of the repo (excluding `docs/superpowers/`) finds no leftover `https://bitrealm.io/get-started/`, `https://bitrealm.io/how-to/`, `https://bitrealm.io/formats/`, or `https://bitrealm.io/reference/` guidebook links.
- After the operator DNS steps: `https://vault.bitrealm.io/` lands on `https://bitrealm.io/`; `https://vault.bitrealm.io/user/` and `https://vault.bitrealm.io/developer/` serve Starlight.

## Implementation notes

- File moves are git moves so history stays attached.
- Starlight relative links that used root paths (`/get-started/…`) become `/user/get-started/…` or `/developer/…`.
- Verification: `npm run check` and `npm run build` in `docs/`. Spot-check the company page and both topic homes in `npm run dev`.
