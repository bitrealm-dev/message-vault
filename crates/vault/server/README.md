# message-vault-server

HTTP API and SQLite storage for browsing imported messages. This is the vault: import, export, contacts, search, auth, and attachment endpoints. It also has CLI subcommands (`serve`, `import`, `reset-demo`, and others).

The Vite SPA in `web/` is the website this server can host. Docker images wrap this crate.

## Build and test

```bash
cargo test -p message-vault-server
cargo run --release -p message-vault-server -- serve
```

Docker: `docker compose up` from the repository root (website and API on http://localhost:8080). Workspace setup: [CONTRIBUTING.md](../../../CONTRIBUTING.md).

## Docs

- Try the vault: https://bitrealm.dev/get-started/try-the-vault/
- Operator Docker: https://bitrealm.dev/developer/docker-compose/
- Server CLI: https://bitrealm.dev/reference/server-cli/
- API: https://bitrealm.dev/reference/api/

## License

MIT.
