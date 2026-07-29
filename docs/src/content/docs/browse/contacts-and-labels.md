---
title: Contacts and labels
description: Active vs Inactive visibility, contacts CSV, VCF import, and sidebar labels.
---

## Active / Inactive

| Section | Meaning |
|---------|---------|
| **Active** | Non-excluded contacts with messages |
| **All** | Every contact with messages, including inactive |
| **Inactive** | `exclude=true` |

Visibility is driven by the `exclude` column in the **per-account** contacts
CSV:

`data/<account_id>/contacts.csv`

The files under `config/` (`contacts.csv.example`, and so on) are optional
templates only. Runtime imports create empty headers under
`data/<account_id>/` on first use.

## Contact data boundary

Message Vault is **not** a contacts manager. It keeps only what browsing
needs:

- Normalized phone handles (E.164 where possible)
- Display names
- Inactive (`exclude`) flag
- Vault-owned labels

Uploaded VCF files are **transient**. The vault does not store the raw VCF,
emails, notes, photos, UIDs, or category provenance. After you confirm an
import, copied categories become ordinary vault labels you can rename or
edit independently of your address book.

## Contacts CSV

`contacts.csv` is **phone-only**. SQLite `contact_handles` holds phones plus
optional iMessage emails for thread linking; emails are not written to the CSV.

Header columns include `phones`, `first_name`, `last_name`, `exclude`, and
ordered `label_1`…`label_N` (as many as needed). Legacy `group_1`…`group_N`
aliases are still accepted on import.

### Export contacts CSV

In the contacts ⋯ menu, **Export contacts CSV** downloads a vault-owned
projection built from SQLite for the signed-in account (`GET
/api/contacts/export-csv`). It includes sanitized phones, names, inactive
state, and **every** current vault label. It is not a raw VCF re-export.

### CLI: VCF → CSV

Convert a VCF address book offline (writes `contacts.csv`; does not open the
web preview):

```bash
cargo run --release -- vcf-to-contacts \
  --vcf path/to/contacts.vcf \
  --account yourusername
```

The converter reads `CATEGORIES` and legacy `[Tag]` markers in `FN`, then
writes dynamic `label_N` columns.

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
needed when UI edits add more labels. Labels list non-excluded contacts by
default.

## Unmapped handles

Handles with messages but no matching contact can appear in Trash workflows /
unassigned APIs. There is no dedicated `/unassigned` browse route.
