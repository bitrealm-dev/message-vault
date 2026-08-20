# whatsapp-exporter

Extract WhatsApp backups with `wtsexporter`, then convert the result into JSON Lines, JSON, CSV, EML, MBOX, or XML.

The desktop app Extract Messages screen uses this crate as a library. The `whatsapp-exporter` command is the same converter from a terminal.

## Build and test

```bash
cargo test -p whatsapp-exporter
cargo run -p whatsapp-exporter -- --help
```

Workspace setup: [CONTRIBUTING.md](../../../CONTRIBUTING.md).

## Docs

Command-line options: https://vault.bitrealm.dev/developer/reference/cli/whatsapp-exporter/

## License

AGPL-3.0. See the repository root `LICENSE`.
