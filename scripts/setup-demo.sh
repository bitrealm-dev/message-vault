#!/usr/bin/env bash
# setup-demo.sh — first-time demo bootstrap (config + import + media conversion)
#
# Usage:
#   ./scripts/setup-demo.sh

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

cd "${REPO_ROOT}"

if [[ ! -f demo/config/config.toml ]]; then
  echo "error: demo bundle missing; run: cargo run -p demo-seed -- --out demo" >&2
  exit 1
fi

echo "Building message-vault-rs (release)…"
cargo build --release

echo "Resetting demo vault…"
cargo run --release -- reset-demo

echo "Converting demo media for web…"
cargo run --release -- process-assets

echo "Demo ready. Start the UI: cd web && npm run dev"
