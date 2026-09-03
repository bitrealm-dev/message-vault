#!/usr/bin/env bash
# Smoke-test POST /v1/import against a temporary config + DB.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$ROOT"

SAMPLE="${ROOT}/scripts/test/fixtures/smoke-sms-text.jsonl"
ACCOUNT="00000000-0000-0000-0000-00000000s001"
TOKEN="mv_smoke_import_token"
TOKEN_HASH="$(printf '%s' "${TOKEN}" | sha256sum | awk '{print $1}')"
BIND="127.0.0.1:18080"
TMP="$(mktemp -d)"
trap 'kill ${SERVER_PID:-} 2>/dev/null || true; rm -rf "$TMP"' EXIT

mkdir -p "$TMP/staging/imessage" "$TMP/config" "$TMP/data"
cp "$SAMPLE" "$TMP/staging/imessage/"

cat >"$TMP/config/config.toml" <<EOF
[paths]
db = "${TMP}/data/vault.db"
data_dir = "${TMP}/data"
assets_dir = "assets"
assets_converted_dir = "assets_converted"

[server]
bind = "${BIND}"
EOF

# Seed account + hashed Import API token (no host admin token).
# Use the shared accounts DDL so seeded tables match the server schema;
# hand-rolled minimal tables would break ix_accounts_hanko_user_id at serve startup.
sqlite3 "$TMP/data/vault.db" <<SQL
PRAGMA foreign_keys = ON;
$(cat "$ROOT/schema/sql/accounts.sql")
INSERT INTO accounts (id, username, read_only) VALUES ('${ACCOUNT}', 'smoke', 0);
INSERT INTO account_session_tokens (account_id, token_hash, created_at)
VALUES ('${ACCOUNT}', '${TOKEN_HASH}', 'smoke');
SQL

export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/target}"
cargo build --release -q
"$CARGO_TARGET_DIR/release/message-vault-server" serve --config "$TMP/config/config.toml" &
SERVER_PID=$!

for _ in $(seq 1 50); do
  if curl -sf "http://${BIND}/health" >/dev/null; then
    break
  fi
  sleep 0.1
done
curl -sf "http://${BIND}/health" >/dev/null

RESP="$(curl -sS -X POST \
  "http://${BIND}/v1/import?source=imessage&account=${ACCOUNT}&mode=replace" \
  -H "Authorization: Bearer ${TOKEN}" \
  -H "Content-Type: application/jsonl" \
  --data-binary @"$SAMPLE")"

echo "$RESP" | grep -q '"messages_appended"'
echo "$RESP" | grep -q '"messages":1'
echo "smoke-import-api: ok"
echo "$RESP"
