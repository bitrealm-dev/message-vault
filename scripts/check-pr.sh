#!/usr/bin/env bash
# Local pre-PR check: format Rust and web, then license consistency, workspace
# build/test, src-tauri check, web lint/test/build, docs check/build.
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

echo "==> license consistency"
"${SCRIPT_DIR}/check-license.sh"

echo "==> cargo deny check advisories"
if cargo deny --version >/dev/null 2>&1; then
  cargo deny check advisories
else
  echo "cargo-deny not installed; skipping advisory check (CI enforces it)" >&2
fi

echo "==> cargo build --workspace"
cargo build --workspace

echo "==> cargo test --workspace"
cargo test --workspace

echo "==> cargo check src-tauri"
cargo check --manifest-path src-tauri/Cargo.toml

echo "==> web lint"
(cd web && npm run lint)
echo "==> web test"
(cd web && npm test)
echo "==> web audit"
(cd web && npm audit --audit-level=high)
echo "==> web build (type-check + bundle)"
(cd web && npm run build)

if [[ ! -d docs/node_modules ]]; then
  echo "==> npm ci (docs)"
  (cd docs && npm ci)
fi
echo "==> docs check"
(cd docs && npm run check)
echo "==> docs build"
(cd docs && npm run build)
echo "==> docs audit"
(cd docs && npm audit --audit-level=high)

echo "All pre-PR checks passed."
