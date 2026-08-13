# go-sms-pro-exporter

Rescue messages from a GO SMS Pro XML (and PDU) export into JSON Lines, JSON, CSV, EML, MBOX, or XML. This is a limited rescue import.

The desktop app Extract Messages screen uses this crate as a library. The `go-sms-pro-exporter` command is the same converter from a terminal.

## Build and test

```bash
cargo test -p go-sms-pro-exporter
cargo run -p go-sms-pro-exporter -- --help
```

Workspace setup: [CONTRIBUTING.md](../../../CONTRIBUTING.md).

## Docs

Command-line options: https://bitrealm.dev/reference/cli/go-sms-pro-exporter/

## License

MIT.
