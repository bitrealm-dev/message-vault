# message-reexport

Convert an existing Message Vault output folder from one packaging format to another (JSON Lines, JSON, CSV, EML, MBOX, or XML).

The desktop app's Export screen uses this crate to write any format other than JSON Lines.

## Build and test

```bash
cargo test -p message-reexport
```

Workspace setup: [CONTRIBUTING.md](../../../CONTRIBUTING.md).

## Docs

How this crate fits the export pipeline: https://bitrealm.io/vault/developer/message-transfer/

How conversion works: https://bitrealm.io/vault/developer/formats/convert/

## License

Fair Core License. See the repository root `LICENSE.md`.
