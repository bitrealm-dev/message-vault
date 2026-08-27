# WhatsApp Import methods design — 2026-08-26

Unify the desktop Import screen’s two WhatsApp sources (`WhatsApp - Android`
and `WhatsApp - iOS`) into one **WhatsApp** source with a Platform picker,
and expose the `wtsexporter` input flags those platforms actually need.

This spec records decisions from the 2026-08-26 design conversation. It is
not an implementation plan.

## Goal

A person importing WhatsApp should pick **WhatsApp**, then pick the phone:

1. Android — a folder that holds `msgstore.db` or `msgstore.db.crypt12` /
   `.crypt14` / `.crypt15`, plus optional contacts and media
2. iPhone — a Finder/iTunes backup folder that includes WhatsApp data

The form then shows only the fields that platform needs. The converter
already shells out to `wtsexporter` with `-a` or `-i`. The Import screen
does not yet present those as one workflow, and it does not forward a
decryption key, contacts database, media folder, message-database override,
or WhatsApp Business flag.

## Current product

The desktop Import screen (`web/src/screens/ImportScreen.tsx` and
`web/src/screens/import/ImportFormFields.tsx`) lists two WhatsApp sources:

- `whatsapp-android` — generic folder picker only
- `whatsapp-ios` — the same generic folder picker

`src-tauri` maps those ids to `WhatsappPlatform::Android` or
`WhatsappPlatform::Ios` and puts the folder on `ExporterConfig.inputs`.
`WhatsappConfig` is otherwise left empty: no key, no `-b`, no `-w`, no
`-m`, no `-d`, no `--business`. Attachment handling and vault contact merge
are not passed for WhatsApp either (those extract extras are only spread
for iMessage and SMS Backup & Restore today).

The command-line converter (`crates/exporters/whatsapp-exporter`) and the
legacy Slint GUI already accept:

- `--platform` `android` or `ios` (`-a` / `-i`)
- `--input` — search root for default filenames (the GUI omits this flag;
  the desktop app uses the chosen folder as that search root)
- `--backup` (`-b`) — Android crypt file or iPhone backup folder
- `--key` (`-k`) — key file path or crypt15 hex
- `--wa` (`-w`) — `wa.db` / `ContactsV2.sqlite`
- `--media` (`-m`) — WhatsApp media folder
- `--db` (`-d`) — explicit `msgstore.db`
- `--business` — iOS WhatsApp Business default files

`wtsexporter` writes JSON. The crate converts that JSON into JSON Lines.
HTML output is always disabled (`--no-html`). `--move-media` is never
passed: it would move the person’s media into a temp folder that is
deleted when the run finishes.

## Platforms

These are two real on-disk layouts. They are not a third “exported chat”
path. WhatsApp’s own Export chat `.txt` / zip (`wtsexporter -e`) is out of
scope.

| Platform | What the person points at | `wtsexporter` device flag | Decryption key |
|---|---|---|---|
| Android | A **folder** that contains `msgstore.db` and/or `msgstore.db.crypt12` / `.crypt14` / `.crypt15` | `-a` | Yes if a crypt file is used. Omit if decrypted `msgstore.db` is present. |
| iPhone | Backup **folder** (the device UUID directory from Finder/iTunes) | `-i` plus `-b` | No. This is not an Apple backup-password field. |

Default Platform on first open: **Android** (same as `WhatsappPlatform` in
`message-vault-io-core`).

## UI structure

Keep iMessage, SMS Backup & Restore, and the other non-WhatsApp sources as
they are.

Replace the two WhatsApp rows in the Import source list with one row:

- Label: **WhatsApp**
- When WhatsApp is selected, show a second dropdown labeled **Platform**:
  - Android
  - iPhone

Do not show `-a` / `-i` as their own controls. Derive the flag from
Platform.

## Internal ids

The visible source is one WhatsApp entry. Each platform still has its own
id so staging folders, remembered paths, and the Tauri extract command stay
distinct. An Android dump folder and an iPhone backup folder must not share
one remembered path.

| UI platform | Internal id | Staging slug |
|---|---|---|
| Android | `whatsapp-android` | `whatsapp-android` (unchanged) |
| iPhone | `whatsapp-ios` | `whatsapp-ios` (unchanged) |

Remembered importer paths (and optional contacts / media / message-database
overrides) are stored **per method id**. Switching Platform does not copy
an Android folder into the iPhone backup field.

Existing `whatsapp-android` / `whatsapp-ios` remembered backup paths keep
working.

Never persist the decryption key (file path or hex).

## Form fields (locked)

Shared for both platforms:

