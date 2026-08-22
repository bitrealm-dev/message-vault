#!/usr/bin/env bash
# Local pre-PR check: format Rust and web, then workspace build/test, web lint/test,
# docs check/build.
#
#   ./scripts/check-pr.sh
#
# Stops on the first failure. Format rewrites files (not --check): rustfmt on
# the workspace and src-tauri, Biome on web/. Runs npm ci in web/ and docs/
# only when that tree has no node_modules yet.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
cd "${REPO_ROOT}"

"${SCRIPT_DIR}/format-all.sh"

echo "==> cargo build --workspace"
cargo build --workspace

echo "==> cargo test --workspace"
cargo test --workspace

echo "==> web lint"
(cd web && npm run lint)
echo "==> web test"
(cd web && npm test)

if [[ ! -d docs/node_modules ]]; then
  echo "==> npm ci (docs)"
  (cd docs && npm ci)
fi
echo "==> docs check"
(cd docs && npm run check)
echo "==> docs build"
(cd docs && npm run build)

echo "All pre-PR checks passed."
