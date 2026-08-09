---
title: Contacts and labels
description: Browse contacts in the vault and understand how names and labels are stored.
---

## Browse contacts

Open **Contacts** in the sidebar to see people and handles discovered from your imports.

- Filter the list with the contacts filter control
- Open a contact to see related conversations and details
- Use [search](/browse/search/) in **Contacts** mode for name and handle queries

Contact data lives in the vault SQLite database for your signed-in account. Uploaded address-book files used during import are inputs for matching — the vault is not a full contacts manager (it does not keep raw VCF photos, notes, or category provenance as a sync’d address book).

## Names and handles

The vault keeps what browsing needs:

- Normalized phone handles (E.164 where possible) and other handles
- Display names for contacts and conversations
- Labels associated with contacts when present

During desktop **Import**, you can choose whether vault contact names fill missing names or overwrite names on the incoming export. See [Work with contacts](/use-the-desktop-app/contacts/).

## Labels

When labels exist for your account, they help organize contacts and can be used in search (`within:label`). Label membership is stored in SQLite for the account.

## Import contacts from the server CLI

Load a **VCF** or **vCard CSV** into the vault database for an account (replaces that account’s contacts):

```bash
cargo run --release -p message-vault-server -- import-contacts \
  --account yourusername \
  --contacts path/to/contacts.vcf
```

VCF `CATEGORIES` (and `[Tag]` markers in `FN`) can become vault labels. See [Server CLI](/reference/server-cli/) for the full command list.
