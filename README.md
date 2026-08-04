# message-vault-io

Phone backups are easy to make. Reading the messages later is harder.

This project turns vendor backups into a shared [conversation structure](docs/src/content/docs/understand-output/export-structure.md), then packages each conversation in the format you pick (default **JSON**):

- **JSON** / **JSON Lines** — default packaging; machine-readable archives
- **CSV** — one spreadsheet file per conversation
- **EML** — one email folder per conversation
- **MBOX** — one mailbox file per conversation
- **XML** — one SyncTech `smses.xml` backup

Photos and other media are saved next to those files when the format needs them.

## Docs

Read the full guide (install, desktop app, supported backups, CSV layout):

**https://bitrealm-dev.github.io/message-vault-io/**

Source Markdown lives in [`docs/src/content/docs/`](docs/src/content/docs/) and is published with Astro Starlight.

## Quick start

**Desktop app / binaries:** Download the platform ZIP from the latest [Release](https://github.com/bitrealm-dev/message-vault-io/releases). Extract it and keep every file in the same folder. Run `message-vault-io`.

**From source:**

```bash
cargo build --workspace --release
cargo run --release -p message-vault-io-gui
```

### WSL2 development

Use WSL2 with WSLg enabled and keep the repository in the Linux filesystem
(`~/repo/...`), not under `/mnt/c`. From Windows PowerShell, update WSL before
setting up the Linux environment:

```powershell
wsl --update
wsl --shutdown
```

Inside Ubuntu, install the compiler and GUI libraries:

```bash
sudo apt update
sudo apt install \
  build-essential pkg-config curl git libfontconfig1-dev \
  libxkbcommon-x11-0 libxkbcommon0 \
  libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev \
  libxkbcommon-dev libxkbcommon-x11-dev
```

Install Rust inside WSL rather than using a Windows Rust installation:

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source "$HOME/.cargo/env"
```

Install [nvm](https://github.com/nvm-sh/nvm) and Node.js 24 inside WSL. This
prevents WSL from invoking Windows `npm.cmd`, which fails when the current
directory is a `\\wsl.localhost\...` path:

```bash
curl -o- https://raw.githubusercontent.com/nvm-sh/nvm/v0.40.6/install.sh | bash
source ~/.bashrc
nvm install 24
nvm alias default 24
```

Confirm that Linux owns the active tools:

```bash
command -v cargo node npm
node --version
npm --version
```

The paths should be under `/home/...`, not `/mnt/c/...` or `C:\...`. The Slint
app automatically opens Windows-native file dialogs and the Windows browser
when it detects WSL. Build it in release mode for realistic export performance:

```bash
cargo run --release -p message-vault-io-gui
```

More Linux package details and optional helpers such as `ffmpeg` are documented
in [CONTRIBUTING.md](CONTRIBUTING.md).

## Supported exporters

| Backup | Converter |
|--------|-----------|
| Apple Messages (`chat.db`) | [`imessage-ir-exporter`](crates/exporters/imessage-ir-exporter) |
| SMS Backup & Restore (SyncTech XML) | [`sms-backup-restore-exporter`](crates/exporters/sms-backup-restore-exporter) |
| WhatsApp (native DB / crypt) | [`whatsapp-exporter`](crates/exporters/whatsapp-exporter) |

Experimental converters also ship in the GUI and release zip: GO SMS Pro, iMazing CSV, OpenExtract, and SMS Backup+. Use those when they are the only backup on hand. Details: the [docs site](https://bitrealm-dev.github.io/message-vault-io/) and [exporter capability matrix](docs/maintainers/exporter-matrix.md).

Already exported? The GUI **Format** tab ([`message-reexporter`](crates/message/reexport/docs/REEXPORT.md)) converts a prior output folder to another format (CSV ↔ EML ↔ MBOX ↔ JSON ↔ JSONL ↔ XML).

Import into Message Vault with the GUI **Vault** tab (JSONL export folder + Import API token). For a standalone `vault-push` CLI or exporter CLIs, use [message-exporters](https://github.com/bitrealm-dev/message-exporters) releases.

## Contributing

Setup, build, run, test, and contribution rules: [CONTRIBUTING.md](CONTRIBUTING.md).

## Releases

Prebuilt Linux (`.tgz`), Windows, and macOS Apple Silicon (`.zip`) archives — **GUI only** plus `lib/` (ffmpeg/ffprobe), `cli/wtsexporter`, and `licenses/`: [Releases](https://github.com/bitrealm-dev/message-vault-io/releases).

Maintainer documentation (architecture, GUI design, signing): [`docs/maintainers/`](docs/maintainers/README.md). Release steps: [Development and releases](docs/maintainers/developing.md).

## License

Most converters are MIT — see [LICENSE](LICENSE). `imessage-ir-exporter` is GPL-3.0-or-later (via `imessage-database` / `crabapple`).
