# Message Vault demo dataset

Committed iMessage JSONL bundle for local browsing without a real iPhone
backup.

Regenerate with:

```bash
cargo run -p demo-seed -- --out demo --seed 42
```

Then import:

```bash
cargo run --release -- reset-demo
cargo run --release -- process-assets
```

## Contents

| Item | Count |
|------|-------|
| Contacts (CSV) | 80 |
| Conversation files | 269 |
| Messages | 5156 |
| Attachment references | 833 |

## Exercises

- **All / labels / No Messages** — contact sections and zero-message rows
- **Unassigned handles** — messages with no CSV contact row (phone + email-only; Trash APIs)
- **Frequent / lapsed** — ~15 contacts busy in the past 3 years; ~10 mostly older history
- **High volume** — a couple 1:1 threads with 1000+ messages
- **Group Chats** — ~200 threads, many untitled, some phone-number-only participants, sizes up to 20
- **Year threads** — message history from 2016 through present (10 years)
- **Replies, tapbacks, attachments** — including one intentionally missing file
- **orphaned.jsonl** — messages without a conversation header
