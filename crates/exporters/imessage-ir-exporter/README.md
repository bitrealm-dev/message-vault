# imessage-ir-exporter

Export Apple Messages from a Mac `chat.db` or an iPhone backup into JSON Lines, JSON, CSV, EML, MBOX, or XML.

The desktop app Import screen uses this crate as a library. The `imessage-ir-exporter` command is the same converter from a terminal.

## Build and test

```bash
cargo test -p imessage-ir-exporter
cargo run -p imessage-ir-exporter -- --help
```

Workspace setup: [CONTRIBUTING.md](../../../CONTRIBUTING.md).

## Docs

Command-line options: https://bitrealm.io/vault/developer/reference/cli/imessage-ir-exporter/

## License

Fair Core License. See the repository root `LICENSE.md`. This crate depends on `imessage-database` (GPL-3.0-or-later), so distributing a binary that includes it must also satisfy the GPL.
