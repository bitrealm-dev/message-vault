---
title: Contacts and labels
description: Active vs Inactive visibility, contacts CSV, and sidebar labels.
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

## Contacts CSV

`contacts.csv` is **phone-only**. SQLite `contact_handles` holds phones plus
optional iMessage emails for thread linking; emails are not written to the CSV.

Default header columns include `phones`, `first_name`, `last_name`, `exclude`,
and `label_1`…`label_5` (legacy `group_1`…`group_5` aliases are still accepted
on import).

Convert a VCF address book:

```bash
cargo run --release -- vcf-to-contacts \
  --vcf path/to/contacts.vcf \
  --account yourusername
```

## Labels

Create and manage labels in the sidebar. Membership is stored in SQLite
(`contact_labels`), not as CSV “groups”. Labels list non-excluded contacts by
default.

## Unmapped handles

Handles with messages but no matching contact can appear in Trash workflows /
unassigned APIs. There is no dedicated `/unassigned` browse route.
