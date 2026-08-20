# Guides under bitrealm.dev/vault (drop vault.bitrealm.dev)

This spec replaces the hostname plan in `2026-08-20-bitrealm-company-page-and-vault-docs-hosts-design.md`. That plan assumed GitHub Pages could serve `bitrealm.dev` and `vault.bitrealm.dev` from one site. GitHub’s own docs say it cannot.

## Context

The docs site is one Astro Starlight app under `docs/`, published by `.github/workflows/docs.yml` to GitHub Pages.

Verified 2026-08-20 against [Managing a custom domain for your GitHub Pages site](https://docs.github.com/en/pages/configuring-a-custom-domain-for-your-github-pages-site/managing-a-custom-domain-for-your-github-pages-site), [Troubleshooting custom domains](https://docs.github.com/en/pages/configuring-a-custom-domain-for-your-github-pages-site/troubleshooting-custom-domains-and-github-pages), and `GET /repos/bitrealm-dev/message-vault/pages`:

- The Pages custom domain is a single field. Live value: `bitrealm.dev`.
- Publish source is a GitHub Actions workflow, so `docs/public/CNAME` is ignored. The Pages settings field is the source of truth.
- GitHub does not support both a root domain and a second custom subdomain on one site. The documented unsupported example is `example.com` and `docs.example.com`.
- A subdomain CNAME must point at `bitrealm-dev.github.io`, not at `bitrealm.dev`. Pointing a subdomain at the root domain breaks HTTPS.
- Extra DNS names that try to use Pages can block the `bitrealm.dev` certificate. At spec time the certificate state was `dns_changed` and Enforce HTTPS was off.
- GitHub’s extra-name option is a **redirect** (the address bar changes). A Cloudflare Worker can keep a second name in the address bar, but that is not a GitHub Pages feature.

After PR #54, the live tree is:

- `https://bitrealm.dev/` — Bitrealm company page
- `https://bitrealm.dev/user/…` — User Guide
- `https://bitrealm.dev/developer/…` — Developer docs
- Company-page buttons and `astro.config.mjs` `site` still say `https://vault.bitrealm.dev`

`vault.bitrealm.dev` currently hits Cloudflare error 526. That name is dropped.

## Goals

- Company page stays at `https://bitrealm.dev/`.
- User Guide lives at `https://bitrealm.dev/vault/user/…`.
- Developer docs live at `https://bitrealm.dev/vault/developer/…`.
- Stop using `vault.bitrealm.dev` (no DNS record, no Worker, no links).
- Fix internal links in the same change. No HTTP redirects of any kind.

## Non-goals

- Any redirect: not `/user/` → `/vault/user/`, not `vault.bitrealm.dev` → `bitrealm.dev`, not old `/get-started/` paths
- A Cloudflare Worker or Redirect Rule
- A second GitHub Pages site or a second docs workflow
- A page at `https://bitrealm.dev/vault/` (that URL may 404)
- Try / Download buttons, an About page, or a second product on `/`
- Rewriting User Guide chapter copy except links and file moves
- Moving `docs/maintainers/` onto the public site
- Changing `api`, `app`, or `cdn` DNS
- Runtime, exporter, desktop app, or vault-server code except rustdoc URL strings

## Public URLs

| What | Address |
|------|---------|
| Bitrealm home | `https://bitrealm.dev/` |
| User Guide home (today’s splash) | `https://bitrealm.dev/vault/user/` |
| Developer home | `https://bitrealm.dev/vault/developer/` |

After this change, `https://bitrealm.dev/user/` and `https://bitrealm.dev/developer/` 404. Bookmarks to `vault.bitrealm.dev` fail once the DNS record is gone.

## Architecture

One Astro project. One GitHub Pages deploy. One custom domain: `bitrealm.dev`.

```text
https://bitrealm.dev/                      Company page (src/pages/index.astro)
https://bitrealm.dev/vault/user/…          Starlight User Guide
https://bitrealm.dev/vault/developer/…     Starlight Developer docs
```

Do not set Astro `base` to `/vault`. That would put the company page under `/vault` as well. Put Starlight files under a `vault/` folder instead.

`astro.config.mjs` `site` is `https://bitrealm.dev`. Company-page buttons use `https://bitrealm.dev/vault/user/` and `https://bitrealm.dev/vault/developer/`. Starlight in-page links use `/vault/user/…` and `/vault/developer/…`.

`starlight-sidebar-topics` stays. User Guide `link` is `/vault/user/`. Developer `link` is `/vault/developer/`. Every sidebar slug gets a `vault/` prefix.

Pages settings stay: source GitHub Actions, custom domain `bitrealm.dev`. Do not type `vault.bitrealm.dev` into that field.

## File moves

Use `git mv` so history stays attached.

- `docs/src/content/docs/user/` → `docs/src/content/docs/vault/user/`
- `docs/src/content/docs/developer/` → `docs/src/content/docs/vault/developer/`

`docs/src/pages/index.astro` stays at `/`.

## Link rewrite

Update links that still say `https://vault.bitrealm.dev/…`, `/user/…`, or `/developer/…` (as Starlight root paths) so they use `/vault/user/…` and `/vault/developer/…` or the matching `https://bitrealm.dev/vault/…` URL.

Same live-file set as PR #54:

- Markdown and MDX under `docs/src/content/docs/`
- `docs/astro.config.mjs`
- Root `README.md`, `CONTRIBUTING.md`, `CLAUDE.md`
- Crate READMEs and rustdoc comments that cite guidebook URLs
- `docs/maintainers/*.md`
- `docker/compose.yml` comment
- `.github/workflows/ci.yml` release-notes text that cites the docs URL

Leave historical files under `docs/superpowers/` as written, including the old hostname spec.

`CONTRIBUTING.md`: delete the Worker / `vault.bitrealm.dev` setup. State that Pages custom domain is `bitrealm.dev`, guides live under `/vault/user/` and `/vault/developer/`, and operators must not add a `vault` DNS name.

## DNS (operator, not in the repo)

1. In Cloudflare, delete the `vault` DNS record.
2. Leave `api`, `app`, and `cdn` alone.
3. Do not add a Worker or a redirect rule for `vault`.
4. After GitHub shows a valid certificate for `bitrealm.dev`, turn on **Enforce HTTPS** in Pages settings.

`bitrealm.dev` A records already point at GitHub Pages (`185.199.108–111.153`). Keep them that way (not orange-clouded), so GitHub can keep issuing the certificate.

## Success criteria

- `cd docs && npm run check && npm run build` succeeds.
- Built `dist/index.html` is the company page (no “Your messages, your way”).
- Built `dist/vault/user/index.html` has “Your messages, your way”.
- Built `dist/vault/developer/index.html` exists.
- `dist/user` and `dist/developer` (as today’s guide roots) do not exist.
- Grep of the repo (excluding `docs/superpowers/`) finds no leftover `https://vault.bitrealm.dev`.
- Grep of live files (excluding `docs/superpowers/`) finds no Starlight or in-repo guidebook hrefs that still use `/user/` or `/developer/` without the `/vault/` prefix.
- After the operator deletes the `vault` DNS record: `https://bitrealm.dev/vault/user/` and `https://bitrealm.dev/vault/developer/` load; `vault.bitrealm.dev` is unused.

## Implementation notes

- File moves are git moves.
- Verification: `npm run check` and `npm run build` in `docs/`. Spot-check `/`, `/vault/user/`, and `/vault/developer/` in `npm run dev`.
- Do not add Astro or Cloudflare redirects to “help” old URLs.
