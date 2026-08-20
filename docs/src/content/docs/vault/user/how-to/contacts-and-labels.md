---
title: Contacts and labels
description: Browse contacts in the vault and choose how Import fills names.
---

## Browse contacts

Open **Contacts** in the sidebar to see people and handles discovered from imports.

- Filter the list with the contacts filter control
- Open a contact to see related conversations and details
- Use [search](/vault/user/how-to/search/) in **Contacts** mode for name and handle queries

The vault keeps display names, handles (E.164 phone numbers where possible), and labels. It is not a full address-book manager (no synced VCF photos or notes).

## Names during Import

On the desktop **Import** screen, choose how vault contact names apply to incoming messages:

- **Fill in missing names using vault contacts** — keep names already on the backup; fill blanks from the vault
- **Overwrite all import names with vault contacts** — prefer vault contact names for matching handles
- **Leave unknown names as is** — keep backup display names unchanged

Some backup types also accept a contacts file (VCF or contacts CSV) on the Import form.

## Labels

When labels exist for your account, they help organize contacts and can be used in search (`within:label`).

Loading a VCF into the vault from the terminal is a Developer command: [Server CLI](/vault/developer/reference/server-cli/).
