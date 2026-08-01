---
title: Try the demo
description: Load the committed sample vault and browse it in the web UI.
---

No real phone backup needed. The repository includes a demo iMessage JSONL
bundle you can import in one step.

## Linux / macOS

```bash
./scripts/setup-demo.sh
cd web && npm ci && npm run dev
```

## Windows (PowerShell)

```powershell
cargo build --workspace --release
New-Item -ItemType Directory -Force .\data | Out-Null
cargo run --release -- reset-demo
cargo run --release -- process-assets

Set-Location .\web
npm ci
npm run dev
```

## Sign in

Open <http://localhost:3000/login>. Enter username **`demo`** with an empty
password (or create another account). You should see demo contacts and
conversations.

## Reset demo data

Reset is **CLI only**. The web menu shows a hint but does not run the reset:

```bash
cargo run --release -- reset-demo
```

`reset-demo` overwrites `config/config.toml` with the demo config, which has
`[server]` commented out. Before using remote import (`serve`), restore
[`config/config.toml.example`](https://github.com/bitrealm-dev/message-vault-rs/blob/main/config/config.toml.example)
or uncomment `[server]`.

## Next

- [Browse the vault](/browse/navigation-and-sources/)
- [Import your own messages](/get-started/first-personal-import/)
