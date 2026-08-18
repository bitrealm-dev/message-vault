#!/usr/bin/env bash
# Host vault for day-to-day work from a git checkout.
#
#   ./scripts/run-vault-dev.sh                 # keep existing data/; empty vault if none
#   ./scripts/run-vault-dev.sh --reset         # wipe data/, start empty
#   ./scripts/run-vault-dev.sh --reset-demo    # wipe data/, seed sample inbox
#   ./scripts/run-vault-dev.sh --sqlweb        # SQLite browser on http://127.0.0.1:8081
#
# Website (separate terminal):
#   cd web && npm run dev          # http://localhost:5173, proxies /v1 here
#   cargo tauri dev                # desktop window, same Vite
#
# Debug profile so server-crate edits recompile quickly. Restart this process
# after Rust changes.
#
# Does not overwrite an existing config/config.toml except after reset-demo,
# which replaces that file with a demo config that has no [server] section.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
cd "${REPO_ROOT}"

CONFIG="config/config.toml"
CONFIG_EXAMPLE="config/config.toml.example"
DEMO=0
RESET=0
SQLWEB=0
SQLWEB_PID=""

usage() {
  cat <<EOF
Usage: $(basename "$0") [--reset | --reset-demo] [--sqlweb]

  --reset       Wipe data/ and start with an empty vault
  --reset-demo  Wipe data/ and seed the sample inbox
  --sqlweb      Start sqlite-web on http://127.0.0.1:8081 (needs sqlite_web on PATH)
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
    --sqlweb) SQLWEB=1 ;;
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
  # Loopback bind (no Docker port publish) and Vite/Tauri CORS.
  sed \
    -e 's/^# cors_origins =/cors_origins =/' \
    "${CONFIG_EXAMPLE}" >"${CONFIG}"
}

stop_sqlweb() {
  if [[ -n "${SQLWEB_PID}" ]]; then
    kill "${SQLWEB_PID}" 2>/dev/null || true
    wait "${SQLWEB_PID}" 2>/dev/null || true
    SQLWEB_PID=""
  fi
}

start_sqlweb() {
  require_cmd sqlite_web
  (
    echo "sqlite-web: waiting for data/vault.ready"
    while [[ ! -f data/vault.ready ]]; do
      sleep 1
    done
    echo "SQLite UI: http://127.0.0.1:8081"
    exec sqlite_web -H 127.0.0.1 -p 8081 -x data/vault.db
  ) &
  SQLWEB_PID=$!
  trap stop_sqlweb EXIT INT TERM
}

run_server() {
  echo "Starting message-vault-server (debug). Restart after server-crate edits."
  if [[ "${SQLWEB}" -eq 1 ]]; then
    cargo run -p message-vault-server -- serve --config "${CONFIG}"
  else
    exec cargo run -p message-vault-server -- serve --config "${CONFIG}"
  fi
}

wipe_data() {
  echo "Removing ${REPO_ROOT}/data/…"
  rm -rf data
  mkdir -p data
}

require_cmd cargo

mkdir -p data

if [[ ! -f "${CONFIG}" ]]; then
  echo "Writing ${CONFIG} from ${CONFIG_EXAMPLE} (CORS for :5173 enabled)."
  write_host_dev_config
fi

if [[ "${RESET}" -eq 1 || "${DEMO}" -eq 1 ]]; then
  wipe_data
fi

if [[ "${DEMO}" -eq 1 ]]; then
  require_cmd ffmpeg
  require_cmd ffprobe
  echo "Seeding demo data…"
  cargo run -p message-vault-server -- reset-demo --config "${CONFIG}"
  write_host_dev_config
  echo "Converting demo media…"
  cargo run -p message-vault-server -- process-assets --config "${CONFIG}" \
    || echo "warning: process-assets failed; UI still works"
elif [[ "${RESET}" -eq 1 ]]; then
  echo "Empty data/ (create an account in the web UI)."
elif [[ ! -f data/vault.db ]]; then
  echo "Empty data/ (pass --reset-demo to seed a sample inbox, or create an account in the web UI)."
else
  echo "Vault DB present; leaving it in place."
fi

echo
echo "Vault API:  http://127.0.0.1:8080"
echo "Website:    cd web && npm run dev     → http://localhost:5173"
echo "Desktop:    cargo tauri dev"
if [[ "${SQLWEB}" -eq 1 ]]; then
  echo "SQLite UI:  http://127.0.0.1:8081"
else
  echo "SQLite UI:  ./scripts/run-vault-dev.sh --sqlweb"
fi
echo

if [[ "${SQLWEB}" -eq 1 ]]; then
  start_sqlweb
fi

run_server
