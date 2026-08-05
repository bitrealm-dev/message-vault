# Message Vault GUI

Desktop GUI built with [Slint](https://slint.dev). Same exporter libraries and
`export.ini` as the rest of the workspace.

**End-user guides:** [docs site](https://bitrealm-dev.github.io/message-vault-io/).

## Run in development

```bash
cargo build --workspace
cargo run -p message-vault-io-gui
```

For release:

```bash
cargo build --release -p message-vault-io-gui
./target/release/message-vault-io
```

On Windows the final command is `target\release\message-vault-io.exe`.

The app searches for helpers under `lib/` (`ffmpeg`/`ffprobe`) and `cli/`
(`wtsexporter` only) next to its own executable, then in `MESSAGE_VAULT_IO_BIN`,
then on `PATH`. Exporters are linked as libraries; they are not separate CLIs
in the message-vault-io release archive.

## Look and feel

Built with Slint's **`native`** widget style (set in `build.rs`):

- **Windows** — Fluent
- **macOS** — Cupertino
- **Linux** — Qt if Qt 5.15+ is installed; otherwise Fluent (pure-Rust fallback; no Qt SDK required)

Custom chrome uses a Fastmail-style **four-seed theme** (same defaults/presets as
message-vault-rs): mode `light` / `dark` / `system` (default dark) and named
color presets. Change them on the Home screen; values persist in
`export.ini` under `[appearance]` (`mode=`, `preset=`). See
[`docs/maintainers/gui.md`](../../docs/maintainers/gui.md) for tokens.

Forms use a classic dialog grid (right-aligned label column, full-width
controls, tight row gaps, content packed at the top). Form rows use bare
`HorizontalLayout`/`VerticalLayout` — not `HorizontalBox`/`VerticalBox`, which
inject Fluent's 8px `layout-padding` per side and inflate every row. Ordinary
fields do not stretch when you grow the window vertically; only the Log viewer
does. Dropdown menus use compact rows; Backup type separates supported sources
from the Experimental group. Override the style at compile time with
`SLINT_STYLE` if needed:

```bash
SLINT_STYLE=fluent cargo run -p message-vault-io-gui
```

## Included

- Top tabs: **Extract Messages** | **Format** | **Vault** | **Contacts** | **Log**
- **Extract Messages**: choose a backup source and extract a JSONL archive; attachments, obfuscation, and optional date filters are available
- **Format**: convert a prior Message Vault output folder to another format
- **Vault**: push a JSONL export folder into Message Vault
- **Contacts**: Check (dry run) / Update (write corrected files)
- Forms for GO SMS Pro, SMS Backup & Restore, SMS Backup+, OpenExtract, iMazing, WhatsApp, and iPhone backup
- Native file/folder dialogs via `rfd`
- Live run log with cooperative cancel
- Help button linking to the published user documentation
- About dialog with Slint attribution (`AboutSlint`) for the Royalty-free license

Architecture notes: [`../../docs/maintainers/gui.md`](../../docs/maintainers/gui.md).
