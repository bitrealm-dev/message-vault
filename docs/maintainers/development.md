# Developer setup

End-user documentation is published at
<https://bitrealm-dev.github.io/message-vault-rs/> (source under
[`../src/content/docs/`](../src/content/docs/)). This page is for contributors
setting up a local workspace.

Message Vault has two local components:

- a Rust workspace for importing, storing, and serving message data;
- a Next.js application in `web/` for browsing the SQLite vault.

Commands below assume the repository root is the current directory unless noted.

## Requirements

- Rust 1.95 or newer (edition 2024; `rusqlite` 0.40 needs `cfg_select!`)
- Node.js 20.9 or newer and npm
- A native C/C++ build toolchain
- Optional: FFmpeg for video/audio conversion and media format fallbacks
- Optional: Docker / Compose — see
  [`docs/src/content/docs/get-started/docker.md`](../src/content/docs/get-started/docker.md)
  (`docker compose up` for the default bind-mounted **dev** profile)

Verify the installed tools:

```text
rustc --version
cargo --version
node --version
npm --version
ffmpeg -version
```

FFmpeg may be omitted if you do not need video/audio conversion.

## Windows

### Install prerequisites

Install Visual Studio 2022 or Visual Studio Build Tools 2022 with the
**Desktop development with C++** workload and a Windows SDK.

The remaining tools can be installed from PowerShell with `winget`:

```powershell
winget install --id Rustlang.Rust.MSVC -e
winget install --id OpenJS.NodeJS.LTS -e
winget install --id Gyan.FFmpeg -e
```

Restart PowerShell after installation so the updated `PATH` is available.
Rust should use the `x86_64-pc-windows-msvc` toolchain.

### Run the demo

The repository's `scripts/setup-demo.sh` helper requires Bash. The equivalent
native PowerShell setup is:

```powershell
Set-Location C:\path\to\message-vault-rs

cargo build --workspace --release
New-Item -ItemType Directory -Force .\data | Out-Null
cargo run --release -- reset-demo

cargo run --release -- process-assets

Set-Location .\web
npm ci
npm run dev
```

Open <http://localhost:3000/login>. Sign in as username **`demo`** with an empty
password (or create another account). Keep the final PowerShell window running
while using the application.

`reset-demo` overwrites `config/config.toml` with the demo config, which has
`[server]` commented out. Before running `cargo run --release -- serve` for
remote import, restore `config/config.toml.example` (or uncomment `[server]`).

The repository's `.cargo/config.toml` gives Windows release binaries a larger
stack. This is needed because the default Windows stack can overflow while
importing the bundled demo.

## Linux

### Install prerequisites

On Debian or Ubuntu, install the native build dependencies:

```bash
sudo apt update
sudo apt install -y build-essential curl pkg-config python3 ffmpeg
```

Install Rust through [rustup](https://rustup.rs/):

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source "$HOME/.cargo/env"
```

Install Node.js 20.9 or newer using your distribution's supported Node.js
package, [nvm](https://github.com/nvm-sh/nvm), or another Node version manager.
Distribution repositories on older Linux releases may provide a Node version
that is too old for Next.js 16.

### Run the demo

```bash
./scripts/setup-demo.sh
cd web
npm ci
npm run dev
```

Open <http://localhost:3000/login>. Sign in as username **`demo`** with an empty
password (or create another account). Keep the development server running while
using the application.

`setup-demo.sh` runs `reset-demo`, which overwrites `config/config.toml` and
leaves `[server]` disabled until you restore the example config.

## Run with personal data

First create local configuration files. They are gitignored.

PowerShell:

```powershell
Copy-Item .\config\config.toml.example .\config\config.toml
Copy-Item .\config\contacts.csv.example .\config\contacts.csv
Copy-Item .\config\exclude.csv.example .\config\exclude.csv
```

Linux:

```bash
cp config/config.toml.example config/config.toml
cp config/contacts.csv.example config/contacts.csv
cp config/exclude.csv.example config/exclude.csv
```

Edit `config/config.toml`:

1. Adjust `[paths]` for the local machine.
2. Ensure `[server]` is present (the example file enables it).
3. Leave `bind = "127.0.0.1:8080"` for local-only access.
4. Create a web account and generate a Vault Import API token from
   **Settings → Access** for `vault-push` (copy it from the one-time dialog).

Source names are not listed in TOML — each import/upload supplies its own
source id (asset folders under `data/<account_id>/<source_id>/`). Runtime
contacts live at `data/<account_id>/contacts.csv` and `exclude.csv` (created
with empty headers on first use). The files under `config/` are optional
templates only.

Start the import API from the repository root:

```text
cargo run --release -- serve
```

In a second terminal, start the web application:

```text
cargo run --release -- process-assets
cd web
npm ci
npm run dev
```

Open <http://localhost:3000>, create an account, and generate a Vault Import
API token under **Settings → Access** (copy it from the one-time dialog).
Keep the import API running while pushing a message-ir export from
[message-exporters](https://bitrealm-dev.github.io/message-exporters/)
(`message-exporter` Vault tab or `cli/vault-push`).

New accounts start in read-only mode for the web UI. Turn that off in Settings
when you need to edit contacts or trash items. CLI and HTTP imports still work
while read-only is enabled.

## Common checks

Build and test the Rust workspace:

```text
cargo build --workspace
cargo test --workspace
```

Check the web application:

```text
cd web
npm run lint
npm test
npm run build
```

Verify a running local instance:

```text
http://localhost:3000/login
http://127.0.0.1:8080/health
```

## Troubleshooting

### `cargo` or `node` is not recognized on Windows

Close and reopen PowerShell after installing the tools. If Rust was installed
with rustup, confirm `%USERPROFILE%\.cargo\bin` is on `PATH`.

### MSVC linker errors

Modify the Visual Studio installation and add **Desktop development with C++**.
The Rust MSVC toolchain needs the MSVC linker and Windows SDK.

### `unable to open database file`

Create the configured database parent directory. With the default config:

```powershell
New-Item -ItemType Directory -Force .\data | Out-Null
```

```bash
mkdir -p data
```

Then rerun `cargo run --release -- reset-demo`.

### `serve` fails with missing `[server]`

`reset-demo` installs the demo config, which comments out `[server]`. Copy
`config/config.toml.example` again (or uncomment `[server]` and set `bind`),
then rerun `cargo run --release -- serve`.

### Media conversion is skipped or fails

Confirm FFmpeg is available with `ffmpeg -version`, then rerun:

```text
cargo run --release -- process-assets --force
```

The web UI still works without derived media, but some video, audio, or HEIC
content may not be converted for browser playback.
