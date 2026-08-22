#!/usr/bin/env bash
# Rewrite Rust and web sources to the project formatters.
#
#   ./scripts/format-all.sh
#
# Stops on the first failure. rustfmt rewrites workspace + src-tauri.
# Biome rewrites matching files under web/. Does not lint, test, or build.
# Runs npm ci in web/ only when that tree has no node_modules yet.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
cd "${REPO_ROOT}"

echo "==> cargo fmt (workspace)"
cargo fmt --all

echo "==> cargo fmt (src-tauri)"
cargo fmt --manifest-path src-tauri/Cargo.toml

if [[ ! -d web/node_modules ]]; then
  echo "==> npm ci (web)"
  (cd web && npm ci)
fi
echo "==> web format"
(cd web && npm run format)

echo "Format complete."
