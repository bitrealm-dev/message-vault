---
title: "Convert an existing export"
description: "How message-reexport converts a Message Vault output folder from one packaging format to another."
---

The `message-reexport` package converts an existing Message Vault output
directory to another packaging format. The desktop app calls it as the second
step of an [Export](/vault/user/how-to/export-from-the-vault/) into any format
other than JSON Lines.

Converting a folder that already exists, without going through the vault, has
no screen yet — see [issue 275](https://github.com/bitrealm-io/message-vault/issues/275).
The library reads all six formats and can convert between any pair of them.

**Formats and what each writes:** [Export formats](/vault/developer/reference/export-formats/)
