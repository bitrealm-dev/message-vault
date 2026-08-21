# vault-pull

Download messages from a running vault server into a local JSON Lines folder (`*.jsonl` plus `attachments/`).

The desktop app **Export** screen uses this crate as a library. The `vault-pull` command is the same exporter from a terminal. Create an API token under **Settings → Account** in the vault website.

## Build and test

```bash
cargo test -p vault-pull
cargo run -p vault-pull -- --help
```

Workspace setup: [CONTRIBUTING.md](../../../CONTRIBUTING.md).

## Docs

Command-line options: https://bitrealm.io/vault/developer/reference/cli/vault-pull/

## License

AGPL-3.0. See the repository root `LICENSE`.
