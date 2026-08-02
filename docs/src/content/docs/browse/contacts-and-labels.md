---
title: Contacts and labels
description: Contacts CSV, VCF import, and sidebar labels (including Active / Inactive).
---

## Contact sections

| Section | Meaning |
|---------|---------|
| **All** | Every non-trashed contact |
| **No Messages** | Contacts with no visible 1:1 or group messages |
| **No label** | Contacts with no label memberships |
| **Per-label** | Contacts that belong to that label (`/label/[slug]`) |

**Active** and **Inactive** are ordinary labels (not special sidebar sections).
A one-time migration assigns them from the legacy `exclude` flag; after that
they behave like any other label.

Contact data is mirrored in the **per-account** contacts CSV:

`data/<account_id>/contacts.csv`

The files under `config/` (`contacts.csv.example`, and so on) are optional
templates only. Runtime imports create empty headers under
`data/<account_id>/` on first use.

## Contact data boundary

Message Vault is **not** a contacts manager. It keeps only what browsing
needs:

- Normalized phone handles (E.164 where possible)
- Display names (`preferred_name` only in SQLite; CSV `first_name` /
  `last_name` columns are joined into `preferred_name` on import)
- Vault-owned labels (including Active / Inactive when present)

Uploaded VCF files are **transient**. The vault does not store the raw VCF,
emails, notes, photos, UIDs, or category provenance. After you confirm an
import, copied categories become ordinary vault labels you can rename or
edit independently of your address book.

## Contacts CSV

`contacts.csv` is **phone-only**. SQLite `contact_handles` holds phones plus
optional iMessage emails for thread linking; emails are not written to the CSV.

Header columns include `phones`, `first_name`, `last_name` (joined into the
SQLite `preferred_name` on import), `exclude` (kept for compatibility; new
writes leave it `false` and encode status as labels), and ordered
`label_1`…`label_N` (as many as needed). Legacy `group_1`…`group_N` aliases
are still accepted on import.

### Export contacts CSV

In the contacts ⋯ menu, **Export contacts CSV** downloads a vault-owned
projection built from SQLite for the signed-in account (`GET
/api/contacts/export-csv`). It includes sanitized phones, names, and **every**
current vault label. It is not a raw VCF re-export.

### CLI: import address book

Load an **iMazing Contacts CSV** or **VCF** into SQLite (replaces that account’s
contacts):

```bash
cargo run --release -- import-contacts \
  --account yourusername \
  --contacts path/to/contacts.vcf

# or iMazing export:
cargo run --release -- import-contacts \
  --account yourusername \
  --contacts path/to/Contacts.csv
```

The same `--contacts` flag is available on `import` / `ingest`. VCF
`CATEGORIES` (and legacy `[Tag]` markers in `FN`) become vault labels.

Optional helper `vcf-to-contacts` still writes the vault dual-write
`contacts.csv` mirror; prefer `import-contacts` for loading into the database.

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
(`contact_labels`). The contacts CSV mirror expands `label_N` columns as
needed when UI edits add more labels. Open a label page to browse its members.

## Unmapped handles

Handles with messages but no matching contact can appear in Trash workflows /
unassigned APIs. There is no dedicated `/unassigned` browse route.
