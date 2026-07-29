---
title: Obsidian
description: Export 1:1 threads as bubble-style markdown for Obsidian.
---

```bash
cargo run --release -- export-markdown \
  --out /path/to/Obsidian-Message-Vault \
  --account yourusername
```

Enable the `message-vault-bubbles` CSS snippet in Obsidian
(**Settings → Appearance**). The snippet ships at
[`config/obsidian-message-vault.css`](https://github.com/bitrealm-dev/message-vault-rs/blob/main/config/obsidian-message-vault.css).

Pass `--snippet-css` if you keep a copy elsewhere.

The export writes year pages and copies assets for **1:1** threads for the
selected account.
