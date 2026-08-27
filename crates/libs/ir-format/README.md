# message-ir-format

Writes a `ConversationDocument` to JSON, JSON Lines, CSV, EML, MBOX, or a single SyncTech `smses.xml`. Media convert/compress and obfuscation run here when finishing an export. Readers exist for every format so a folder can be converted later.

Exporters and `message-reexport` use this crate. The desktop app Format tab uses it through `message-reexport`.

## Build and test

```bash
cargo test -p message-ir-format
```

Workspace setup: [CONTRIBUTING.md](../../../CONTRIBUTING.md).

## Docs

This crate is a library. Contributor notes: [shared message model](../../../docs/maintainers/architecture/message-ir.md). Mail archives: https://bitrealm.io/vault/developer/formats/mail-archive/ . XML output: https://bitrealm.io/vault/developer/formats/sms-backup-restore-xml/

## License

AGPL-3.0. See the repository root `LICENSE`.
