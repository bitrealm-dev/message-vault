---
title: Release
description: "How Message Vault versions ship: version lockstep, git tags, and the artifacts CI builds."
---

Releasing is a maintainer task. One product version ([Semantic Versioning](https://semver.org/spec/v2.0.0.html): `MAJOR.MINOR.PATCH`) ships as two artifacts:

- The vault image `bitrealm/message-vault:<version>` on Docker Hub (also `<major>.<minor>`, `latest`, and `sha-…`). The Docker tag has no `v` prefix (`0.8.0`, not `v0.8.0`).
- Unsigned desktop installers on [GitHub Releases](https://github.com/bitrealm-io/message-vault/releases): Linux `.deb` and AppImage, Windows `.msi`, macOS `.dmg`.

Nothing is published to npm or PyPI. Pushing the git tag `v<version>` is what runs the release jobs. A merge to `main` does not ship.

The JSONL schema version 3 is independent of the product version. Leave other `Cargo.toml` files at `0.1.0`, and don't bump `crates/message-vault-io-gui/` or `web-next/` for a product release.

## Before tagging

1. Merge the work to `main`. Wait until CI on `main` is green (`fmt`, workspace tests, `web` tests). `./scripts/check-pr.sh` is optional locally.
2. Move `[Unreleased]` entries in `CHANGELOG.md` under the new version heading ([Keep a Changelog](https://keepachangelog.com/en/1.1.0/)).
3. Set these four files to the same number (for example `0.8.0`):
   - `src-tauri/Cargo.toml`
   - `src-tauri/tauri.conf.json`
   - `web/package.json`
   - `crates/vault/server/Cargo.toml`
4. Commit and push that bump on `main`.
5. Tag that commit `v0.8.0` and push the tag.

## After tagging

GitHub Actions builds the image and the installers and opens a GitHub Release named `Message Vault v0.8.0`. The installers are not code-signed, so users may see SmartScreen or Gatekeeper warnings.

Don't create or push a tag unless a release should ship.
