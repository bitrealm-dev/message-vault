---
title: Import modes and dedupe
description: How replace vs append work, and how cross-source soft-dedupe hides duplicate SMS.
---

**Short answer:** Each archive is deduped against itself on import. After
import, a separate database pass soft-hides the same SMS when it shows up in
more than one archive. Rows are never deleted.

## Terms

- **Source** — one import archive slug, such as `go-sms-pro` or `imessage` (not listed in TOML).
- **Guid** — a message id string from the exporter. Guids are only unique *inside* one source.
- **Content key** — a hash built from chat + UTC time + direction + text + attachment hashes.
- **Soft-hide** — set `duplicate_of` to point at the kept message. The copy stays in the database but **Combined** skips it.

## Per-source import modes

| Mode | Behavior |
|------|----------|
| **replace** | Wipe that source’s messages for the account, then reload. CLI `ingest` / `import` default. |
| **append** | Keep existing rows. Skip when `(account_id, source, guid)` already exists. HTTP API default. |

Other sources are not touched.

## Cross-source soft-dedupe

Runs after import via `ingest`, `./scripts/ingest-staging.sh`, or:

```bash
cargo run --release -- dedupe-cross-source --account yourusername
```

HTTP import defaults to `dedupe=false`; pass `dedupe=true` to run this pass
after a remote push. See [HTTP import API](/import/http-api/).

### Content key

Each message gets a `content_key` from chat identity, direction, UTC epoch
seconds, normalized body text, and sorted attachment `sha256` hashes.

Chat identity is `chat_identifier` for 1:1 threads. For groups it is the sorted
participant handle list, so the same people match across exporters even when
chat ids differ.

Re-running `dedupe-cross-source` rebuilds every content key and clears prior
`duplicate_of` flags before matching again.

### Pass A — exact match

Group messages that share a content key across **two or more** sources. Keep
one survivor (prefer more attachments, then earliest source for the account,
then lower message id). Soft-hide the rest.

### Pass B — near time

Inside one conversation, match pairs from different sources with the same
direction, same body or attachment hashes, within **±2 seconds**
(`--window-secs`).

## What the UI shows

- **Combined** — one copy of each matched SMS (hides `duplicate_of`).
- **Single source** — the full archive, including soft-hidden copies.

## What matches well / what can miss

**Usually matches:** same 1:1 SMS in two sources with matching UTC second and
body; same MMS bytes via attachment hashes.

**May miss:** encoding-only body differences; large clock skew; mismatched
group participant sets.

**False positives (rare):** two different texts with the same body in the same
chat within 2 seconds. Soft-hide is reversible by clearing `duplicate_of` or
re-running dedupe after fixing data.

## What is not done

- Losing rows are **not** deleted — only flagged.
- Cross-source matching does **not** use `guid`.
- There is no fuzzy full-text cross-database match.
