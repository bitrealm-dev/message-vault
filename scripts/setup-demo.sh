#!/usr/bin/env bash
# setup-demo.sh — first-time demo bootstrap
#
# Usage:
#   ./scripts/setup-demo.sh

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

cd "${REPO_ROOT}"

echo "Building message-vault-server (release)…"
cargo build --release

echo "Regenerating demo bundle, importing, and processing assets…"
cargo run --release -- reset-demo

echo "Demo ready. Start the UI: cd web && npm run dev"
