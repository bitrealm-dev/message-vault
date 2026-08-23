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

Fair Core License. See the repository root `LICENSE.md`. This crate links `imessage-ir-exporter`, which depends on `imessage-database` (GPL-3.0-or-later); distributing a binary that includes it must also satisfy the GPL.
