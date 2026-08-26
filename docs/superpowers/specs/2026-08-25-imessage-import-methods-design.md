# iMessage Import methods design — 2026-08-25

Unify the desktop Import screen’s two Apple sources (`iPhone - iOS` and
`iMessage - macOS`) into one **iMessage** source with three extraction
methods, and expose the converter flags that those methods actually need.

This spec records decisions from the 2026-08-25 design conversation, with
validation, defaults, and user-facing error copy locked on 2026-08-26. It
is not an implementation plan.

## Goal

A person importing Apple Messages should pick **iMessage**, then pick how
the data was obtained:

1. Local Mac Messages (`chat.db`)
2. An iPhone Finder/iTunes backup folder (unencrypted or encrypted)
3. A jailbroken iPhone filesystem copy (`sms.db` plus attachment folders)

The form then shows only the fields that method needs. The converter already
understands Mac vs iOS layouts, encrypted backups (via crabapple), a custom
attachment root, and an Apple AddressBook file. The Import screen does not
yet present those as one workflow, and jailbreak is not a first-class
method.

## Current product

The desktop Import screen (`web/src/screens/ImportScreen.tsx` and
`web/src/screens/import/ImportFormFields.tsx`) lists two Apple sources:

- `imessage-ios` — backup folder, optional encryption password, attachment
  handling, vault contact merge
- `imessage-macos` — generic folder picker only; no password, no attachment
  handling, no attachment root, no Apple Contacts file

`src-tauri` maps those source ids to `ApplePlatform::Ios` or
`ApplePlatform::MacOs`. Staging folder names use slugs `iphone-ios` and
`macos`.

The converter (`crates/exporters/imessage-ir-exporter`) already accepts:

- `--platform` `macOS` or `iOS`
- input path (`chat.db` on Mac, backup root on iOS)
- `--copy-method` (the GUI maps Copy/Convert/Compress/Skip onto the vault
  media pipeline instead of upstream ImageMagick modes)
- `--attachment-root` (Mac layout only; no effect on iOS backups)
- `--backup-password` (iOS backups only)
- Apple Contacts path (`AppleConfig.apple_contacts`; Mac layout only — the
  converter logs that a manual contacts path has no effect on iOS)
- staging output directory (chosen by the app, not the user)

## Extraction methods

These are three real on-disk layouts, not a raw `macOS` / `iOS` dropdown.
Jailbreak uses the **Mac** database schema with a custom attachment root.
Treating jailbreak as an iOS *backup* would look in the hashed backup
layout and miss files.

| Method | What the person points at | Converter `--platform` | Password | Attachment root |
|---|---|---|---|---|
| Mac Messages | `chat.db` | `macOS` | No | Optional. Default is the Messages folder next to the database (`~/Library/Messages` on a live Mac). |
| iPhone backup | Backup **folder** (the device UUID directory), not `sms.db` inside it | `iOS` | Yes if the backup is encrypted; omit if not | Hidden. Ignored by the converter. Unencrypted backups are read in place. Encrypted backups are decrypted with crabapple. |
| Jailbroken iPhone | `sms.db` (same schema as Mac `chat.db`) | `macOS` | No | Required. Absolute path to the folder that **contains** `Attachments` and `StickerCache` (the Messages root, not a backup hashed tree). |

## UI structure

Keep WhatsApp, SMS Backup & Restore, and the other non-Apple sources as they
are.

Replace the two Apple rows in the Import source list with one row:

- Label: **iMessage**
- When iMessage is selected, show a second dropdown for the extraction
  method:
  - Mac Messages
  - iPhone backup
  - Jailbroken iPhone

Do not show `--platform` as its own control. Derive it from the method
table above.

Default method on first open: **iPhone backup** (same as today’s default
source `imessage-ios`).

## Internal ids

The visible source is one iMessage entry. Each method still has its own id
so staging folders, remembered paths, and the Tauri extract command stay
distinct. A backup folder and an `sms.db` file must not share one remembered
path.

| UI method | Internal id | Staging slug |
|---|---|---|
| Mac Messages | `imessage-macos` | `macos` (unchanged) |
| iPhone backup | `imessage-ios` | `iphone-ios` (unchanged) |
| Jailbroken iPhone | `imessage-jailbreak` | `iphone-jailbreak` (new) |

Remembered importer paths (and, for Mac/jailbreak, attachment folder and
Apple Contacts file) are stored **per method id**. Switching methods does
not copy a `chat.db` path into the backup-folder field.

Existing `imessage-ios` / `imessage-macos` remembered paths keep working.
Jailbreak is a new id, not a reuse of iOS.

## Form fields (locked)

Shared for all three methods:

