#!/usr/bin/env bash
# Stage a self-contained platform archive for a message-vault-io release.
#
# Layout:
#   root       — message-vault-io (GUI)
#   lib/       — ffmpeg, ffprobe
#   cli/       — exporter CLIs, wtsexporter, message-reexporter, vault-push
#   licenses/  — LICENSE + third-party notices
#
# Usage:
#   scripts/package-release.sh <version> <artifact_suffix> [ext]
#
# Example:
#   scripts/package-release.sh 0.3.0 x86_64-unknown-linux-gnu
#   scripts/package-release.sh 0.3.0 x86_64-pc-windows-msvc .exe
#
# Expects release binaries already built under target/release/.
# Writes:
#   Linux  → dist/message-vault-io-<version>-<suffix>.tgz
#   Windows/macOS → dist/message-vault-io-<version>-<suffix>.zip
set -euo pipefail

VERSION="${1:?version required (e.g. 0.3.0)}"
SUFFIX="${2:?artifact suffix required (e.g. x86_64-unknown-linux-gnu)}"
EXT="${3:-}"

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

case "$SUFFIX" in
  *unknown-linux*) ARCHIVE_EXT=".tgz" ;;
  *) ARCHIVE_EXT=".zip" ;;
esac

STAGE="dist/stage-${SUFFIX}"
OUT_ARCHIVE="dist/message-vault-io-${VERSION}-${SUFFIX}${ARCHIVE_EXT}"
RELEASE_DIR="${CARGO_TARGET_DIR:-target}/release"
rm -rf "$STAGE"
mkdir -p "$STAGE" dist

# --- desktop app at archive root ---
GUI_BIN="message-vault-io${EXT}"
src="${RELEASE_DIR}/${GUI_BIN}"
if [[ ! -f "$src" ]]; then
  echo "missing release binary: $src" >&2
  exit 1
fi
cp "$src" "${STAGE}/${GUI_BIN}"
chmod +x "${STAGE}/${GUI_BIN}" || true

# --- CLI exporters / utilities under cli/ ---
CLI_DIR="${STAGE}/cli"
LIB_DIR="${STAGE}/lib"
LICENSES_DIR="${STAGE}/licenses"
mkdir -p "$CLI_DIR" "$LIB_DIR" "$LICENSES_DIR"
for bin in \
  go-sms-pro-exporter \
  sms-backup-restore-exporter \
  sms-backup-plus-exporter \
  openextract-exporter \
  imazing-exporter \
  imessage-ir-exporter \
  whatsapp-exporter \
  message-reexporter \
  vault-push
do
  src="${RELEASE_DIR}/${bin}${EXT}"
  if [[ ! -f "$src" ]]; then
    echo "missing release binary: $src" >&2
    exit 1
  fi
  cp "$src" "${CLI_DIR}/${bin}${EXT}"
  chmod +x "${CLI_DIR}/${bin}${EXT}" || true
done

# --- third-party helpers (pinned + checksummed) ---
# wtsexporter 0.13.0 (KnugiHK/WhatsApp-Chat-Exporter)
WTSEXPORTER_VERSION=0.13.0
# ffmpeg/ffprobe from eugeneware/ffmpeg-static b6.1.1 (binaries report 7.0.2-static)
FFMPEG_STATIC_TAG=b6.1.1

verify_sha256() {
  local file="$1"
  local expect="$2"
  local actual
  if command -v sha256sum >/dev/null 2>&1; then
    actual="$(sha256sum "$file" | awk '{print $1}')"
  else
    actual="$(shasum -a 256 "$file" | awk '{print $1}')"
  fi
  if [[ "$actual" != "$expect" ]]; then
    echo "SHA-256 mismatch for $file" >&2
    echo "  expected: $expect" >&2
    echo "  actual:   $actual" >&2
    exit 1
  fi
}

download() {
  local url="$1"
  local dest="$2"
  local sha="$3"
  curl -fsSL -o "$dest" "$url"
  verify_sha256 "$dest" "$sha"
}

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

