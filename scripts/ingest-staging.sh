#!/usr/bin/env bash
# ingest-staging.sh — import Message Exporters JSONL staging folders + dedupe
#
# Each source folder must already contain `*.jsonl` (+ attachments).
# Default staging path: staging/<source_id>/
#
# Usage:
#   ./scripts/ingest-staging.sh --account <username> --source imessage
#   ./scripts/ingest-staging.sh --account <username> --source imessage --staging-dir /path
#   ./scripts/ingest-staging.sh --account <username> --source imessage --source go-sms-pro
#   ./scripts/ingest-staging.sh --account <username> --append --source sms-backup-plus
#
# Runs:
#   cargo run --release -- ingest <id> --account <username> --staging-dir … …

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
CONFIG="${REPO_ROOT}/config/config.toml"

MODE="replace"
OVERWRITE_CONTACTS=0
SKIP_DEDUPE=0
ACCOUNT=""
SOURCES=()
STAGING_DIR=""

usage() {
  cat <<'EOF'
Usage: ingest-staging.sh --account <username> --source <id> [OPTIONS] [--source <id>…]

Options:
  --account <username>     Vault account username or UUID (required)
  --source <id>            Source slug to import (required; repeatable)
  --staging-dir <path>     Override staging for a single --source
  --append                 Import mode append (default: replace)
  --overwrite-contacts     Reload contacts CSV on import
  --skip-dedupe            Skip cross-source soft-dedupe after import
  -h, --help               Show this help
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
    --staging-dir)
      STAGING_DIR="${2:-}"
      if [[ -z "${STAGING_DIR}" ]]; then
        echo "error: --staging-dir requires a path" >&2
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
    --skip-dedupe)
      SKIP_DEDUPE=1
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

if [[ -n "${STAGING_DIR}" && ${#SOURCES[@]} -ne 1 ]]; then
  echo "error: --staging-dir only applies with a single --source" >&2
  exit 1
fi

cd "${REPO_ROOT}"

last_idx=$((${#SOURCES[@]} - 1))
echo "Ingesting ${#SOURCES[@]} source(s): ${SOURCES[*]}"
echo

for i in "${!SOURCES[@]}"; do
  id="${SOURCES[$i]}"
  n=$((i + 1))
  echo "==> [${n}/${#SOURCES[@]}] ${id}"

  if [[ -n "${STAGING_DIR}" ]]; then
    staging="${STAGING_DIR}"
  else
    staging="${REPO_ROOT}/staging/${id}"
  fi
  if [[ ! -d "${staging}" ]]; then
    echo "error: staging directory missing: ${staging}" >&2
    exit 1
  fi

  cmd=(
    cargo run --release -- ingest "${id}"
    --config "${CONFIG}"
    --account "${ACCOUNT}"
    --staging-dir "${staging}"
    --mode "${MODE}"
  )
  if [[ "${OVERWRITE_CONTACTS}" -eq 1 ]]; then
    cmd+=(--overwrite-contacts)
  fi
  # Dedupe once after the last source (or never if --skip-dedupe).
  if [[ "${SKIP_DEDUPE}" -eq 1 || "${i}" -lt "${last_idx}" ]]; then
    cmd+=(--skip-dedupe)
  fi

  echo "+" "${cmd[@]}"
  "${cmd[@]}"
  echo
done

echo "All ${#SOURCES[@]} source(s) finished."
