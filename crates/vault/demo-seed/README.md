# demo-seed

Generate synthetic JSON Lines conversations for the demo vault so the website can be clicked through without a real phone backup.

`message-vault-server` `reset-demo` uses this crate. Config lives in `demo_seed.toml`. Names and corpus files are under `data/`.

## Build and test

```bash
cargo test -p demo-seed
cargo run -p demo-seed
```

Regenerate and import in one step:

```bash
cargo run --release -p message-vault-server -- reset-demo
```

Workspace setup: [CONTRIBUTING.md](../../../CONTRIBUTING.md). Demo walkthrough: https://bitrealm.dev/get-started/try-the-vault/

## License

MIT.
