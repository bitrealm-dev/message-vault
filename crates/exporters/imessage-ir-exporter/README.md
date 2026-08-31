# imessage-ir-exporter

Export Apple Messages from a Mac `chat.db` or an iPhone backup into JSON Lines, JSON, CSV, EML, MBOX, or XML.

The desktop app Import screen uses this crate as a library.

## Build and test

```bash
cargo test -p imessage-ir-exporter
```

Workspace setup: [CONTRIBUTING.md](../../../CONTRIBUTING.md).

## Docs

How this crate fits the export pipeline: https://bitrealm.io/vault/developer/message-transfer/

## License

Fair Core License. See the repository root `LICENSE.md`. This crate depends on `imessage-database` (GPL-3.0-or-later), so distributing a binary that includes it must also satisfy the GPL.
