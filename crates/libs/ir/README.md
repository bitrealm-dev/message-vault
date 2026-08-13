# message-ir

Shared conversation types for Message Vault: `ConversationDocument`, messages, attachments, and participants. This crate has no I/O and no formatting. Attachment bytes are never serialized to JSON; paths and hashes point at sidecar files.

Exporters, `message-ir-format`, `vault-push`, `vault-pull`, and the vault server use this crate.

## Build and test

```bash
cargo test -p message-ir
```

Workspace setup: [CONTRIBUTING.md](../../../CONTRIBUTING.md).

## Docs

This crate is a library. Schema notes for contributors: [shared message model](../../../docs/maintainers/architecture/message-ir.md). JSON Lines layout for imports: https://bitrealm.dev/reference/export-structure/

## License

AGPL-3.0. See the repository root `LICENSE`.