- Attachments: Copy / Convert / Compress / Skip (existing vault media
  pipeline, including compress extras). Pass these through extract for
  WhatsApp the same way iMessage and SMS Backup & Restore already do.
- Vault contacts: fill missing / overwrite / as-is (existing import merge).
  This is **not** `wtsexporter -w`.

Per platform:

| Field | Android | iPhone |
|---|---|---|
| Backup folder | Required. Folder picker. Search root for `msgstore.db` / crypt files / `wa.db` / `WhatsApp` media. | Required. Folder picker. Forwarded as `-b`. |
| Decryption key | Shown. Optional until a crypt file is used. Key file path or crypt15 hex (`-k`). | Hidden |
| Contacts database (`-w`) | Optional. `wa.db`. Leave empty if that file is in the backup folder. | Optional. `ContactsV2.sqlite`. Leave empty if that file is in the backup. |
| Media folder (`-m`) | Optional. Leave empty if a `WhatsApp` media folder is in the backup folder. | Hidden. Media lives inside the iPhone backup. |
| Message database (`-d`) | Optional. Leave empty if `msgstore.db` is in the backup folder. | Hidden. The hashed database lives inside the backup. |
| WhatsApp Business | Hidden | Optional checkbox, off by default (`--business`) |

Empty optional `-w` / `-m` / `-d` values are omitted so `wtsexporter`
defaults still run from the backup folder.

Stars and `(Optional)` follow the iMessage Import rules: a red `*` only
when there is no usable default. Empty optional path fields say
**(Optional)**. Platform, Attachments, vault Contacts, and the Business
checkbox stay unmarked.

## `wtsexporter` flags: keep, hide, or skip

“Most support” means every extraction path that **works** is reachable. It
does not mean showing every flag in `--help`.

| Flag | Decision |
|---|---|
| `-a` / `-i` | Supported, hidden. Derived from Platform. |
| `-b` `--backup` | Supported. iPhone: the backup folder. Android: auto-set when a crypt file in the folder is used. |
| `-k` `--key` | Add on Android only. File path or hex. Never prompt on stdin. |
| `-w` `--wa` | Add as optional file picker. Omit when empty. |
| `-m` `--media` | Add as optional folder picker on Android. Omit when empty. Hide on iPhone. |
| `-d` `--db` | Add as optional file picker on Android. Omit when empty. Hide on iPhone. |
| `--business` | Add as a checkbox on iPhone only. |
| `-o` / `-j` / `--no-html` / `--no-banner` | Supported, hidden. Staging directory and JSON output stay app-owned. |
| `-c` `--move-media` | Skip. Never pass. Copy media only. |
| `-e` `--exported` | Skip. WhatsApp Export chat `.txt` is not a Platform. |
| `--call-db` | Skip. Call history is not a vault conversation in this pass. |
| `--wab` / `--wa-backup` | Skip as a field. Decrypted `wa.db` is the optional Contacts database. Do not auto-pass `--wab` for `wa.db.crypt15`. |
| `--enrich-from-vcards` / `--default-country-code` | Skip. Vault Contacts fill names after import. |
| HTML / JSON pretty-print / `--tg` / `--per-chat` / `--import` / `--txt` | Skip. |
| Incremental merge / `--source-dir` / `--target-dir` | Skip. |
| `--date` / `--include` / `--exclude` / `--time-offset` / `--dont-filter-empty` | Skip. |
| `--debug` / `--showkey` / `--check-update` / `--assume-first-as-me` / `--decrypt-chunk-size` / `--max-bruteforce-worker` / `--fix-dot-files` | Skip. |
| Apple device-backup password | Skip. `wtsexporter --help` has no password flag. Do not put an Apple backup-password field on this form. Correct the User Guide line that tells people to type one here. |

## Converter wiring (implementation notes, not a plan)

The web Import job and Tauri extract options today pass the folder and the
platform. They do not pass key, `-b`, `-w`, `-m`, `-d`, or `--business`.
Those fields already exist on `WhatsappConfig` in
`message-vault-io-core`. Plumb them through for `whatsapp-android` and
`whatsapp-ios`.

The WhatsApp key is **not** `ExtractArgs.backup_password`. That field is
the iMessage iPhone backup password. Add separate extract fields for
WhatsApp.

For iPhone, the chosen folder is both the extract `path` (search root) and
`WhatsappConfig.backup` (`-b`). Today iOS WhatsApp Import never sets `-b`,
which is why a Finder backup often cannot be read unless the hashed
database was copied to the folder root.

### Android crypt file in the folder

Look in the backup folder root only (no recursive walk):

