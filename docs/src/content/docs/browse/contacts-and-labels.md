---
title: Contacts and labels
description: Contacts CSV, VCF import, and sidebar labels.
---

## Contact sections

| Section | Meaning |
|---------|---------|
| **All** | Every non-trashed contact |
| **No Messages** | Contacts with no visible 1:1 or group messages |
| **No label** | Contacts with no label memberships |
| **Per-label** | Contacts that belong to that label (`/label/[slug]`) |

Contact data is stored only in SQLite. Contact files are transient import
inputs or explicit downloads; they are never written under an account’s data
directory.

## Contact data boundary

Message Vault is **not** a contacts manager. It keeps only what browsing
needs:

- Normalized phone handles (E.164 where possible)
- Display names (`preferred_name` in SQLite)
- Vault-owned labels

Uploaded VCF files are **transient**. The vault does not store the raw VCF,
emails, notes, photos, UIDs, or category provenance. After you confirm an
import, copied categories become ordinary vault labels you can rename or
edit independently of your address book.

## Export contacts CSV

In the contacts ⋯ menu, **Export contacts CSV** downloads a vault-owned
projection built from SQLite for the signed-in account (`GET
/api/contacts/export-csv`). It includes sanitized phones, names, and **every**
current vault label. It is not a raw VCF re-export.

The downloaded CSV is phone-only and uses `phones`, `first_name`, `last_name`,
and ordered `label_1`…`label_N` columns. Optional iMessage email handles remain
in SQLite and are not exported.

### CLI: import address book

Load a **VCF** or **vCard CSV** (contacts exported as CSV) into SQLite (replaces
that account’s contacts):

```bash
cargo run --release -- import-contacts \
  --account yourusername \
  --contacts path/to/contacts.vcf

# or vCard CSV:
cargo run --release -- import-contacts \
  --account yourusername \
  --contacts path/to/Contacts.csv
```

The same `--contacts` flag is available on `import`. VCF
`CATEGORIES` (and `[Tag]` markers in `FN`) become vault labels.

## Import VCF (web)

When the vault is writable, **Import VCF** in the contacts menu:

1. Uploads the file for a **preview** (nothing is written yet)
2. Keeps only cards whose phones appear in conversations that have messages
   for this account
3. Lists discovered categories with matched-member counts
4. Lets you enable/disable each category and rename its destination vault label
5. On confirm, merges names/phones for matched cards and copies selected
   categories into labels **once** (additive; not a sync)

Unmatched address-book cards and non-vault VCF fields are ignored.

## Labels

Create and manage labels in the sidebar. Membership is stored in SQLite
(`contact_labels`). Open a label page to browse its members.

## Unmapped handles

Handles with messages but no matching contact can appear in Trash workflows /
unassigned APIs. There is no dedicated `/unassigned` browse route.
