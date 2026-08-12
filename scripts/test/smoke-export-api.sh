#!/usr/bin/env bash
# Smoke-test GET /v1/export/messages and GET /v1/assets/{sha256}.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$ROOT"

SAMPLE="${ROOT}/scripts/test/fixtures/smoke-sms-text.jsonl"
ACCOUNT="00000000-0000-0000-0000-00000000s001"
TOKEN="mv_smoke_export_token"
TOKEN_HASH="$(printf '%s' "${TOKEN}" | sha256sum | awk '{print $1}')"
BIND="127.0.0.1:18082"
TMP="$(mktemp -d)"
trap 'kill ${SERVER_PID:-} 2>/dev/null || true; rm -rf "$TMP"' EXIT

mkdir -p "$TMP/config" "$TMP/data" "$TMP/media"
# Minimal JPEG (1x1) for asset round-trip.
printf '\xff\xd8\xff\xd9' >"$TMP/media/photo.jpg"
SHA="$(sha256sum "$TMP/media/photo.jpg" | awk '{print $1}')"

# message-ir JSONL with pre-uploaded attachment digest
cat >"$TMP/att.jsonl" <<EOF
{"schema_version":3,"export":{"source":"sms-backup-restore","tool":"smoke","tool_version":"0","owner_handle":null,"owner_display_name":null},"conversation":{"chat_identifier":"+14075555678","conversation_type":"individual","group_title":null,"participants":[{"handle":"+14075555678","display_name":"Bob"}],"stats":{"message_count":1,"attachment_count":1,"first_timestamp_unix_ms":1426183522000,"last_timestamp_unix_ms":1426183522000}}}
{"guid":"smoke-export-att-1","timestamp_unix_ms":1426183522000,"direction":"outgoing","service":"sms","message_kind":"mms","sender_handle":null,"sender_display_name":null,"subject":null,"text":"Photo","attachments":[{"path":"attachments/photo.jpg","original_name":"photo.jpg","mime_type":"image/jpeg","digest_sha256":"${SHA}","is_sticker":false,"transcription":null,"sticker_effect":null}],"imessage":null,"source":null}
EOF

cat >"$TMP/config/config.toml" <<EOF
[paths]
db = "${TMP}/data/vault.db"
data_dir = "${TMP}/data"
assets_dir = "assets"
assets_converted_dir = "assets_converted"

[server]
bind = "${BIND}"
EOF

# Seed account + hashed Import API token (same pattern as smoke-import-api.sh).
# Use the shared accounts DDL so seeded tables match the server schema;
# hand-rolled minimal tables would break ix_accounts_hanko_user_id at serve startup.
python3 - <<PY
import sqlite3
db = sqlite3.connect("${TMP}/data/vault.db")
db.executescript("""
PRAGMA foreign_keys = ON;
$(cat "$ROOT/schema/sql/accounts.sql")
INSERT INTO accounts (id, username, read_only) VALUES ('${ACCOUNT}', 'smoke', 0);
INSERT INTO account_session_tokens (account_id, token_hash, created_at)
VALUES ('${ACCOUNT}', '${TOKEN_HASH}', 'smoke');
""")
db.commit()
db.close()
PY

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

# Auth check
AUTH="$(curl -sS "http://${BIND}/v1/auth/check" -H "Authorization: Bearer ${TOKEN}")"
echo "$AUTH" | grep -q '"ok":true'

# Upload asset then import message that references it
curl -sS -X PUT \
  "http://${BIND}/v1/assets/${SHA}?source=sms-backup-restore&account=${ACCOUNT}" \
  -H "Authorization: Bearer ${TOKEN}" \
  -H "Content-Type: image/jpeg" \
  --data-binary @"$TMP/media/photo.jpg" | grep -q '"ok":true'

curl -sS -X POST \
  "http://${BIND}/v1/import?source=sms-backup-restore&account=${ACCOUNT}&mode=replace" \
  -H "Authorization: Bearer ${TOKEN}" \
  -H "Content-Type: application/jsonl" \
  --data-binary @"$TMP/att.jsonl" | grep -q '"ok":true'

# Also import a plain text conversation from the fixture (append)
curl -sS -X POST \
  "http://${BIND}/v1/import?source=imessage&account=${ACCOUNT}&mode=append" \
  -H "Authorization: Bearer ${TOKEN}" \
  -H "Content-Type: application/jsonl" \
  --data-binary @"$SAMPLE" | grep -q '"ok":true'

# Export all messages
EXPORT="$(curl -sS "http://${BIND}/v1/export/messages?limit=50" \
  -H "Authorization: Bearer ${TOKEN}")"
echo "$EXPORT" | grep -q '"ok":true'
echo "$EXPORT" | grep -q 'smoke-export-att-1'
echo "$EXPORT" | grep -q "$SHA"

# Fastmail-style filter (metadata; not full web FTS)
FILTERED="$(curl -sS --get "http://${BIND}/v1/export/messages" \
  --data-urlencode "q=has:attachment after:2015-01-01" \
  -H "Authorization: Bearer ${TOKEN}")"
echo "$FILTERED" | grep -q 'smoke-export-att-1'

# Contacts mode rejected
CODE="$(curl -sS -o /tmp/export-contacts.json -w '%{http_code}' \
  --get "http://${BIND}/v1/export/messages" \
  --data-urlencode "q=search:contacts" \
  -H "Authorization: Bearer ${TOKEN}")"
test "$CODE" = "400"

# Download asset
curl -sS "http://${BIND}/v1/assets/${SHA}?source=sms-backup-restore&account=${ACCOUNT}" \
  -H "Authorization: Bearer ${TOKEN}" \
  -o "$TMP/downloaded.jpg"
cmp -s "$TMP/media/photo.jpg" "$TMP/downloaded.jpg"

echo "smoke-export-api: ok"
