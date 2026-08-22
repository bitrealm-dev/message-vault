#!/usr/bin/env bash
# Run Rust Clippy and the web linter.
#
#   ./scripts/lint-all.sh
#
# Stops on the first failure. Clippy covers the workspace (except the legacy
# Slint GUI crate) and src-tauri. Biome lints web/; recommended rules are
# errors (same as CI `biome ci`). Clippy warnings do not fail this script.
# Does not format, test, or build. Runs npm ci in web/ only when that tree
# has no node_modules yet.
#
# Skips docs/, web-next/, and message-vault-io-gui (not the product path).
# CI does not run Clippy. This script is the local Clippy + web lint command.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
cd "${REPO_ROOT}"

echo "==> cargo clippy (workspace, exclude message-vault-io-gui)"
cargo clippy --workspace --all-targets --exclude message-vault-io-gui

echo "==> cargo clippy (src-tauri)"
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets

if [[ ! -d web/node_modules ]]; then
  echo "==> npm ci (web)"
  (cd web && npm ci)
fi
echo "==> web lint"
(cd web && npm run lint)

echo "Lint complete."
