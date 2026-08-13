# vault-push

Import a Message Vault JSON Lines export folder into a running vault server.

The desktop app **Import** screen uses this crate as a library. The `vault-push` command is the same importer from a terminal. Create an API token under **Settings → Account** in the vault website.

## Build and test

```bash
cargo test -p vault-push
cargo run -p vault-push -- --help
```

Workspace setup: [CONTRIBUTING.md](../../../CONTRIBUTING.md).

## Docs

Command-line options: https://bitrealm.dev/reference/cli/vault-push/

## License

MIT.
