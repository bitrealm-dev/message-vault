# Web UI

Next.js app that browses the vault SQLite database (`data/vault.db` by default).

Product documentation (routes, Message Sources, contacts, settings, undo):

**https://bitrealm-dev.github.io/message-vault-rs/browse/navigation-and-sources/**

## Local development

From the repo root, import or reset demo data first (see the
[root README](../README.md)), then:

```bash
npm ci
npm run process-assets
npm run dev
```

Open [http://localhost:3000](http://localhost:3000).

### Scripts

| Script | Purpose |
|--------|---------|
| `npm run dev` | Next.js dev server |
| `npm run build` / `start` | Production build / serve |
| `npm run lint` | ESLint |
| `npm test` | `tsx --test src/**/*.test.ts` |
| `npm run process-assets` | Convert media for the browser |

`process-assets` flags: `--force`, `--dry-run`, `--skip-image`, `--skip-video`,
`--skip-audio`.

### Notes

- Paths and DB location come from repo-root `config/config.toml`
  (override with `VAULT_DB` / `VAULT_DATA_DIR`).
- Converted assets land under
  `data/<account_id>/<source_id>/assets_converted`.
- JSONL import is the Rust `serve` API / CLI, not Next.js.
- Reset demo is CLI-only: `cargo run --release -- reset-demo`.
