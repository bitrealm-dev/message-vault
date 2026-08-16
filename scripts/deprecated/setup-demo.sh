#!/usr/bin/env bash
# setup-demo.sh — first-time demo bootstrap without Docker
#
# Deprecated: development targets Docker. `docker compose up` seeds the same
# demo data on an empty database unless DEMO_DATA=false.
#
# Usage:
#   ./scripts/deprecated/setup-demo.sh

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"

cd "${REPO_ROOT}"

echo "Building message-vault-server (release)…"
cargo build --release -p message-vault-server

echo "Regenerating demo bundle, importing, and processing assets…"
cargo run --release -p message-vault-server -- reset-demo

echo "Demo ready. Start the UI: cd web && npm run dev"
