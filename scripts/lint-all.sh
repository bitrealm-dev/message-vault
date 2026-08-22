#!/usr/bin/env bash
# Run Rust Clippy and the web linter.
#
#   ./scripts/lint-all.sh
#
# Stops on the first failure. Clippy covers the workspace and src-tauri.
# Biome lints web/. Warnings do not fail the script (same as CI for web).
# Does not format, test, or build. Runs npm ci in web/ only when that tree
# has no node_modules yet.
#
# CI does not run Clippy. This script is the local Clippy + web lint command.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
cd "${REPO_ROOT}"

echo "==> cargo clippy (workspace)"
cargo clippy --workspace --all-targets

echo "==> cargo clippy (src-tauri)"
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets

if [[ ! -d web/node_modules ]]; then
  echo "==> npm ci (web)"
  (cd web && npm ci)
fi
echo "==> web lint"
(cd web && npm run lint)

echo "Lint complete."
