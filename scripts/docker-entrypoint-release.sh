#!/usr/bin/env bash
# Release profile entrypoint: seed if needed, then serve + next start.
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
      if [[ -x web/node_modules/.bin/tsx ]]; then
        echo "Converting demo media…"
        (cd web && ./node_modules/.bin/tsx scripts/process-assets.ts) \
          || echo "warning: process-assets failed; UI still works"
      else
        echo "warning: web tooling missing; skip process-assets"
      fi
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

echo "Starting import API (message-vault-rs serve)…"
message-vault-rs serve --config "${CONFIG}" &
SERVE_PID=$!

cleanup() {
  kill "${SERVE_PID}" 2>/dev/null || true
}
trap cleanup EXIT INT TERM

# Standalone output from a web/ sub-app lives at /app/web/server.js when the
# monorepo root is the Docker WORKDIR during `next build`. Fall back to /app/server.js.
if [[ -f web/server.js ]]; then
  SERVER_JS="web/server.js"
elif [[ -f server.js ]]; then
  SERVER_JS="server.js"
else
  echo "error: Next.js standalone server.js not found under /app" >&2
  ls -la /app /app/web 2>/dev/null || true
  exit 1
fi

echo "Starting Next.js (${SERVER_JS})…"
export HOSTNAME="${HOSTNAME:-0.0.0.0}"
export PORT="${PORT:-3000}"
exec node "${SERVER_JS}"
