# Contributing

How to set up, build, run, and contribute to message-vault-io.

End-user guides (install, first export, formats) live on the [docs site](https://bitrealm-dev.github.io/message-vault-io/). Architecture, releases, signing, and GUI design notes live under [`docs/maintainers/`](docs/maintainers/README.md).

## Prerequisites

| Tool | Notes |
|------|--------|
| **Rust** | Stable toolchain via [rustup](https://rustup.rs/). This workspace uses Rust edition **2024**, which needs **Rust 1.85+**. CI builds with the latest stable. |
| **Windows** | [Visual Studio Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/) with the “Desktop development with C++” workload (MSVC). |
| **macOS** | Xcode Command Line Tools (`xcode-select --install`). |
| **Linux** | C toolchain plus GUI system libs (see [Linux packages](#linux-packages) below). |
| **Node.js 24** | Only if you edit the Astro Starlight docs under `docs/`. |

Optional for full WhatsApp / media features while developing: Python (`pip`) for `wtsexporter`, and `ffmpeg` / `ffprobe` on `PATH` (or see [Helper binaries](#helper-binaries-and-environment-variables)).

### Linux packages

The Slint desktop GUI needs a C toolchain, **fontconfig**, and X11 keyboard libs (Slint / winit at **runtime**). On Debian/Ubuntu:

```bash
sudo apt update
sudo apt install \
  build-essential pkg-config libfontconfig1-dev \
  libxkbcommon-x11-0 libxkbcommon0 \
  libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev \
  libxkbcommon-dev libxkbcommon-x11-dev
```

On Fedora:

```bash
sudo dnf install \
  gcc pkgconf-pkg-config fontconfig-devel \
  libxkbcommon libxkbcommon-x11 \
  libxkbcommon-devel libxcb-devel
```

`libxkbcommon-x11-0` provides `libxkbcommon-x11.so` — required to **run** `message-vault-io` even when the build succeeded.

## Clone and build

```bash
git clone https://github.com/bitrealm-dev/message-vault-io.git
cd message-vault-io
cargo build --workspace
```

The first build compiles every workspace crate and can take several minutes.

Release profile:

```bash
cargo build --workspace --release
```

Binaries that are only needed when packaging a release ZIP (not for day-to-day GUI work):

```bash
cargo build --release -p message-reexport --bin message-reexporter
cargo build --release -p vault-push --features cli
```

## Run the app

```bash
cargo run -p message-vault-io-gui
```

Use a release build when testing real exports. Debug builds compile faster, but parsing,
attachment hashing, and JSON serialization can be substantially slower:

```bash
cargo run --release -p message-vault-io-gui
```

Settings persist in `export.ini` (working directory or next to the binary). Template: [`crates/message-vault-io-gui/export.example.ini`](crates/message-vault-io-gui/export.example.ini). Backup passwords are never written.

Compile-time Slint style override (optional):

```bash
SLINT_STYLE=fluent cargo build -p message-vault-io-gui
```

## Helper binaries and environment variables

Most export work runs in-process as Rust libraries. A few features still shell out to sibling tools:

| Helper | Used for |
|--------|----------|
| `wtsexporter` | WhatsApp extract step |
| `ffmpeg` / `ffprobe` | Media convert / compress |

Lookup order: beside the current executable → `lib/` / `cli/` next to the GUI (or `../lib/` from `cli/`) → legacy one directory up → directory in `MESSAGE_VAULT_IO_BIN` → `PATH`. WhatsApp also accepts an explicit `WTSEXPORTER` path.

| Variable | Purpose |
|----------|---------|
| `MESSAGE_VAULT_IO_BIN` | Directory that contains helper binaries |
| `WTSEXPORTER` | Full path to the WhatsApp extractor |
| `SLINT_STYLE` | Slint widget style at compile time (e.g. `fluent`) |

Local options:

- Install WhatsApp helper: `pip install 'whatsapp-chat-exporter>=0.13'`
- Install system `ffmpeg` / `ffprobe`, or copy them from a [release ZIP](https://github.com/bitrealm-dev/message-vault-io/releases) next to your built GUI
- After `cargo build --workspace --release`, point helpers at the build output:

```powershell
# Windows PowerShell
$env:MESSAGE_VAULT_IO_BIN = "$PWD\target\release"
cargo run --release -p message-vault-io-gui
```

```bash
# Linux / macOS
export MESSAGE_VAULT_IO_BIN="$PWD/target/release"
cargo run --release -p message-vault-io-gui
```

## Test

```bash
cargo test --workspace
```

Run a single crate:

```bash
cargo test -p go-sms-pro-exporter
```

Exporter smoke tests under `crates/*/tests/convert_smoke.rs` use committed fixtures. You do not need personal phone backups to run the suite.

## Docs site (optional)

User-facing docs are Astro Starlight under `docs/`:

```bash
cd docs
npm ci
npm run dev
```

Before publishing doc changes: `npm run check` and `npm run build`.

CLI reference pages are generated from crate manpages. Edit `crates/<name>/docs/MANPAGE.md`, then:

```bash
cd docs
npm run sync:cli
npm run check
npm run build
```

Do not edit generated files under `docs/src/content/docs/reference/cli/` by hand. Release and GitHub Pages steps: [Development and releases](docs/maintainers/developing.md).

## Workspace map

- **Libraries:** under `crates/message/` — `ir`, `contacts`, `media`, `mail`, `sbr`, `phone`, `csv`, `obfuscate`; plus `message-vault-io-core`
- **Exporter crates:** under `crates/exporters/` — `imessage-ir-exporter`, `whatsapp-exporter`, `sms-backup-restore-exporter`, and experimental converters (GO SMS Pro, iMazing, OpenExtract, SMS Backup+)
- **GUI:** `message-vault-io-gui`
- **Utilities:** `vault-push`, `message-reexporter` (package `message-reexport`)

Most crates are MIT. `imessage-ir-exporter` is **GPL-3.0-or-later** (via `imessage-database` / `crabapple`). The GUI binary therefore includes GPL-licensed code.

## Contribution rules

1. **Keep changes focused.** Prefer small PRs that do one job over mixed refactors and features.
2. **Match existing style.** Follow patterns already used in nearby crates; do not add drive-by renames or unrelated cleanup.
3. **Verify before you open a PR.** At minimum: `cargo build --workspace` and `cargo test --workspace`. If you touched docs under `docs/`, also run `npm run check` there.
4. **No secrets or personal data.** Do not commit passwords, vault keys, certificates, `.env` files with credentials, or real message backups. Use fixtures under `crates/*/tests/fixtures/` for test data.
5. **Respect licenses.** Call out GPL implications when changing `imessage-ir-exporter` or anything that pulls it into new binaries.
6. **Document CLI changes in the crate manpage** (`crates/<name>/docs/MANPAGE.md`), then sync the docs site as above.
7. **Put design depth in maintainer docs**, not in this file. Architecture, format contracts, GUI option matrices, releases, and signing stay under [`docs/maintainers/`](docs/maintainers/README.md).

## Troubleshooting

| Symptom | What to try |
|---------|-------------|
| `Package 'fontconfig' not found` / `yeslogic-fontconfig-sys` panic | Install `libfontconfig1-dev` (Debian/Ubuntu) or `fontconfig-devel` (Fedora); see [Linux packages](#linux-packages) |
| `Library libxkbcommon-x11.so could not be loaded` | Install `libxkbcommon-x11-0` (Debian/Ubuntu) or `libxkbcommon-x11` (Fedora), then re-run |
| “Could not find wtsexporter / ffmpeg / ffprobe” | Install the helper, put it on `PATH`, or set `MESSAGE_VAULT_IO_BIN` / `WTSEXPORTER` |
| Windows linker / `link.exe` errors | Install MSVC Build Tools with the C++ desktop workload |
| Other GUI link / load errors on Linux | Install the packages under [Linux packages](#linux-packages) |

## Further reading

- [Maintainer documentation index](docs/maintainers/README.md)
- [Development and releases](docs/maintainers/developing.md)
- [Exporter capability matrix](docs/maintainers/exporter-matrix.md)
- [Code signing](docs/maintainers/signing.md)
- End-user docs: <https://bitrealm-dev.github.io/message-vault-io/>
