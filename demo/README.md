# Message Vault demo dataset

Committed message-ir JSONL bundle for local browsing without a real phone backup.

Two staging trees simulate separate phone backups:

- `staging/imessage/` — Apple Messages-style export
- `staging/sms-backup-restore/` — Android SMS Backup & Restore–style export

Most conversations are single-source. A small set appears in both so the Sources panel and
cross-source dedupe can be exercised.

Regenerate + import in one step:

```bash
cargo run --release -p message-vault-server -- reset-demo
```

Or regenerate the bundle only:

```bash
cargo run -p demo-seed -- --out demo
```

Config knobs live in `crates/vault/demo-seed/demo_seed.toml` (seed, contact count, rate/span
distributions, group membership, dual-source split). Message bodies are sampled from Pride and
Prejudice (5274 sentences) under `crates/vault/demo-seed/data/corpus/`. Names come from
`crates/vault/demo-seed/data/names/`.

## Contents (seed 42)

| Item | Count |
|------|------:|
| Contacts (VCF) | 200 |
| Groups | 178 |
| Conversation files | 355 |
| Messages | 544553 |
| Attachment references | 8994 |

## Exercises

- **Dual sources** — `imessage` vs `sms-backup-restore`; light overlap threads
- **Contacts / labels / No Messages** — label memberships and zero-message rows
- **Unassigned** — handles with messages but no VCF row (phone + email)
- **Rate skew** — most 1:1 threads ~200–300 msgs/year (bursty days); rare whales up to ~12k/year
- **History** — typical first contact ~3–5 years ago; longest ~14 years; newest ~1 week
- **Group Chats** — membership mean ~5 groups/contact; size mean ~4; at least 10 groups with 8–20 participants; bursty days (several / none / a lot)
- **Replies, tapbacks, attachments** — including one intentionally missing file
- **orphaned.jsonl** — synthetic orphaned conversation
