#!/usr/bin/env bash
# Host vault against compose Postgres, from a git checkout.
#
#   ./scripts/run-vault-pg-dev.sh                 # start Postgres if needed; keep data
#   ./scripts/run-vault-pg-dev.sh --reset         # wipe volume + data/, empty vault
#   ./scripts/run-vault-pg-dev.sh --reset-demo    # wipe, seed sample inbox (demo / empty password)
#
# Website (separate terminal):
#   cd web && npm run dev
#   cargo tauri dev
#
# Stops the Postgres container when this script exits (volume stays).
# Do not run at the same time as ./scripts/run-vault-dev.sh (both use :8080).
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
cd "${REPO_ROOT}"

CONFIG="config/config.toml"
CONFIG_EXAMPLE="config/config.toml.example"
COMPOSE=(docker compose -f docker-compose.pg.yml)
DB_URL="postgres://vault:vault@127.0.0.1:5432/vault"
DEMO=0
RESET=0

usage() {
  cat <<EOF
Usage: $(basename "$0") [--reset | --reset-demo]

  --reset       Wipe the Postgres volume and data/, start empty
  --reset-demo  Wipe the Postgres volume and data/, seed the sample inbox
  -h, --help
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --reset) RESET=1 ;;
    --reset-demo) DEMO=1 ;;
    --demo)
      echo "error: --demo was renamed to --reset-demo (always wipes data/ and reseeds)" >&2
      exit 1
      ;;
    --sqlweb)
      echo "error: --sqlweb is only for ./scripts/run-vault-dev.sh (SQLite)" >&2
      exit 1
      ;;
    -h | --help)
      usage
      exit 0
      ;;
    *)
      echo "error: unknown argument: $1" >&2
      usage >&2
      exit 1
      ;;
  esac
  shift
done

if [[ "${RESET}" -eq 1 && "${DEMO}" -eq 1 ]]; then
  echo "error: use either --reset or --reset-demo, not both" >&2
  exit 1
fi

require_cmd() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "error: '$1' not found on PATH" >&2
    exit 1
  fi
}

write_host_dev_config() {
  mkdir -p config data
  if [[ ! -f "${CONFIG_EXAMPLE}" ]]; then
    echo "error: missing ${CONFIG_EXAMPLE}" >&2
    exit 1
  fi
  sed \
    -e 's/^# cors_origins =/cors_origins =/' \
    "${CONFIG_EXAMPLE}" >"${CONFIG}"
}

stop_postgres() {
  "${COMPOSE[@]}" down
}

wait_postgres() {
  local i
  for i in $(seq 1 30); do
    if "${COMPOSE[@]}" exec -T postgres pg_isready -U vault >/dev/null 2>&1; then
      return 0
    fi
    sleep 1
  done
  echo "error: Postgres did not become ready on 127.0.0.1:5432" >&2
  exit 1
}

require_cmd cargo
require_cmd docker

mkdir -p data

if [[ ! -f "${CONFIG}" ]]; then
  echo "Writing ${CONFIG} from ${CONFIG_EXAMPLE} (CORS for :5173 enabled)."
  write_host_dev_config
fi

if [[ "${RESET}" -eq 1 || "${DEMO}" -eq 1 ]]; then
  echo "Removing Postgres volume vault_pg_data and ${REPO_ROOT}/data/…"
  "${COMPOSE[@]}" down -v
  rm -rf data
  mkdir -p data
fi

trap stop_postgres EXIT INT TERM

echo "Starting Postgres (docker compose -f docker-compose.pg.yml)…"
"${COMPOSE[@]}" up -d
wait_postgres

if [[ "${DEMO}" -eq 1 ]]; then
  require_cmd ffmpeg
  require_cmd ffprobe
  echo "Seeding demo data into Postgres…"
  cargo run -p message-vault-server -- reset-demo --config "${CONFIG}" --db-url "${DB_URL}"
  write_host_dev_config
elif [[ "${RESET}" -eq 1 ]]; then
  echo "Empty Postgres (create an account in the web UI)."
else
  echo "Postgres volume present; leaving it in place."
fi

echo
echo "Vault API:  http://127.0.0.1:8080  (Postgres postgres://127.0.0.1:5432/vault)"
echo "Website:    cd web && npm run dev     → http://localhost:5173"
echo "Desktop:    cargo tauri dev"
echo "Stop:       Ctrl+C also stops the Postgres container (volume kept)."
echo

echo "Starting message-vault-server (debug). Restart after server-crate edits."
cargo run -p message-vault-server -- serve --config "${CONFIG}" --db-url "${DB_URL}"
