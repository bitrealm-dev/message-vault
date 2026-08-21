# Development and releases

For local setup and build, see [Contributing](https://bitrealm.io/vault/developer/contributing/) (repo pointer: [CONTRIBUTING.md](../../CONTRIBUTING.md)).

End-user documentation lives in the [Starlight source](../src/content/docs/) (start with [Export structure](../src/content/docs/vault/developer/reference/export-structure.md)). Use the [maintainer index](README.md) to find architecture, GUI, exporter, and format documentation.

## Cutting a release

> **Retired process.** Everything in this section describes the Slint GUI
> (`crates/message-vault-io-gui`), which is deprecated, and a
> `.github/workflows/release.yml` that no longer exists. Releases now come from
> pushing a `v*` tag, which makes [`.github/workflows/ci.yml`](../../.github/workflows/ci.yml)
> build the Docker image and the Tauri desktop installers. Bump `version` in
> `src-tauri/Cargo.toml` before tagging. The text below is kept for reference
> while the Slint GUI is still in the workspace.

Prebuilt archives are published only by a **manual** GitHub Actions workflow. Nothing builds or releases on push, PR, or tag by default.

Workflow file: [`.github/workflows/release.yml`](../../.github/workflows/release.yml)

Packaging script: [`scripts/deprecated/package-release.sh`](../../scripts/deprecated/package-release.sh)

### Version

The release version is **`crates/message-vault-io-gui/Cargo.toml` → `version`**. That value names the GitHub tag (`v0.4.0`), archive filenames, and the GUI window title (`CARGO_PKG_VERSION`). Bump it on `main` (or your release branch) before running the workflow. There is no separate version input in Actions.

### Steps

1. Bump `version` in [`crates/message-vault-io-gui/Cargo.toml`](../../crates/message-vault-io-gui/Cargo.toml) and merge whatever should ship onto `main` (or the branch you intend to build; the workflow checks out the branch you select when you run it).
2. Open [Actions → Release](https://github.com/bitrealm-dev/message-vault/actions/workflows/release.yml).
3. Click **Run workflow**.
4. Choose the branch to build from (usually `main`).
5. Wait for all three OS jobs (Linux, Windows, macOS) to finish and for the release job to create the GitHub Release.
6. Confirm the release at [Releases](https://github.com/bitrealm-dev/message-vault/releases). The tag will be `v` plus the Cargo.toml version (`0.4.0` → `v0.4.0`). The workflow fills release notes (Highlights, Upgrade notes, Archives, Layout) from [`.github/workflows/release.yml`](../../.github/workflows/release.yml); edit that `--notes` block when the next cut needs different copy.

You need write access to the repository (to run workflows that create releases and tags).

### What gets published

Exactly **three** platform archives (no loose individual executables):

| Archive | Runner |
|---------|--------|
| `message-vault-io-<version>-x86_64-unknown-linux-gnu.tgz` | `ubuntu-latest` |
| `message-vault-io-<version>-x86_64-pc-windows-msvc.zip` | `windows-latest` |
| `message-vault-io-<version>-aarch64-apple-darwin.zip` | `macos-latest` (Apple Silicon) |

Layout (same on every platform):

**Root**

- `message-vault` (`.exe` on Windows) — runs exporters, Contacts, Format, and Vault as **libraries** (linked into the GUI; no Rust exporter CLIs in this archive)

**`lib/` — media tools**

- `ffmpeg` / `ffprobe` — eugeneware/ffmpeg-static `b6.1.1` (binaries report FFmpeg `7.0.2-static`)

**`cli/` — WhatsApp helper only**

- `wtsexporter` — KnugiHK WhatsApp-Chat-Exporter `0.13.0` (pinned + SHA-256 in `scripts/deprecated/package-release.sh`)

**`licenses/`**

- `LICENSE`, `THIRD_PARTY_NOTICES.md`, `THIRD_PARTY_WTSEXPORTER.LICENSE`, `THIRD_PARTY_FFMPEG.LICENSE`

Standalone exporter CLIs (`*-exporter`, `message-reexport`, `vault-push`) ship from
[message-exporters](https://github.com/bitrealm-dev/message-exporters) releases, not
this product. The GUI finds `ffmpeg`/`ffprobe` under `lib/` and `wtsexporter` under
`cli/`. Keep the extracted archive together.

### Code signing

Windows Authenticode and macOS codesign / notarization steps are already in the Release workflow but stay skipped until certificate secrets are set. See [Code signing for Windows and macOS releases](signing.md).

### Local packaging smoke test

```bash
cargo build --release -p message-vault-io-gui
scripts/deprecated/package-release.sh 0.0.0-dev x86_64-unknown-linux-gnu
tar -tzf dist/message-vault-io-0.0.0-dev-x86_64-unknown-linux-gnu.tgz | head
```

Re-running the workflow when that Cargo.toml version already has a tag/release will fail at `gh release create`. Bump `message-vault-io-gui`’s `version` (or delete the old release/tag) if you intentionally want to replace it.

### Notifications

The workflow does not send email itself. GitHub may still email you about failed (or successful) Actions runs based on your account settings.

To quiet that: [Notification settings](https://github.com/settings/notifications) → **Actions** → turn off the emails you do not want. That is account-level; it cannot be forced from the workflow YAML.

## Documentation site (GitHub Pages)

User-facing docs use [Astro Starlight](https://starlight.astro.build/) under [`docs/`](..), deployed by [`.github/workflows/docs.yml`](../../.github/workflows/docs.yml).

### Enable Pages (one-time)

1. Repo **Settings → Pages**.
2. **Build and deployment → Source** → **GitHub Actions** (not “Deploy from a branch”).
3. Push to `main` or run the **Docs** workflow under **Actions**.
4. Site URL: `https://bitrealm.io/vault/user/`.

Local preview:

```bash
cd docs
npm ci
npm run dev
```

Run `npm run check` and `npm run build` before publishing documentation changes.
