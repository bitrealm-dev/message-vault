# obfuscate

Rewrite an export so chats, timestamps, and attachment counts stay, while names, numbers, message bodies, and media bytes are replaced. Remaps are stable for a given seed and are not reversible from the output alone.

`message-ir-format` applies this when obfuscation is on. The `imazing-obfuscate` binary can rewrite an iMazing vendor CSV in place.

## Build and test

```bash
cargo test -p obfuscate
cargo run -p obfuscate --bin imazing-obfuscate -- --help
```

Workspace setup: [CONTRIBUTING.md](../../../CONTRIBUTING.md).

## Docs

This crate is a library. User options: https://bitrealm.dev/how-to/media-and-privacy/

## License

MIT.
