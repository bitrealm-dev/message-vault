# message-vault-io-core

Shared form models (`Form`, `Exporter`, `ExporterConfig`), job spawning (`spawn_job` with cancel and progress), and `export.ini` load/save.

The Tauri desktop app in `src-tauri/` uses this crate. Exporter libraries use the same config types.

## Build and test

```bash
cargo test -p message-vault-io-core
```

Workspace setup: [CONTRIBUTING.md](../../../CONTRIBUTING.md).

## Docs

This crate is a library used by the desktop app and the converters. Maintainer GUI notes: [crates/message-vault-io-gui/gui.md](../../message-vault-io-gui/gui.md).

## License

AGPL-3.0. See the repository root `LICENSE`.
