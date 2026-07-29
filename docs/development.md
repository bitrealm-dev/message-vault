# Developer setup

Message Vault has two local components:

- a Rust workspace for importing, storing, and serving message data;
- a Next.js application in `web/` for browsing the SQLite vault.

Commands below assume the repository root is the current directory unless noted.

## Requirements

- Rust 1.85 or newer (the workspace uses Rust 2024 edition)
- Node.js 20.9 or newer and npm
- A native C/C++ build toolchain
- Optional: FFmpeg for video/audio conversion and media format fallbacks

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

Set-Location .\web
npm ci
npm run process-assets
npm run dev
```

Open <http://localhost:3000>. Keep the final PowerShell window running while
using the application.

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
npm run process-assets
npm run dev
```

Open <http://localhost:3000>. Keep the development server running while using
the application.

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
2. Uncomment `[server]` (or keep it if already present).
3. Leave `bind = "127.0.0.1:8080"` for local-only access.
4. Create a web account and use its Import API token from Settings for `vault-push`.

Source names are not listed in TOML — each import/upload supplies its own
source id (asset folders under `data/<account_id>/<source_id>/`).

Start the import API from the repository root:

```text
cargo run --release -- serve
```

In a second terminal, start the web application:

```text
cd web
npm ci
npm run process-assets
npm run dev
```

Open <http://localhost:3000>, create an account, and copy its Import API token
from **Settings**. Keep the import API running while pushing JSONL from
[message-exporters](https://github.com/bitrealm-dev/message-exporters).

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

### Media conversion is skipped or fails

Confirm FFmpeg is available with `ffmpeg -version`, then rerun:

```text
cd web
npm run process-assets -- --force
```

The web UI still works without derived media, but some video, audio, or HEIC
content may not be converted for browser playback.
