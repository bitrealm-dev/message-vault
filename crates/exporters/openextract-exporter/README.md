# openextract-exporter

Rescue messages from an OpenExtract conversation CSV (and optional VCF) into JSON Lines, JSON, CSV, EML, MBOX, or XML. This is a limited rescue import.

The desktop app Extract Messages screen uses this crate as a library. The `openextract-exporter` command is the same converter from a terminal.

## Build and test

```bash
cargo test -p openextract-exporter
cargo run -p openextract-exporter -- --help
```

Workspace setup: [CONTRIBUTING.md](../../../CONTRIBUTING.md).

## Docs

Command-line options: https://bitrealm.io/vault/developer/reference/cli/openextract-exporter/

## License

Fair Core License. See the repository root `LICENSE.md`.
