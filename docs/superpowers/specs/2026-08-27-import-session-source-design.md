# Map Import session source to IR source — 2026-08-27

Open the desktop Import session with the chat-kind slug the conversation
files already carry, not the Platform method id. This spec records
decisions from the 2026-08-27 design conversation for
[issue 203](https://github.com/bitrealm-io/message-vault/issues/203).
It is not an implementation plan.

## Goal

Desktop Import should upload iMessage and WhatsApp conversations the
same way it did before the unified Platform picker.

The vault import session (the numbered run the server tracks) and each
conversation request must use the same source string. After that, the
vault’s match check succeeds. Conversations import, append, and dedupe
as they do today.

Mac and iPhone stay in one vault bucket. Existing `messages.source`
rows and asset folders under `data/<account>/imessage/` (and
`…/whatsapp/`) keep matching.

## Current product

The desktop Import form stores two kinds of id on the same field,
`form.source`:

- **Method id** — which backup layout to parse. Examples:
  `imessage-ios`, `imessage-macos`, `imessage-jailbreak`,
  `whatsapp-android`, `whatsapp-ios`. This picks form fields, the
  extract command, the staging folder, and remembered paths.
- **IR source** — what kind of chat the JSONL is. Each exporter writes
  one `export.source` in the conversation header (`EXPORT_SOURCE` in
  the emit crate). Examples: `imessage`, `whatsapp`,
  `sms-backup-restore`. The vault stores that slug on
  `messages.source` and under `data/<account>/<source>/assets/`.

The unified Platform picker set `form.source` to the method id. The
job hook (`web/src/screens/import/useImportJob.ts`) then opens the
session with that same value:

```text
POST /v1/imports  { source: form.source, … }
```

`vault-push` reads `export.source` from each JSONL file and sends that
on the conversation request. The server
(`require_reusable_import` in
`crates/vault/server/src/db/vault_imports.rs`) rejects the request
when `row.source != source`.

A 2026-08-27 iPhone Import failed every conversation:

```text
import 5 source mismatch (session=imessage-ios, request=imessage)
```

`import 5` is `vault_imports.id`, not a count of five errors. Staging:
`staging-iphone-ios-260827-003815`. Report: 681 conversations failed,
0 succeeded, 85,476 messages attempted, 0 inserted. Nothing from that
run was stored. Attachments were skipped because their hashes were
already in the vault from an earlier import that used source
`imessage`.

CLI `vault-push` without a pre-created session never hits this. It
opens the session from the JSONL header, so both sides are
`imessage`. The desktop job is what puts the method id on the
session.

WhatsApp JSONL is `export.source = "whatsapp"`. After the unified
WhatsApp form, a desktop Import opens the session as
`whatsapp-android` or `whatsapp-ios` and hits the same check.

Older iPhone Imports that opened the session as `imessage` succeeded.

## Non-goals

- Group identical rows in the Import Errors table. That is
  [issue 202](https://github.com/bitrealm-io/message-vault/issues/202).
- Change JSONL `schema_version` or `export.source` in the IR
  exporters. Writing `imessage-ios` into the header would split Mac
  vs iPhone in `messages.source` and under
  `data/<account>/imessage-ios/` vs `…/imessage/`.
- Teach `vault-push` to send method ids. After the session source is
  correct, the JSONL slug already matches.
- Change the vault’s match check to accept method ids as aliases of
  `imessage` or `whatsapp`. That would store extract-layout names on
  new sessions and drift from older rows.
- Migrate existing `messages.source` values. They should already be
  `imessage` / `whatsapp`.
- Change saved-group names. After a run that inserted messages, the
  local group is still named from the method id (for example
  `Import imessage-ios 2026-08-27`) so iPhone vs Mac vs jailbreak
  stays distinguishable.
- Add Playwright coverage. Import is desktop-only. Tests are Vitest.

## Decisions

1. **Keep `imessage` and `whatsapp` as the vault source.** Mac and
   iPhone stay in one bucket. Method ids stay extract, staging,
   remembered-path, and saved-group only.
2. **Map only when creating the session.** When the desktop job posts
   `/v1/imports`, send the IR source, not `form.source`. Extract
   (`invokeExtract`), staging directories, remembered paths, and
   saved-group names keep the method id.
3. **Mapping table.**

   | Method id | Vault / session source |
   |-----------|------------------------|
   | `imessage-ios`, `imessage-macos`, `imessage-jailbreak` | `imessage` |
   | `whatsapp-android`, `whatsapp-ios` | `whatsapp` |
   | anything else (`sms-backup-restore`, …) | unchanged (`form.source` already matches JSONL) |

   An unknown string is returned unchanged. SMS Backup & Restore and
   the other single-id sources already match their JSONL headers.
4. **Leave `vault-push` and the exporters alone.** Conversation
   requests keep sending JSONL `export.source`. After the session is
   `imessage` or `whatsapp`, that string matches.
5. **No new user-facing error copy.** After the mapping, the existing
   vault mismatch should not fire for iMessage or WhatsApp.

## Architecture

```text
form.source (method id, e.g. imessage-ios)
  ├─ vaultSourceForMethod → POST /v1/imports { source: imessage }
  ├─ invokeExtract({ source: imessage-ios, … })
  ├─ resolveImportStagingDir(…, imessage-ios)
  └─ saveImportSavedGroup({ source: imessage-ios, … })

vault-push detect_source()
  → JSONL export.source (imessage)
  → conversation request source=imessage
  → require_reusable_import: session imessage == request imessage
```

A pure helper, `vaultSourceForMethod(source: string): string`, lives
in a new file under `web/src/lib/` next to the iMessage and WhatsApp
import helpers. It reuses the existing constants those files already
export (`IMESSAGE_SOURCE_ID` is `imessage`, `WHATSAPP_SOURCE_ID` is
`whatsapp`). Tests sit next to the helper.

`useImportJob` calls the helper only for `POST /v1/imports`. Every
other use of `form.source` stays the method id.

The helper has one job: turn a method id into the vault source. It
does not pick extract fields, staging folders, or group names.
Callers that need the method id keep using `form.source`.

## Files

| Path | Change |
|------|--------|
| `web/src/lib/vaultSource.ts` | New pure helper |
| `web/src/lib/vaultSource.test.ts` | Mapping table tests |
| `web/src/screens/import/useImportJob.ts` | Session create uses the helper; extract, staging, and saved groups stay on `form.source` |

Leave exporter `EXPORT_SOURCE` values, `vault-push` `detect_source()`,
and the vault match check alone.

## Testing

Import is desktop-only. Cover this with Vitest, not Playwright against
Vite.

Helper:

- Each iMessage method id maps to `imessage`.
- Each WhatsApp method id maps to `whatsapp`.
- `sms-backup-restore` stays itself.
- An unknown string is returned unchanged.

Session create:

- If an existing import-job or extract-fields test posts
  `/v1/imports`, assert the body `source` is `imessage` when the form
  method is `imessage-ios`.
- If no such test exists, add a focused unit test around the
  session-create call rather than a full job-hook mount.

## Reproduce

1. Desktop Import → iMessage → Platform iPhone, point at a Finder
   backup, run Import against a vault that already has (or does not
   have) iMessage data.
2. Before this change, upload fails every JSONL with
   `source mismatch (session=imessage-ios, request=imessage)`.
3. After this change: session `source=imessage`, requests
   `source=imessage`, conversations import (append/dedupe as today).