- Attachments: Copy / Convert / Compress / Skip (existing vault media
  pipeline, including compress extras). Show this for Mac and jailbreak as
  well as iPhone backup. Do not add a second ImageMagick-style copy-method
  picker.
- Vault contacts: fill missing / overwrite / as-is (existing import merge).
  This is **not** `--contacts-path`.

Per method:

| Field | Mac Messages | iPhone backup | Jailbroken iPhone |
|---|---|---|---|
| Database / backup path | File picker for `chat.db` | Folder picker for the backup root | File picker for `sms.db` |
| Encryption password | Hidden | Shown, optional until the backup is encrypted | Hidden |
| Attachment folder (`--attachment-root`) | Optional. Hint: folder that contains `Attachments` and `StickerCache`. Needed when those folders are not next to `chat.db`. | Hidden | Required. Same hint. Import stays disabled until this folder is set. |
| Apple Contacts file (`--contacts-path`) | Optional. `AddressBook-v22.abcddb` or `AddressBook.sqlitedb`. On a live Mac, empty means scan the local AddressBook. | Hidden. The backup’s AddressBook is used instead. A manual path has no effect on the iOS platform. | Optional. Same files. Local Mac AddressBook scan will not find a phone copy. |

Empty optional attachment-root and contacts-path values are omitted so Mac
auto-scan and default attachment layout still run.

## Upstream flags: keep, hide, or skip

Mapping against `imessage-exporter`-style flags. “Most support” means every
extraction path that **works** is reachable. It does not mean showing flags
the converter ignores.

| Flag | Decision |
|---|---|
| `-c, --copy-method` | Supported via the existing Attachments control (Copy/Convert/Compress/Skip). |
| `-o, --export-path` | Supported, hidden. Staging directory stays app-owned. |
| `-p, --db-path` | Add. Path picker type depends on method (file vs folder). |
| `-a, --platform` | Do not show. Derive from method. |
| `-r, --attachment-root` | Add for Mac (optional) and jailbreak (required). Hide for iPhone backup. |
| `-x, --cleartext-password` | Keep on iPhone backup only. Feed the library from the form field. Do not prompt in a terminal. |
| `-n, --contacts-path` | Add as optional file picker on Mac and jailbreak. Hide for iPhone backup. |
| `-s` / `-e` dates | Intentionally unused in Import. Backend can still accept them. |
| `-t` conversation filter | Intentionally unused. |
| `-m` custom name / `-i` use-caller-id | Unused. JSONL import treats the signed-in vault account as the owner. |
| `-b` ignore disk warning | Intentionally unused. |
| `--use-message-times` | Skip. It stamps HTML/txt transcript headers and sets filesystem creation times on copied attachment files (macOS/Windows). JSONL already stores message timestamps. The vault does not read Finder/Windows birth times. |

`--contacts-path` names people **during export** from Apple’s AddressBook.
The Import “Contacts” dropdown names people **after import** from vault
contacts. Both stay. Showing both is intentional: a copied `chat.db` /
`sms.db` on Linux will not auto-find AddressBook, and vault contacts may
still be empty on a first import.

## Converter wiring (implementation notes, not a plan)

The web Import job and Tauri extract options today pass the backup path,
password, and attachment/media settings. They do not pass `attachment_root`
or `apple_contacts`. Those fields already exist on `Form` /
`AppleConfig` in `message-vault-io-core`. Plumb them through for
`imessage-macos` and `imessage-jailbreak`.

Jailbreak uses the same `ApplePlatform::MacOs` path as Mac Messages, plus
attachment root (and optional Apple Contacts).

## Out of scope

- Changing WhatsApp or other non-Apple Import sources
- Date range, conversation filter, disk-space bypass, owner display-name
  flags, `--use-message-times`
- Showing `--attachment-root` or `--contacts-path` on iPhone backup
- Replacing vault contact merge with Apple AddressBook (both remain)
- Implementation sequencing (a separate plan after this spec is accepted)

## Validation and defaults (locked)

A path the user typed or picked must exist. Auto-detect finding no
AddressBook is different: that is a warning and the run continues without
those names.

### Defaults

- First visit: source iMessage, method **iPhone backup**, all paths empty.
- Mac Messages **on macOS only**, and only if `~/Library/Messages/chat.db`
  exists: pre-fill that file. Leave attachment folder and Apple Contacts
  empty so the converter’s normal defaults still apply.
- iPhone backup, Jailbroken iPhone, Linux, and Windows: never pre-fill
  home-directory paths. Finder/iTunes backups live under a device UUID
  folder, and jailbreak copies have no standard location on this machine.

### Enable Import when

