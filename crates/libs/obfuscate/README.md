# obfuscate

Shared library and tools to rewrite exporter CSV output so it keeps message **structure** (chats, timestamps, directions, attachment counts) without exposing real names, phone numbers, message bodies, or media bytes.

Remaps are **stable** for a given secret seed (HMAC-SHA256) and **not reversible** from the CSV alone. No real→fake mapping file is written.

## Flags on converters

Every converter accepts:

- `--obfuscate` — rewrite the output directory after convert
- `--obfuscate-seed <8-hex>` — reproducible remaps (implies obfuscate). Exactly 8 hex characters. If omitted, a random 8-hex seed is printed once to stderr.

## iMazing CSV rewriter

iMazing vendor CSV is not converted here—only rewritten:

```bash
cargo run --release -p obfuscate --bin imazing-obfuscate -- \
  --input /path/to/imazing.csv \
  --output ./staging/imazing-anon
```

Optional: `--obfuscate-seed <8-hex>`.

## What changes

| Field | Behavior |
|-------|----------|
| Phone numbers | Same digit count; `+` / spaces / dashes / parentheses kept |
| Display names | Human first + last from a fixed word list |
| Emails | Valid `first.last@example.invalid` (stable per original) |
| URLs | Valid `http(s)://…example.invalid…` (path shape kept) |
| Message text | Word-shape nonsense; embedded emails/URLs/phones stay valid fakes |
| Attachments | Shared placeholders: image → `placeholder.jpg`, video → `placeholder.mp4`, other → `placeholder.bin` |
