# Web UI (Next.js browse)

Next.js browse UI restored from git history (`7a1b6be`, handles-schema tip).
Browses the vault SQLite database (`data/vault.db` by default) via Route
Handlers and `better-sqlite3`.

The primary product GUI is the Vite SPA in [`../web/`](../web/) (Tauri + Docker
static). This package is the restored historical Next.js browse app.

Product documentation:

**https://bitrealm.io/vault/user/browse-your-messages/**

## Local development

From the repo root, import or reset demo data first (`./scripts/run-vault-dev.sh --reset-demo`, or see
[CONTRIBUTING.md](../CONTRIBUTING.md)), then:

```bash
# From repo root — convert media for the browser (requires ffmpeg)
cargo run --release -p message-vault-server -- process-assets

cd web-next
npm ci
npm run dev
```

Open [http://localhost:3000](http://localhost:3000).

### Scripts

| Script | Purpose |
|--------|---------|
| `npm run dev` | Next.js dev server |
| `npm run build` / `start` | Production build / serve (`output: "standalone"`) |
| `npm run lint` | ESLint |
| `npm test` | `tsx --test src/**/*.test.ts` |

Derived media: from the repo root, run
`cargo run --release -p message-vault-server -- process-assets` with optional
flags `--force`, `--dry-run`, `--skip-image`, `--skip-video`, `--skip-audio`,
`--db`, `--source`.

### Notes

- Paths and DB location come from repo-root `config/config.toml`
  (override with `VAULT_DB` / `VAULT_DATA_DIR`).
- On-disk layout (same as the Rust vault server):
  - `data/vault.db`
  - `data/<account_id>/<source_id>/assets/`
  - `data/<account_id>/<source_id>/assets_converted/`
- `./scripts/run-vault-dev.sh` writes `data/` in the git checkout, so this
  Next.js app and the vault server see the same files.
- JSONL import is the Rust `serve` API / CLI, not Next.js.
- Reset demo is CLI-only:
  `cargo run --release -p message-vault-server -- reset-demo`.
- Regenerate embedded DDL after schema changes:
  `node scripts/sync-vault-schema.mjs`
