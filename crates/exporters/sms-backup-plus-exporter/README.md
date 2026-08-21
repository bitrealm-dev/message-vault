# sms-backup-plus-exporter

Rescue messages from an SMS Backup+ `.eml` mail archive into JSON Lines, JSON, CSV, EML, MBOX, or XML. This is a limited rescue import.

The desktop app Extract Messages screen uses this crate as a library. The `sms-backup-plus-exporter` command is the same converter from a terminal.

## Build and test

```bash
cargo test -p sms-backup-plus-exporter
cargo run -p sms-backup-plus-exporter -- --help
```

Workspace setup: [CONTRIBUTING.md](../../../CONTRIBUTING.md).

## Docs

Command-line options: https://bitrealm.io/vault/developer/reference/cli/sms-backup-plus-exporter/

Format notes: https://bitrealm.io/vault/developer/formats/sms-backup-plus/format/

Import mapping: https://bitrealm.io/vault/developer/formats/sms-backup-plus/mapping/

## License

AGPL-3.0. See the repository root `LICENSE`.
