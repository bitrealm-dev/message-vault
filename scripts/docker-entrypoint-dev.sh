#!/usr/bin/env bash
# Dev profile entrypoint: seed data if needed, then cargo run -- serve + next dev.
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

# Named volume mounts create an empty web/node_modules dir, so "-d" is not enough.
# tsx is an npm dependency (web/package.json), not a system binary.
web_deps_ready() {
  [[ -f web/node_modules/tsx/dist/cli.mjs ]] && [[ -d web/node_modules/next ]]
}

install_web_deps() {
  if web_deps_ready; then
    return
  fi
  echo "Installing web dependencies…"
  (cd web && npm ci)
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
      cargo run --release -- reset-demo --config "${CONFIG}"
      # reset-demo installs demo config without [server]; restore docker bind.
      ensure_docker_config
      install_web_deps
      if ! web_deps_ready; then
        echo "error: web deps missing after npm ci (tsx/next not found)" >&2
        exit 1
      fi
      echo "Converting demo media…"
      (cd web && npm run process-assets) || echo "warning: process-assets failed; UI still works"
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

install_web_deps
seed_if_needed

echo "Starting import API (cargo run -- serve)…"
cargo run --release -- serve --config "${CONFIG}" &
SERVE_PID=$!

cleanup() {
  kill "${SERVE_PID}" 2>/dev/null || true
}
trap cleanup EXIT INT TERM

echo "Starting Next.js (npm run dev)…"
cd web
exec npm run dev -- --hostname 0.0.0.0 --port 3000
