#!/usr/bin/env bash
# Assert docker/Dockerfile copies every [patch.crates-io] path crate.
#
#   ./scripts/check-docker-context.sh
#
# Read-only. Cargo resolves workspace patches from the rust-builder
# WORKDIR. If a patched path is missing from the image, `cargo build`
# fails with "failed to read …/Cargo.toml".
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
cd "${REPO_ROOT}"

DOCKERFILE="docker/Dockerfile"
failures=0
saw_patch=0

if [[ ! -f "${DOCKERFILE}" ]]; then
  echo "${DOCKERFILE}: missing" >&2
  exit 1
fi

in_patch=0
while IFS= read -r line || [[ -n "${line}" ]]; do
  if [[ "${line}" == "[patch.crates-io]" ]]; then
    in_patch=1
    continue
  fi
  if [[ "${in_patch}" -eq 1 && "${line}" =~ ^\[ ]]; then
    break
  fi
  if [[ "${in_patch}" -eq 1 && "${line}" =~ path[[:space:]]*=[[:space:]]*\"([^\"]+)\" ]]; then
    saw_patch=1
    path="${BASH_REMATCH[1]}"
    top="${path%%/*}"
    if [[ ! -f "${path}/Cargo.toml" ]]; then
      echo "Cargo.toml patches ${path}, but ${path}/Cargo.toml is missing" >&2
      failures=$((failures + 1))
      continue
    fi
    if ! grep -Eq "^[[:space:]]*COPY[[:space:]]+(${top}|${path})([[:space:]]|/)" "${DOCKERFILE}"; then
      echo "${DOCKERFILE}: COPY ${top} (or ${path}) so cargo can load the [patch.crates-io] crate at ${path}" >&2
      failures=$((failures + 1))
    fi
  fi
done <Cargo.toml

if [[ "${in_patch}" -eq 0 ]]; then
  echo "Cargo.toml has no [patch.crates-io] section; nothing to check." >&2
fi

if [[ "${in_patch}" -eq 1 && "${saw_patch}" -eq 0 ]]; then
  echo "Cargo.toml [patch.crates-io] has no path = crates; nothing to check." >&2
fi

if [[ ${failures} -gt 0 ]]; then
  echo "Docker rust-builder context is missing patched crates (${failures} failure(s))." >&2
  exit 1
fi
