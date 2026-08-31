# sms-backup-restore-exporter

Convert an SMS Backup & Restore (SyncTech) XML backup into JSON Lines, JSON, CSV, EML, MBOX, or SMS Backup & Restore XML.

The desktop app Import screen uses this crate as a library.

## Build and test

```bash
cargo test -p sms-backup-restore-exporter
```

Workspace setup: [CONTRIBUTING.md](../../../CONTRIBUTING.md).

## Docs

How this crate fits the export pipeline: https://bitrealm.io/vault/developer/message-transfer/

Input format: https://bitrealm.io/vault/developer/formats/sms-backup-restore/input/

Import mapping: https://bitrealm.io/vault/developer/formats/sms-backup-restore/mapping/

## License

Fair Core License. See the repository root `LICENSE.md`.
