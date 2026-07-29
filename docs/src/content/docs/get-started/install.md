---
title: Install
description: Requirements and tools for running Message Vault locally.
---

Message Vault has two local components:

- a **Rust** workspace for importing, storing, and serving message data
- a **Next.js** app in `web/` for browsing the SQLite vault

## Requirements

- Rust 1.85 or newer (edition 2024)
- Node.js 20.9 or newer and npm
- A native C/C++ build toolchain
- Optional: FFmpeg for video/audio/HEIC conversion (`npm run process-assets`)

Verify:

```text
rustc --version
cargo --version
node --version
npm --version
ffmpeg -version
```

## Clone and build

```bash
git clone https://github.com/bitrealm-dev/message-vault-rs.git
cd message-vault-rs
cargo build --workspace --release
```

For the web UI:

```bash
cd web
npm ci
```

## Next steps

- No real backup yet? [Try the demo](/get-started/try-the-demo/).
- Ready to import your own messages? [First personal import](/get-started/first-personal-import/).

Full Windows and Linux prerequisite install steps (Visual Studio workloads,
`winget`, `apt`, troubleshooting) live in the in-repo maintainer guide:
[`docs/maintainers/development.md`](https://github.com/bitrealm-dev/message-vault-rs/blob/main/docs/maintainers/development.md).
