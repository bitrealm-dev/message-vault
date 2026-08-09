#!/usr/bin/env bash
# Dev profile entrypoint: seed data if needed, then cargo run -- serve.
set -euo pipefail

cd /app

CONFIG_DOCKER="config/config.docker.toml"
CONFIG="config/config.toml"
VAULT_MODE="${VAULT_MODE:-demo}"

ensure_docker_config() {
  mkdir -p config data
  if [[ ! -f "${CONFIG_DOCKER}" ]]; then
    echo "error: missing ${CONFIG_DOCKER} (bind-mount the repo root)" >&2
    exit 1
  fi
  cp "${CONFIG_DOCKER}" "${CONFIG}"
}

seed_if_needed() {
  if [[ -f data/vault.db ]]; then
    echo "Vault DB present; skipping seed (VAULT_MODE=${VAULT_MODE})."
    ensure_docker_config
    return
  fi

  ensure_docker_config
  mkdir -p data

  case "${VAULT_MODE}" in
    demo)
      echo "Seeding demo vault…"
      cargo run --release -p message-vault-server -- reset-demo --config "${CONFIG}"
      ensure_docker_config
      echo "Converting demo media…"
      cargo run --release -p message-vault-server -- process-assets --config "${CONFIG}" \
        || echo "warning: process-assets failed; UI still works"
      ;;
    personal)
      echo "Personal mode: empty data/ (create an account in the web UI)."
      ;;
    *)
      echo "error: VAULT_MODE must be 'demo' or 'personal' (got '${VAULT_MODE}')" >&2
      exit 1
      ;;
  esac
}

# Link Vite build output if available (built externally via npm run dev or npm run build)
if [[ -d /app/static ]]; then
  echo "Static files found at /app/static"
else
  echo "Note: no /app/static directory — create a symlink to your Vite build:"
  echo "  ln -s /path/to/web/dist /app/static"
  mkdir -p /app/static
fi

seed_if_needed

echo "Starting message-vault-server (API + static files)…"
exec cargo run --release -p message-vault-server -- serve --config "${CONFIG}"
