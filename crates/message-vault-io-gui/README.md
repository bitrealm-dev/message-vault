# DEPRECATED — Slint desktop app

This Slint GUI is deprecated. Do not modify it or add features to it. The supported UI is the Tauri desktop app (`src-tauri/` plus `web/`).

It still compiles as a standalone binary for historical reference.

## Run (reference only)

```bash
cargo run -p message-vault-io-gui
```

Release binary name: `message-vault-io`.

Workspace setup: [CONTRIBUTING.md](../../CONTRIBUTING.md).

## License

AGPL-3.0. The desktop app that replaced this crate still links `imessage-ir-exporter` (`imessage-database` is GPL-3.0-or-later).
