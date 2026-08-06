#!/usr/bin/env bash
# Multipart JSONL import + per-user API token auth checks.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

ACCOUNT="00000000-0000-0000-0000-00000000s001"
OTHER="00000000-0000-0000-0000-00000000s002"
USER_TOKEN="mv_smoke_user_token_s001"
USER_TOKEN_HASH="$(printf '%s' "${USER_TOKEN}" | sha256sum | awk '{print $1}')"
BIND="127.0.0.1:18081"
TMP="$(mktemp -d)"
trap 'kill ${SERVER_PID:-} 2>/dev/null || true; rm -rf "$TMP"' EXIT

mkdir -p "$TMP/config" "$TMP/data" "$TMP/client/media"

printf '\xff\xd8\xff\xd9' >"$TMP/client/media/photo.jpg"
cp "$ROOT/scripts/fixtures/smoke-sms-attachment.jsonl" "$TMP/client/chat-a.jsonl"
cp "$ROOT/scripts/fixtures/smoke-sms-text.jsonl" "$TMP/client/chat-b.jsonl"

cat >"$TMP/config/config.toml" <<EOF
[paths]
db = "${TMP}/data/vault.db"
data_dir = "${TMP}/data"
assets_dir = "assets"
assets_converted_dir = "assets_converted"

[server]
bind = "${BIND}"
EOF

# Seed account + hashed Import API token before serve.
# Use the shared accounts DDL so seeded tables match the server schema;
# hand-rolled minimal tables would break ix_accounts_hanko_user_id at serve startup.
sqlite3 "$TMP/data/vault.db" <<SQL
PRAGMA foreign_keys = ON;
$(cat "$ROOT/schema/sql/accounts.sql")
INSERT INTO accounts (id, username, read_only) VALUES ('${ACCOUNT}', 'smoke', 0);
INSERT INTO account_api_tokens (account_id, token_hash, created_at)
VALUES ('${ACCOUNT}', '${USER_TOKEN_HASH}', 'smoke');
SQL

export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/target}"
cargo build --release -q -p message-vault-server

"$CARGO_TARGET_DIR/release/message-vault-server" serve --config "$TMP/config/config.toml" &
SERVER_PID=$!

for _ in $(seq 1 50); do
  if curl -sf "http://${BIND}/health" >/dev/null; then
    break
  fi
  sleep 0.1
done
curl -sf "http://${BIND}/health" >/dev/null

# Auth check: bad token → 401; user token → ok
code="$(curl -sS -o /dev/null -w '%{http_code}' \
  -H "Authorization: Bearer wrong-token" \
  "http://${BIND}/v1/auth/check")"
test "$code" = "401"

AUTH="$(curl -sS \
  -H "Authorization: Bearer ${USER_TOKEN}" \
  "http://${BIND}/v1/auth/check")"
echo "$AUTH" | grep -q '"ok":true'
echo "$AUTH" | grep -q "\"account_id\":\"${ACCOUNT}\""

# Multipart import chat-a (jsonl + attachment file)
RESP_A="$(curl -sS -X POST \
  "http://${BIND}/v1/import?source=imessage&account=${ACCOUNT}&mode=append" \
  -H "Authorization: Bearer ${USER_TOKEN}" \
  -F "jsonl=@${TMP}/client/chat-a.jsonl;type=application/jsonl" \
  -F "file=@${TMP}/client/media/photo.jpg;filename=media/photo.jpg")"
echo "$RESP_A" | grep -q '"ok":true'
echo "$RESP_A" | grep -q '"messages":1'

# Multipart import chat-b (jsonl only)
RESP_B="$(curl -sS -X POST \
  "http://${BIND}/v1/import?source=imessage&account=${ACCOUNT}&mode=append" \
  -H "Authorization: Bearer ${USER_TOKEN}" \
  -F "jsonl=@${TMP}/client/chat-b.jsonl;type=application/jsonl")"
echo "$RESP_B" | grep -q '"ok":true'
echo "$RESP_B" | grep -q '"messages":1'

AUTH2="$(curl -sS \
  -H "Authorization: Bearer ${USER_TOKEN}" \
  "http://${BIND}/v1/auth/check?account=${ACCOUNT}")"
echo "$AUTH2" | grep -q '"account_ok":true'
echo "$AUTH2" | grep -q 'imessage'

# User token cannot target another account
code="$(curl -sS -o /dev/null -w '%{http_code}' \
  -H "Authorization: Bearer ${USER_TOKEN}" \
  "http://${BIND}/v1/auth/check?account=${OTHER}")"
test "$code" = "403"

# User token import without account query (bound to token)
RESP_USER="$(curl -sS -X POST \
  "http://${BIND}/v1/import?source=imessage&mode=append" \
  -H "Authorization: Bearer ${USER_TOKEN}" \
  -F "jsonl=@${TMP}/client/chat-b.jsonl;type=application/jsonl")"
echo "$RESP_USER" | grep -q '"ok":true'
echo "$RESP_USER" | grep -q "\"account\":\"${ACCOUNT}\""

echo "smoke-vault-push: ok"
