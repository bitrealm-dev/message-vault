# iMessage Backup Identity Check — Design

Date: 2026-08-31
Status: agreed (interview completed 2026-08-31; every question answered by the user)

## Problem

The iMessage import path (`imessage-ios`, `imessage-macos`, `imessage-jailbreak`)
has no identity check at all. Nothing compares the backup being imported against
the account importing it, so a backup made by someone else's device imports as
the account owner: the other person's sent messages become `is_from_me` rows
indistinguishable from the owner's, and the owner's own number appears as an
ordinary incoming contact.

A mismatch has no other mechanical consequence. Attribution runs on
`account_id` (the importing account) and `is_from_me` (copied from Apple's
`message.is_from_me`) regardless of any identity anywhere. The harm is entirely
the wrong-backup case, so the feature is a guard, not a boundary: it interferes,
it never refuses. The person running the desktop app owns the vault.

## What was decided

### The identity set

The union of three reads, deduplicated:

| Signal | Scope | Handling |
| --- | --- | --- |
| `chat.account_login` | all three methods | strip the `P:` / `E:` prefix, drop empty remainders |
| `message.destination_caller_id` | all three methods | strip a `tel:` prefix, drop NULLs |
| `Info.plist` → `Phone Number` | `imessage-ios` only | read as written (human-formatted), then normalize |

Measured on a real 140 MB / 86,897-message backup: the
`destination_caller_id` distinct scan takes 0.05 s and the `account_login`
scan 0.0002 s, so both stay in. Real data observed on that backup, which the
cleaning rules must survive:

- `account_login` values: `P:+19412660605`, `E:mjbeisser@gmail.com`, and —
  28% of chats — the bare prefix `E:` with nothing after it. A naive
  non-empty test passes on all of those; the check must strip the prefix and
  test the remainder.
- `destination_caller_id` values: `+19412660605`, `tel:+19412660605`,
  `mjbeisser@gmail.com`, and NULL.
- `Info.plist` `Phone Number`: `+1 (941) 266-0605` — needs digit
  normalization before it can match anything.
- A dual-SIM device whose second number appears in **none** of the three
  signals. A real device can carry an identity absent from its own backup,
  which is why a mismatch is a stop the user can continue through, never a
  refusal.

Values containing `@` are emails; everything else is a phone. Phones
deduplicate and compare by US national digits (strip non-digits; drop a
leading `1` from an 11-digit number — the same rule as the existing
`toUsNationalDigits` in `web/src/lib/phoneTokens.ts` and the vault's
`sanitize_number`). Emails deduplicate and compare lowercased.

### When the check runs

Immediately after the source opens, before any message is parsed and before
the import session is created. One probe, one code path, one reader of the
backup: a public `backup_identities` function in `imessage-ir-exporter` opens
the source through the same `DataSource::from` the real run uses, runs the two
queries, reads `Info.plist`, and returns the cleaned list. Mac `chat.db` and
jailbreak `sms.db` reach that point instantly; an encrypted iOS backup reaches
it after the unlock and single-file `chat.db` decrypt it must do anyway.

Cost accepted knowingly: for an encrypted iOS backup, the probe decrypts
`chat.db` (and the contacts database) to a temp file once for the check and
the real run decrypts again — a few seconds on a 140 MB database. Keeping one
code path is worth more than saving that.

The check stays **out of the import stage machine**. The six session stages
(`parse`, `write`, `awaiting_gate_1`, `transcode`, `awaiting_gate_2`,
`pushing`) are unchanged; the desktop simply does not create the session until
the user is past the check. Cancel at the stop therefore has nothing to clean
up: no session row, no staging folder.

### Comparison

The device identity set is compared against the account's **phones and
emails** (`GET /v1/account/profile` → `phones`, `emails`). Any overlap is a
match. Two of the three identities on the reference backup are Apple ID
emails, which is why emails count.

The comparison happens client-side in the web app, mirroring how the SBR
mismatch check works, with the same fail-open rule: a failed profile fetch
never blocks an import.

### Behaviour

- **Some identity matches** → the import proceeds with no interruption. The
  identity list appears as a section on Gate 1, each address marked as on the
  profile or not, with an inline "Add to profile" action on the unmatched
  ones.
- **Nothing matches** (including a profile with no handles at all) → a stop
  screen before the session is created: the identity list with its marks,
  and two actions — **Continue import** and **Cancel**. No acknowledgment
  checkbox: a mismatch has no mechanical consequence, so a checkbox sentence
  would have to invent one, which is the warning-label shape this product's
  copy avoids. Cancel returns to the form; nothing was created.
- **No signal at all** (probe returned an empty list, or errored) → the
  import proceeds. The Gate 1 section states, as a fact, that the backup
  doesn't record which account it came from. No block, no acknowledgment.
- **Adding an address** calls the existing `POST /v1/account/profile`
  endpoint with one `{handle, service}` — no new server endpoint. The
  comparison re-runs against the updated profile, so claiming the device's
  address on the stop screen resolves the mismatch in place.
- **Probe errors fail open.** A source the probe cannot read will fail
  identically in the extractor seconds later with the proper error message
  (wrong password, not a backup, …); the probe returning an empty list keeps
  error reporting in one place.

### Recording

The cleaned identity list is stored on the import session
(`vault_imports.source_identities`, JSON array, nullable) and returned from
`GET /v1/imports/active`. Gate 1 on a resumed session reads it from there
instead of re-reading the backup. That takes a schema bump
(`SCHEMA_VERSION` 5 → 6); there is no migration before the first stable
release, so the bump is free.

### Copy

Statements of fact only, per the product copy rule:

- Stop heading: **"None of the addresses this backup sent from are on your
  profile."**
- Match mark: "On your profile" / unmatched: "Not on your profile", with
  "Add to profile".
- No signal: "This backup doesn't record which account it came from."
- Buttons at the stop: "Continue import" / "Cancel".

## Explicitly not doing

- No hard block, ever — a real device can carry an identity its own backup
  does not record (the dual-SIM case above).
- No automatic profile writes; adding an address is always the user's click.
- No change to how messages are attributed (`account_id` + `is_from_me`).
- `vault-push` (CLI) stays uncovered; this is a desktop Import feature.
- The SBR owner-phones check is untouched.
- Settings → Storage could show the recorded identities; it is not wired
  here.
- Published docs are not updated here.

## Interview trail

Q1 purpose: wrong-backup guard. Q2 signal: union of both DB columns plus
`Info.plist`. Q3 placement: after the source opens, before parsing. Q4 keep
`destination_caller_id` (measured, not guessed). Q5 no signal → proceed and
say so. Q6 compare against phones **and** emails. Q7 list always shown, stop
only on no-match. Q8 inline add via the existing profile endpoint. Q9 stop
offers Continue / Cancel, no checkbox. Options artifact:
https://claude.ai/code/artifact/3ea33692-7d2b-4c31-b106-d310e630563d
