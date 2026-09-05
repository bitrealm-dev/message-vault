#!/usr/bin/env bash
# Test coverage for the Rust workspace, measured by cargo-llvm-cov.
#
#   ./scripts/coverage.sh          # summary table on stdout, reports under target/llvm-cov/
#   ./scripts/coverage.sh --open   # the same, then open the HTML report in a browser
#
# Writes target/llvm-cov/html/index.html (per-file, per-line view),
# target/llvm-cov/lcov.info (for editor plugins and other tools) and
# target/llvm-cov/summary.txt (the table this script prints).
#
# Needs cargo-llvm-cov (`cargo install cargo-llvm-cov --locked`) and the
# llvm-tools rustup component, which rust-toolchain.toml installs. The
# instrumented build lives under target/llvm-cov-target, apart from plain
# `cargo build` output, so the first run compiles the workspace from scratch.
# Export MV_TEST_POSTGRES_URL to include the Postgres-gated server suites.
# src-tauri is not a workspace member and is not measured; its commands are
# thin wrappers over the exporter and push/pull crates, which are.
# Test code itself (tests/ directories and the <module>/tests.rs files) is
# left out of the numbers. Coverage is a report, never a gate:
# docs/adr/0007-ci-is-the-only-gate.md. The Coverage workflow runs this
# same script on every push to main and keeps the reports as artifacts.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
cd "${REPO_ROOT}"

if ! cargo llvm-cov --version >/dev/null 2>&1; then
  echo "cargo-llvm-cov is not installed: cargo install cargo-llvm-cov --locked" >&2
  exit 1
fi

OUT="target/llvm-cov"
IGNORE='(^|/)tests/|/tests\.rs$'
mkdir -p "${OUT}"

echo "==> cargo llvm-cov (workspace tests, instrumented)"
cargo llvm-cov --workspace --no-report --ignore-filename-regex "${IGNORE}"

echo "==> reports"
cargo llvm-cov report --ignore-filename-regex "${IGNORE}" --lcov --output-path "${OUT}/lcov.info"
cargo llvm-cov report --ignore-filename-regex "${IGNORE}" --html --output-dir "${OUT}"
cargo llvm-cov report --ignore-filename-regex "${IGNORE}" | tee "${OUT}/summary.txt"

echo "HTML report: ${OUT}/html/index.html"
if [[ "${1:-}" == "--open" ]]; then
  cargo llvm-cov report --ignore-filename-regex "${IGNORE}" --html --output-dir "${OUT}" --open
fi
