# Maintainer documentation

This directory holds contributor and operator notes that are **not** published
in the Starlight site.

End-user guides live in [`../src/content/docs/`](../src/content/docs/) and are
published at <https://bitrealm-dev.github.io/message-vault-rs/>.

| Doc | Topic |
|-----|-------|
| [development.md](development.md) | Windows/Linux setup, checks, troubleshooting |
| [../../demo/README.md](../../demo/README.md) | Demo dataset contents and regen |
| [../../web/STYLE_GUIDE.md](../../web/STYLE_GUIDE.md) | Web UI theme tokens |

Bitrealm production VPS runbook (Cloudflare, Hanko, Hub compose, Ansible) lives
in the private **message-vault-ops** repository.

## Documentation site

User-facing docs use [Astro Starlight](https://starlight.astro.build/) under
[`docs/`](..), deployed by [`.github/workflows/docs.yml`](../../.github/workflows/docs.yml).

### Enable Pages (one-time)

1. Repo **Settings → Pages**.
2. **Build and deployment → Source** → **GitHub Actions**.
3. Push to `main` or run the **Docs** workflow under **Actions**.
4. Site URL: `https://bitrealm-dev.github.io/message-vault-rs/`.

Local preview:

```bash
cd docs
npm ci
npm run dev
```

Run `npm run check` and `npm run build` before publishing documentation changes.

### Cross-links with Message Exporters

Keep the two sites separate. Link exporters for backup → **message-ir** JSONL
(and release layout `message-exporter` + `lib/` + `cli/`), and this site for
store / import / browse (message-ir ingest):

- Exporters docs: <https://bitrealm-dev.github.io/message-exporters/>
- Exporters install / archive layout: <https://bitrealm-dev.github.io/message-exporters/get-started/install/>
- Vault docs: <https://bitrealm-dev.github.io/message-vault-rs/>
- message-ir ingest: <https://bitrealm-dev.github.io/message-vault-rs/reference/message-ir/>

Prefer absolute published URLs when linking across sites. A reciprocal “Import
into Message Vault” link on the exporters site can be added in that repository
when convenient; it is not required to ship this site.
