---
title: Work with contacts
description: Resolve display names during import and browse contacts in the vault.
---

Contacts show up in two places: as optional name data when you extract or import, and as the **Contacts** list after messages are in the vault.

## During import

On the desktop app **Import** screen you can choose how vault contact names apply to incoming messages:

- **Fill in missing names using vault contacts** — keep names already on the export; fill blanks from the vault
- **Overwrite all import names with vault contacts** — prefer vault contact names for matching handles
- **Leave unknown names as is** — keep backup display names unchanged; do not fill from vault contacts

Some backup types also accept a contacts file (VCF or contacts CSV) when preparing or extracting. See the [prepare your backups](/prepare-your-backups/iphone-ipad/) guides for when a contacts export helps name resolution.

Better contacts data means more readable display names in conversation lists.

## In the vault (browse)

After you sign in (browser or desktop app):

1. Open **Contacts** in the sidebar
2. Filter or search the list
3. Open a contact to see related conversations and details

Labels and contact grouping in the vault come from imported data and profile settings, not from a separate “Contacts check” tab in the desktop app.

## Cleaning a contacts file before import

If you need to validate or normalize phone numbers in a VCF or CSV before using it with an exporter, prepare that file with your usual contacts tools, then point the extract/import flow at the cleaned file. The vault does not require a separate Check/Update contacts tab for day-to-day use.

## Related

- [Browse contacts](/browse/contacts-and-labels/)
- [Import into the vault](/use-the-desktop-app/import-into-vault/)