1. If `msgstore.db` exists, use it. Do not pass `-b` or `-k`.
2. Else if `msgstore.db.crypt12`, `.crypt14`, or `.crypt15` exists, pass
   that file as `-b`. The key field is then required and is passed as `-k`.
3. If both a decrypted database and a crypt file exist, prefer
   `msgstore.db` (step 1).

The form uses the same rule to show a red star on the key field. The
converter uses the same rule so CLI `--input` matches Import.

Stdin stays closed (`Stdio::null()`). An empty `-k` on crypt15 must not
prompt.

## Out of scope

- Exported chat files (`-e`) and `--assume-first-as-me`
- `--call-db`, `--wab` as fields or auto-pass
- HTML/JSON pretty-print, Telegram, incremental merge, date filters, vCard
  enrich, `--move-media`
- Apple backup password on the WhatsApp form
- Changing how `wtsexporter` decrypts iPhone *device* backups
  (`iphone_backup_decrypt`)
- Call logs as vault conversations
- Legacy Slint GUI
- Obfuscate on the WhatsApp form (stays iMessage iPhone and SMS Backup &
  Restore)
- Implementation sequencing (a separate plan after this spec is accepted)

## Validation and defaults (locked)

A path the user typed or picked must exist.

### Defaults

- First visit: source WhatsApp, Platform **Android**, all paths empty,
  Business off.
- Never pre-fill home-directory paths. Android dumps and iPhone backups
  have no single standard location on this machine.

### Enable Import when

| Platform | Button enabled |
|---|---|
| Android | Backup folder is a non-empty existing directory. If a crypt file will be used (step 2 above), the key is also non-empty. |
| iPhone | Backup folder is a non-empty existing directory. |

If an optional contacts, media, or message-database field has a value, that
path must exist and be the right kind or the button stays disabled.

### Path kind (fail before a long run)

- Backup folder path is a file: field error `Pick the backup folder.`
- Optional contacts or message-database path is a directory: field error
  `This path must be a file.`
- Optional media path is a file: field error `This path must be a folder.`

### Android decryption key

The GUI must never prompt on a terminal.

When the folder has no `msgstore.db` and does have a crypt file, treat the
key as **required** to enable Import (red star). If the folder cannot be
read yet, leave the key optional and fail after start if `wtsexporter`
cannot decrypt.

Do not pass `-k` unless a crypt file is being forwarded as `-b`.

## User-facing errors (locked)

Scope is the WhatsApp Import form plus extract/converter failures after
Import is clicked. Vault upload errors stay as they are today.

### Where a message appears

- **Under the field**, and Import stays disabled, for anything the form
  can know before a run.
- **In the progress summary** after Import is clicked, for extract and
  converter failures.

Required path empty: no extra sentence. Import stays disabled. The label
already has a red star.

### Form (under the field)

| When | Copy |
|---|---|
| Typed or picked path does not exist | `This path does not exist.` |
| Backup folder path is a file | `Pick the backup folder.` |
| Android crypt file, no decrypted `msgstore.db`, key empty | `Decryption key is required for an encrypted backup.` |
| Optional contacts or message-database path is a directory | `This path must be a file.` |
| Optional media path is a file | `This path must be a folder.` |

### After start (summary)

`wtsexporter` missing from `PATH` stays a run error in the log (existing
resolver message). Convert/Compress still needs ffmpeg; keep the existing
media-pipeline error for that case. Do not invent a second WhatsApp-only
ffmpeg sentence.

## Docs

- `docs/src/content/docs/vault/user/import-from-a-backup.md`: one WhatsApp
  source, Platform Android / iPhone, stars vs (Optional).
- `docs/src/content/docs/vault/user/prepare-a-backup/android-whatsapp.md` and
  `docs/src/content/docs/vault/user/prepare-a-backup/iphone-whatsapp.md`:
  “open Import, choose WhatsApp, set Platform.”
- Remove the iPhone WhatsApp line that tells people to type an Apple backup
  password into this form.

## Success

- One WhatsApp source in the Import list; two named platforms; fields match
  the locked table.
- Android decrypted `msgstore.db` imports without a key.
- Android crypt12/14/15 imports pass `-b` and `-k` when the crypt file sits
  in the chosen folder and `msgstore.db` does not.
- iPhone Finder/iTunes backup folders are forwarded as `-b`.
- Optional `-w` / `-m` / `-d` are omitted when empty.
- `--business` is passed only for iPhone when the checkbox is on.
- `--call-db`, `--wab`, `-e`, and `--move-media` are not passed.
- The decryption key is never written to `export.ini` or remembered paths.
- Form errors for WhatsApp use the locked catalog. Vault upload errors are
  unchanged.
