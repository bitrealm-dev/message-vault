# sms-backup-restore-exporter

Convert an SMS Backup & Restore (SyncTech) XML backup into JSON Lines, JSON, CSV, EML, MBOX, or SMS Backup & Restore XML.

The desktop app Extract Messages screen uses this crate as a library. The `sms-backup-restore-exporter` command is the same converter from a terminal.

## Build and test

```bash
cargo test -p sms-backup-restore-exporter
cargo run -p sms-backup-restore-exporter -- --help
```

Workspace setup: [CONTRIBUTING.md](../../../CONTRIBUTING.md).

## Docs

Command-line options: https://bitrealm.io/vault/developer/reference/cli/sms-backup-restore-exporter/

Input format: https://bitrealm.io/vault/developer/formats/sms-backup-restore/input/

Import mapping: https://bitrealm.io/vault/developer/formats/sms-backup-restore/mapping/

## License

AGPL-3.0. See the repository root `LICENSE`.