| Method | Button enabled |
|---|---|
| Mac Messages | `chat.db` path is non-empty |
| iPhone backup | Backup folder path is non-empty. Password may be empty until encryption is known. |
| Jailbroken iPhone | `sms.db` path **and** attachment folder are both non-empty |

If an optional attachment folder or Apple Contacts file has a value, that
path must exist or the button stays disabled (same rule as failing
immediately with a field error).

### Path kind (fail before a long run)

- iPhone backup, path is a file: field error
  `Pick the backup folder, or switch to Jailbroken iPhone.`
- Mac, path is a directory: field error `Pick chat.db.`
- Jailbreak, path is a directory: field error `Pick sms.db.`

### Encrypted backup password

The GUI must never prompt on a terminal. If the backup is encrypted and the
password field is empty, do not call the converter’s stdin prompt (in the
desktop app stdin is not a TTY).

When the chosen folder contains `Manifest.plist` marked encrypted, treat
the password as **required** to enable Import. If encryption cannot be read
(no plist yet, unreadable folder), leave the password optional and fail
after start with the locked string in **User-facing errors**.

Wrong password and leftover password on an unencrypted backup use the
locked strings in that same section. A leftover password must not be
silently ignored.

### Apple Contacts

- User supplied a file that does not exist: fail with the locked Apple
  Contacts missing-file string.
- Field empty and auto-scan finds nothing: continue; that is a warning,
  not a failed Import. Vault contacts can still fill names at import.

## User-facing errors (locked)

Scope is the iMessage Import form plus extract/converter failures after
Import is clicked. Vault upload errors stay as they are today.

Import-language strings for cases a person can act on live in the
converter (and the form helpers), so the CLI and the desktop app share
one sentence. Disk I/O, SQLite permission failures, crabapple parse
failures other than “not a backup”, cancel, and “media processing failed
for all candidate files” stay engine text in the progress summary.

`RuntimeError::InvalidOptions` must not prefix those sentences with
`Invalid options!`. The summary shows the sentence from the tables
below.

### Where a message appears

- **Under the field**, and Import stays disabled, for anything the form
  can know before a run.
- **In the progress summary** after Import is clicked, for extract and
  converter failures.

Required path empty: no extra sentence. Import stays disabled. The label
already says (required).

### Form (under the field)

| When | Copy |
|---|---|
| Typed or picked path does not exist | `This path does not exist.` |
| iPhone backup path is a file | `Pick the backup folder, or switch to Jailbroken iPhone.` |
| Mac path is a directory | `Pick chat.db.` |
| Jailbreak path is a directory | `Pick sms.db.` |
| Attachment folder is a file | `Pick the folder that contains Attachments and StickerCache.` |
| Apple Contacts path is a directory | `Pick AddressBook-v22.abcddb or AddressBook.sqlitedb.` |
| `Manifest.plist` says encrypted, password empty | `The backup is encrypted — fill Encryption password.` |

### After start (summary)

Same string on the CLI.

| When | Copy |
|---|---|
| Encrypted backup, password empty (plist unread at form time) | `The backup is encrypted — fill Encryption password.` |
| Wrong password | `The iOS backup password was incorrect.` |
| Password set on an unencrypted backup | `This backup is not encrypted. Clear Encryption password.` |
| Attachment folder missing after start | `Attachment folder does not exist.` |
| Apple Contacts file missing after start | `Apple Contacts file does not exist.` |
| `chat.db` / `sms.db` missing or not a file | `Messages database does not exist.` |
| iPhone folder is not a backup, or Messages is missing inside it | `This folder is not an iPhone backup, or Messages is missing from it.` |
| Convert/Compress and ffmpeg/ffprobe missing | `Convert and Compress need ffmpeg and ffprobe. Put them on PATH, or in the desktop app set the ffmpeg directory in Settings → System.` |

### Warnings (run continues)

These are not a failed Import.

- Apple Contacts empty and auto-scan finds nothing: keep
  `Unable to build contacts index: … Continuing without contact names...`
  as a log warning.
- iPhone backup Contacts database cannot be decrypted: keep the existing
  continue-without-contacts log line.

## Success

- One iMessage source in the Import list; three named methods; fields match
  the locked table.
- iPhone backups (plain and encrypted) keep working.
- Mac `chat.db` imports can copy attachments when the media folder is beside
  the database or when an attachment root is supplied.
- Jailbreak `sms.db` imports are possible by treating the database as Mac
  layout and requiring an attachment root.
- Optional Apple Contacts file on Mac/jailbreak can supply names at export
  time. iPhone backup still uses the AddressBook inside the backup.
- `--use-message-times` is not in the UI and is not passed.
- Form and extract errors for iMessage use the locked catalog. Vault
  upload errors are unchanged.
