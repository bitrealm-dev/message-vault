# imazing-exporter

Rescue messages from an iMazing Messages or WhatsApp CSV export into JSON Lines, JSON, CSV, EML, MBOX, or XML. This is a limited rescue import.

The desktop app Extract Messages screen uses this crate as a library. The `imazing-exporter` command is the same converter from a terminal.

## Build and test

```bash
cargo test -p imazing-exporter
cargo run -p imazing-exporter -- --help
```

Workspace setup: [CONTRIBUTING.md](../../../CONTRIBUTING.md).

## Docs

Command-line options: https://bitrealm.dev/reference/cli/imazing-exporter/

Input format: https://bitrealm.dev/formats/imazing/input/

Importer design: https://bitrealm.dev/formats/imazing/design/

## License

MIT.
