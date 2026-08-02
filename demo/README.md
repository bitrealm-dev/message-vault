# Message Vault demo dataset

Committed message-ir JSONL demo bundle for local browsing without a real phone backup.

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
| Conversation files | 266 |
| Messages | 5388 |
| Attachment references | 878 |

## Exercises

- **Contacts / All / labels / No Messages** — label memberships and zero-message rows
- **Unassigned** — handles with messages but no CSV row (phone + email-only)
- **Frequent / lapsed** — ~15 contacts busy in the past 3 years; ~10 mostly older history
- **High volume** — a couple 1:1 threads with 1000+ messages
- **Group Chats** — ~200 threads, many untitled, some phone-number-only participants, sizes up to 20
- **Year threads** — message history from 2016 through present (10 years)
- **Replies, tapbacks, attachments** — including one intentionally missing file
- **orphaned.jsonl** — synthetic “orphaned” conversation (header + messages)
