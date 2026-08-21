# message-vault-server

HTTP API and SQLite storage for browsing imported messages. This is the vault: import, export, contacts, search, auth, and attachment endpoints. It also has CLI subcommands (`serve`, `import`, `reset-demo`, and others).

The Vite SPA in `web/` is the website this server can host. Docker images wrap this crate.

## Build and test

```bash
cargo test -p message-vault-server
cargo run --release -p message-vault-server -- serve
```

Docker (release-shaped image from this checkout): `docker compose -f docker/compose.release.yml up --build`. Day-to-day from a clone: `./scripts/run-vault-dev.sh` (see [CONTRIBUTING.md](../../../CONTRIBUTING.md)). Published image: [Try the vault](https://bitrealm.io/vault/user/get-started/try-the-vault/).

## Docs

- Try the vault: https://bitrealm.io/vault/user/get-started/try-the-vault/
- Operator Docker: https://bitrealm.io/vault/developer/docker-compose/
- Server CLI: https://bitrealm.io/vault/developer/reference/server-cli/
- API: https://bitrealm.io/vault/developer/reference/api/

## License

AGPL-3.0. See the repository root `LICENSE`.
