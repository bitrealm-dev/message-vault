# Message Vault demo dataset

Committed message-ir JSONL bundle for local browsing without a real phone backup.

Regenerate with:

```bash
cargo run -p demo-seed -- --config crates/demo-seed/demo_seed.toml --out demo
```

Config knobs live in `crates/demo-seed/demo_seed.toml` (seed, contact count, rate/span
distributions, group membership). Message bodies are sampled from Pride and Prejudice
(5274 sentences) under `crates/demo-seed/data/corpus/`. Names come from
`crates/demo-seed/data/names/`.

Then import:

```bash
cargo run --release -- reset-demo
cargo run --release -- process-assets
```

## Contents (seed 42)

| Item | Count |
|------|------:|
| Contacts (VCF) | 200 |
| Groups | 224 |
| Conversation files | 391 |
| Messages | 307201 |
| Attachment references | 5912 |

## Exercises

- **Contacts / labels / No Messages** — label memberships and zero-message rows
- **Unassigned** — handles with messages but no VCF row (phone + email)
- **Rate skew** — most 1:1 threads ~200–300 msgs/year; rare whales up to ~12k/year
- **History** — typical first contact ~3–5 years ago; longest ~14 years; newest ~1 week
- **Group Chats** — membership mean ~5 groups/contact; size mean ~4; 5–50 msgs/year
- **Replies, tapbacks, attachments** — including one intentionally missing file
- **orphaned.jsonl** — synthetic orphaned conversation
