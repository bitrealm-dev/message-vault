# Message Vault

Extract messages from phone backups, import them into a local vault, and browse them in an interface you control.

Message Vault has two parts that work together:

- **The vault** — a Docker container running a REST API and SQLite database. It stores your messages and serves them through a web interface you open in your browser.
- **The desktop app** — a Tauri desktop application that extracts messages from Apple and Android phone backups, converts them between formats, and imports them into the vault.

Your messages stay on your own machine — nothing is uploaded to a cloud service, and no account is required.

## Docs

Read the full guide (install, desktop app, supported backups, formats, API):

**https://bitrealm.dev/**

Source Markdown lives in [`docs/src/content/docs/`](docs/src/content/docs/). GitHub Pages deploys from this repository (`.github/workflows/docs.yml`).

## Quick start

**Desktop app:** Download the platform archive from the latest [Release](https://github.com/bitrealm-dev/message-vault/releases). Extract it and keep every file in the same folder. Run `message-vault`.

**Vault server (Docker):**

```bash
docker run -d --name message-vault \
  -p 8080:8080 \
  -e VAULT_MODE=demo \
  -v message-vault-data:/app/data \
  mbeisser1/message-vault:latest
```

Open **http://localhost:8080** and sign in with username `demo` and an empty password. The web UI and API share that origin.

**From source (desktop app):**

```bash
cargo install tauri-cli --version "^2"
cd web && npm ci && cd ..
cargo tauri dev
```

### WSL2 development

Use WSL2 with WSLg enabled and keep the repository in the Linux filesystem (`~/repo/...`), not under `/mnt/c`. From Windows PowerShell, update WSL before setting up the Linux environment:

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
  libxkbcommon-dev libxkbcommon-x11-dev libdbus-1-dev 
```

Install Rust inside WSL rather than using a Windows Rust installation:

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source "$HOME/.cargo/env"
```

Install [nvm](https://github.com/nvm-sh/nvm) and Node.js 24 inside WSL. This prevents WSL from invoking Windows `npm.cmd`, which fails when the current directory is a `\\wsl.localhost\...` path:

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

The paths should be under `/home/...`, not `/mnt/c/...` or `C:\...`. Build in release mode for realistic export performance:

```bash
cd web && npm ci && cd ..
cargo tauri dev
```

More Linux package details and optional helpers such as `ffmpeg` are documented in [CONTRIBUTING.md](CONTRIBUTING.md).

## Supported backups

| Backup | Converter |
|--------|-----------|
| Apple Messages (`chat.db`) | `imessage-ir-exporter` |
| SMS Backup & Restore (SyncTech XML) | `sms-backup-restore-exporter` |
| WhatsApp (native DB / crypt) | `whatsapp-exporter` |

Experimental converters also ship in the desktop app: GO SMS Pro, iMazing CSV, OpenExtract, and SMS Backup+. Use those when they are the only backup on hand. Details: the [docs site](https://bitrealm.dev/) and [exporter capability matrix](docs/maintainers/exporter-matrix.md).

Already exported? Use **Format** in the desktop app (available from the login screen without signing in) to convert a prior output folder to another format (CSV ↔ EML ↔ MBOX ↔ JSON ↔ JSONL ↔ XML).

Import into Message Vault with the desktop app **Import** screen after signing in (or `vault-push` with an Import API token from Settings → Profile). Export a copy back to disk with **Export** or `vault-pull`. For standalone CLI tools, build from source in this repo.

## Contributing

Setup, build, run, test, and contribution rules: [CONTRIBUTING.md](CONTRIBUTING.md).

## Releases

Prebuilt Linux (`.tgz`), Windows, and macOS Apple Silicon (`.zip`) archives — **GUI only** plus `lib/` (ffmpeg/ffprobe), `cli/wtsexporter`, and `licenses/`: [Releases](https://github.com/bitrealm-dev/message-vault/releases).

Maintainer documentation (architecture, GUI design, signing): [`docs/maintainers/`](docs/maintainers/README.md). Release steps: [Development and releases](docs/maintainers/developing.md).

## License

Most converters are MIT — see [LICENSE](LICENSE). `imessage-ir-exporter` is GPL-3.0-or-later (via `imessage-database`).
