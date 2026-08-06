#!/usr/bin/env bash
# Build the Vite SPA and copy into static/ for Docker.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
IO_ROOT="$(cd "$REPO_ROOT/../message-vault-io" && pwd)"

echo "Building Vite SPA in $IO_ROOT/web…"
(cd "$IO_ROOT/web" && npm run build)

echo "Copying dist/ to $REPO_ROOT/static/…"
rm -rf "$REPO_ROOT/static"
cp -r "$IO_ROOT/web/dist" "$REPO_ROOT/static"

echo "Done. Ready for Docker build: docker compose -f compose-release.yml up --build"
