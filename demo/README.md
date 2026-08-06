# Message Vault demo dataset

Committed message-ir JSONL bundle for local browsing without a real phone backup.

Regenerate + import in one step:

```bash
cargo run --release -- reset-demo
```

Or regenerate the bundle only:

```bash
cargo run -p demo-seed -- --out demo
```

Config knobs live in `crates/demo-seed/demo_seed.toml` (seed, contact count, rate/span
distributions, group membership). Message bodies are sampled from Pride and Prejudice
(5274 sentences) under `crates/demo-seed/data/corpus/`. Names come from
`crates/demo-seed/data/names/`.

## Contents (seed 42)

| Item | Count |
|------|------:|
| Contacts (VCF) | 200 |
| Groups | 223 |
| Conversation files | 390 |
| Messages | 627207 |
| Attachment references | 10643 |

## Exercises

- **Contacts / labels / No Messages** — label memberships and zero-message rows
- **Unassigned** — handles with messages but no VCF row (phone + email)
- **Rate skew** — most 1:1 threads ~200–300 msgs/year (bursty days); rare whales up to ~12k/year
- **History** — typical first contact ~3–5 years ago; longest ~14 years; newest ~1 week
- **Group Chats** — membership mean ~5 groups/contact; size mean ~4; bursty days (several / none / a lot)
- **Replies, tapbacks, attachments** — including one intentionally missing file
- **orphaned.jsonl** — synthetic orphaned conversation
