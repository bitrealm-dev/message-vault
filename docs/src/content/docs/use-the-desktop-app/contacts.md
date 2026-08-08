---
title: Work with contacts
description: Validate a contacts file or write a cleaned copy — improve name resolution in your exports.
---

The **Contacts** tab checks phone numbers in a VCF or contacts CSV. It can validate your file without changing it, or write a cleaned copy with corrected numbers. Better contacts data means more display names in your exports.

## What you can use

- A VCF (vCard) file
- A CSV with first name, last name, and phone columns

## Check contacts (dry run)

1. Open **Contacts**
2. Choose your contacts **File**
3. Keep **USA numbers** selected for US numbers, or clear it for international parsing
4. Select **Check**
5. Read the **Log** for uncertain numbers, duplicates, and the summary

Check is read-only. It does not write anything.

## Write a cleaned copy

1. Use the same file and country setting
2. Select **Update**
3. Read the **Log** for the written files

The app writes a new file next to the original — like `contacts-update.vcf` — and a log. It never overwrites your original. Only unambiguous phone numbers are changed; uncertain ones stay as they were.

Use the cleaned file when an export source asks for a contacts file. A missing contacts file usually means names are left blank rather than preventing the export from working.
