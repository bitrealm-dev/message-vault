#!/usr/bin/env bash
# Release profile entrypoint: seed if needed, then serve static + API.
set -euo pipefail

cd /app

CONFIG_DOCKER="config/config.docker.toml"
CONFIG="config/config.toml"
VAULT_MODE="${VAULT_MODE:-demo}"

export VAULT_DB="${VAULT_DB:-/app/data/vault.db}"
export VAULT_DATA_DIR="${VAULT_DATA_DIR:-/app/data}"

ensure_docker_config() {
  mkdir -p config data
  cp "${CONFIG_DOCKER}" "${CONFIG}"
}

seed_if_needed() {
  if [[ -f data/vault.db ]]; then
    echo "Vault DB present; skipping seed (VAULT_MODE=${VAULT_MODE})."
    ensure_docker_config
    return
  fi

  ensure_docker_config

  case "${VAULT_MODE}" in
    demo)
      echo "Seeding demo vault…"
      message-vault-rs reset-demo --config "${CONFIG}"
      ensure_docker_config
      echo "Converting demo media…"
      message-vault-rs process-assets --config "${CONFIG}" \
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

seed_if_needed

echo "Starting message-vault-rs (API + static files)…"
exec message-vault-rs serve --config "${CONFIG}"
