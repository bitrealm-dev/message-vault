#!/usr/bin/env bash
# import-staging.sh — import JSONL from staging/ into the vault DB
#
# Usage:
#   ./scripts/import-staging.sh --account <username> --source imessage
#   ./scripts/import-staging.sh --account <username> --append --source go-sms-pro
#   ./scripts/import-staging.sh --account <username> --overwrite-contacts --source imessage
#
# Modes:
#   replace (default) — delete that source's messages, then import
#   --append          — keep existing; dedupe by (source, guid)
#
# After import, runs `dedupe-cross-source` to soft-hide the same SMS across sources.
# Default staging path: staging/<source_id>/

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
CONFIG="${REPO_ROOT}/config/config.toml"

MODE="replace"
OVERWRITE_CONTACTS=0
ACCOUNT=""
SOURCES=()

usage() {
  cat <<'EOF'
Usage: import-staging.sh --account <username> --source <id> [OPTIONS] [--source <id>…]

Options:
  --account <username>   Vault account username or UUID (required)
  --source <id>          Source slug to import (required; repeatable)
  --append               Import mode append (default: replace)
  --overwrite-contacts   Reload contacts CSV on the first import
  -h, --help             Show this help
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --account)
      ACCOUNT="${2:-}"
      if [[ -z "${ACCOUNT}" ]]; then
        echo "error: --account requires a username or uuid" >&2
        exit 1
      fi
      shift 2
      ;;
    --source)
      SOURCES+=("${2:-}")
      if [[ -z "${SOURCES[-1]}" ]]; then
        echo "error: --source requires an id" >&2
        exit 1
      fi
      shift 2
      ;;
    --append)
      MODE="append"
      shift
      ;;
    --overwrite-contacts)
      OVERWRITE_CONTACTS=1
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    -*)
      echo "error: unknown option '$1'" >&2
      usage >&2
      exit 1
      ;;
    *)
      echo "error: unexpected argument '$1' (use --source <id>)" >&2
      usage >&2
      exit 1
      ;;
  esac
done

if [[ -z "${ACCOUNT}" ]]; then
  echo "error: --account <username> is required" >&2
  usage >&2
  exit 1
fi

if [[ ${#SOURCES[@]} -eq 0 ]]; then
  echo "error: at least one --source <id> is required" >&2
  usage >&2
  exit 1
fi

cd "${REPO_ROOT}"

for id in "${SOURCES[@]}"; do
  staging="${REPO_ROOT}/staging/${id}"
  if [[ ! -d "${staging}" ]]; then
    echo "error: staging directory missing: ${staging}" >&2
    exit 1
  fi
  cmd=(
    cargo run --release -- import
    --config "${CONFIG}"
    --account "${ACCOUNT}"
    --source "${id}"
    --export-dir "${staging}"
    --mode "${MODE}"
  )
  if [[ "${OVERWRITE_CONTACTS}" -eq 1 ]]; then
    cmd+=(--overwrite-contacts)
  fi
  echo "+" "${cmd[@]}"
  "${cmd[@]}"
  OVERWRITE_CONTACTS=0
done

echo "Import finished (mode=${MODE})."

dedupe_cmd=(
  cargo run --release -- dedupe-cross-source
  --config "${CONFIG}"
  --account "${ACCOUNT}"
)
echo "+" "${dedupe_cmd[@]}"
"${dedupe_cmd[@]}"
echo "Cross-source dedupe finished."