case "$SUFFIX" in
  x86_64-unknown-linux-gnu)
    WTS_ASSET="wtsexporter_linux_x64"
    WTS_OUT="wtsexporter"
    WTS_SHA="e8ee1d5630e0b98bb0ee236e7f64bad7e43225353f18b3d18bbea8764576dcff"
    FFMPEG_ASSET="ffmpeg-linux-x64.gz"
    FFMPEG_SHA="bfe8a8fc511530457b528c48d77b5737527b504a3797a9bc4866aeca69c2dffa"
    FFPROBE_ASSET="ffprobe-linux-x64.gz"
    FFPROBE_SHA="25d9b6ccb05e3d9de9e04e31e2506d8dd7f9f0418981965ac6df12e8d3afd067"
    FFMPEG_LICENSE_ASSET="linux-x64.LICENSE"
    FFMPEG_LICENSE_SHA="8ceb4b9ee5adedde47b31e975c1d90c73ad27b6b165a1dcd80c7c545eb65b903"
    FFMPEG_BIN="ffmpeg"
    FFPROBE_BIN="ffprobe"
    ;;
  aarch64-apple-darwin)
    WTS_ASSET="wtsexporter_macos_arm64"
    WTS_OUT="wtsexporter"
    WTS_SHA="0e243ee2d1aae81e98a3f3e976deaf534fc0da9778ddd753d18b338d9291ddeb"
    FFMPEG_ASSET="ffmpeg-darwin-arm64.gz"
    FFMPEG_SHA="8923876afa8db5585022d7860ec7e589af192f441c56793971276d450ed3bbfa"
    FFPROBE_ASSET="ffprobe-darwin-arm64.gz"
    FFPROBE_SHA="d986a8ec7b030899fe66a8a288ed809a3543338705a3ce178cfb85869c5d80be"
    FFMPEG_LICENSE_ASSET="darwin-arm64.LICENSE"
    FFMPEG_LICENSE_SHA="cb48bf09a11f5fb576cddb0431c8f5ed0a60157a9ec942adffc13907cbe083f2"
    FFMPEG_BIN="ffmpeg"
    FFPROBE_BIN="ffprobe"
    ;;
  x86_64-pc-windows-msvc)
    WTS_ASSET="wtsexporter_win_x64.exe"
    WTS_OUT="wtsexporter.exe"
    WTS_SHA="2d4819b07ef627d48f75aa7cd87bfb42173304f1e7e1af94773a46ab4288ffb0"
    FFMPEG_ASSET="ffmpeg-win32-x64.gz"
    FFMPEG_SHA="8883a3dffbd0a16cf4ef95206ea05283f78908dbfb118f73c83f4951dcc06d77"
    FFPROBE_ASSET="ffprobe-win32-x64.gz"
    FFPROBE_SHA="f309e6223ad89d2fe54bccd420a7709b66fd27540674e92309578ed491a43c8d"
    FFMPEG_LICENSE_ASSET="win32-x64.LICENSE"
    FFMPEG_LICENSE_SHA="8ceb4b9ee5adedde47b31e975c1d90c73ad27b6b165a1dcd80c7c545eb65b903"
    FFMPEG_BIN="ffmpeg.exe"
    FFPROBE_BIN="ffprobe.exe"
    ;;
  *)
    echo "No third-party asset mapping for suffix ${SUFFIX@Q}" >&2
    exit 1
    ;;
esac

download \
  "https://github.com/KnugiHK/WhatsApp-Chat-Exporter/releases/download/${WTSEXPORTER_VERSION}/${WTS_ASSET}" \
  "${CLI_DIR}/${WTS_OUT}" \
  "$WTS_SHA"
chmod +x "${CLI_DIR}/${WTS_OUT}" || true
download \
  "https://raw.githubusercontent.com/KnugiHK/WhatsApp-Chat-Exporter/${WTSEXPORTER_VERSION}/LICENSE" \
  "${LICENSES_DIR}/THIRD_PARTY_WTSEXPORTER.LICENSE" \
  "5db9b4306fed174f2a9462b8ba0728dea3ad5ee261644ca077c1de030f5d6772"

download \
  "https://github.com/eugeneware/ffmpeg-static/releases/download/${FFMPEG_STATIC_TAG}/${FFMPEG_ASSET}" \
  "${TMP}/${FFMPEG_ASSET}" \
  "$FFMPEG_SHA"
download \
  "https://github.com/eugeneware/ffmpeg-static/releases/download/${FFMPEG_STATIC_TAG}/${FFPROBE_ASSET}" \
  "${TMP}/${FFPROBE_ASSET}" \
  "$FFPROBE_SHA"
download \
  "https://github.com/eugeneware/ffmpeg-static/releases/download/${FFMPEG_STATIC_TAG}/${FFMPEG_LICENSE_ASSET}" \
  "${LICENSES_DIR}/THIRD_PARTY_FFMPEG.LICENSE" \
  "$FFMPEG_LICENSE_SHA"

gunzip_to() {
  local src="$1"
  local dest="$2"
  if command -v gzip >/dev/null 2>&1; then
    gzip -dc "$src" > "$dest"
  else
    python3 -c 'import gzip,sys; open(sys.argv[2],"wb").write(gzip.open(sys.argv[1],"rb").read())' "$src" "$dest"
  fi
}
gunzip_to "${TMP}/${FFMPEG_ASSET}" "${LIB_DIR}/${FFMPEG_BIN}"
gunzip_to "${TMP}/${FFPROBE_ASSET}" "${LIB_DIR}/${FFPROBE_BIN}"
chmod +x "${LIB_DIR}/${FFMPEG_BIN}" "${LIB_DIR}/${FFPROBE_BIN}" || true

# --- licenses ---
cp LICENSE "${LICENSES_DIR}/LICENSE"
cp scripts/release/THIRD_PARTY_NOTICES.md "${LICENSES_DIR}/THIRD_PARTY_NOTICES.md"

# --- archive (paths relative to stage root; no nested folder) ---
rm -f "$OUT_ARCHIVE"
(
  cd "$STAGE"
  case "$ARCHIVE_EXT" in
    .tgz)
      tar -czf "../$(basename "$OUT_ARCHIVE")" .
      ;;
    .zip)
      if command -v zip >/dev/null 2>&1; then
        zip -9 -r "../$(basename "$OUT_ARCHIVE")" .
      else
        python3 -c '
import pathlib, zipfile, sys
out = pathlib.Path(sys.argv[1])
stage = pathlib.Path(".")
with zipfile.ZipFile(out, "w", compression=zipfile.ZIP_DEFLATED) as zf:
    for path in sorted(stage.rglob("*")):
        if path.is_file():
            zf.write(path, path.as_posix())
print(f"wrote {out}")
' "../$(basename "$OUT_ARCHIVE")"
      fi
      ;;
    *)
      echo "unsupported archive extension: ${ARCHIVE_EXT@Q}" >&2
      exit 1
      ;;
  esac
)

ls -la "$OUT_ARCHIVE"
echo "Staged contents:"
find "$STAGE" -type f | sort
