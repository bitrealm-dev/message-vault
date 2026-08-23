# message-reexport

Convert an existing Message Vault output folder from one packaging format to another (JSON Lines, JSON, CSV, EML, MBOX, or XML).

The desktop app **Format** tab uses this crate as a library. The `message-reexporter` command is the same converter from a terminal.

## Build and test

```bash
cargo test -p message-reexport
cargo run -p message-reexport -- --help
```

Workspace setup: [CONTRIBUTING.md](../../../CONTRIBUTING.md).

## Docs

Command-line options: https://bitrealm.io/vault/developer/reference/cli/message-reexporter/

How conversion works: https://bitrealm.io/vault/developer/formats/convert/

## License

Fair Core License. See the repository root `LICENSE.md`.
