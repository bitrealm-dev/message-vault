# message-contacts

Load a VCF or a vCard CSV and resolve names to phone numbers (and the reverse) for backup converters.

The desktop app Contacts screen and several exporters use this crate. The `contacts-validate` helper binary checks a contacts file from a terminal.

## Build and test

```bash
cargo test -p contacts
cargo run -p contacts --bin contacts-validate -- --help
```

Workspace setup: [CONTRIBUTING.md](../../../CONTRIBUTING.md).

## Docs

This crate is a library. Desktop app contacts: https://bitrealm.dev/how-to/contacts-and-labels/

## License

AGPL-3.0. See the repository root `LICENSE`.
