# Four-step Import progress — 2026-08-27

Make the Import progress list match the work the desktop job actually
does. This spec records decisions from the 2026-08-27 design
conversation. It is not an implementation plan.

## Goal

While a desktop Import runs, the page shows four sequential steps that
match the extract-then-upload job:

1. Parse the backup (messages only).
2. Copy, convert, or skip attachments, with a live file count and size.
3. Prepare messages (write each conversation `.jsonl` once).
4. Upload the staging folder into the vault.

The long attachment pass is visible while files are written. Preparing
messages is not labeled as attachment work. Settings import history
shows the same four steps and four times.

## Current product

The page always shows three steps:

1. Parse backup
2. Copy attachments (or Convert / Skip, from the form)
3. Upload to vault

Extract does something else. Each exporter reads messages and, in the
same loop, copies or converts attachment files into `staging/attachments/`.
That disk work is most of the wait. After the loop, conversation
documents are buffered and written as `.jsonl`. Tauri maps the write
banner (`Writing N conversation file(s)…` and `wrote N/M`) to the
middle step, whose title is still Copy attachments. That line finishes
in about a second. Then `vault-push` uploads.

Progress events use three names: `parse`, `convert`, `upload`.
`convert` means “conversation files are being written,” not “attachments
are being copied.” The job hook comment already says convert-stage
counts are conversation writes.

Each import stores three durations: `parse_ms`, `convert_ms`,
`upload_ms`. `convert_ms` is that short write, not the attachment pass.
Settings history hard-codes the middle label as Convert attachments.

Conversation `.jsonl` files store each attachment’s staged path, SHA-256
hash, and size. The filename under `attachments/` is built from that
hash (for example `20260827_153012-a1b2c3d4e5f67890.jpg`). Those fields
are only known after the file has been read, and after convert/compress
if that mode is on.

## Non-goals

- Do not write `.jsonl` files before attachments. That would leave wrong
  paths and hashes, then force a second write of every conversation file.
- Do not keep one interleaved pass that copies files while messages are
  parsed. The page is strictly sequential.
- Do not keep `convert` as a progress step or `convert_ms` as a column.
  There is no compatibility with old import history.
- Do not change how `vault-push` uploads or how the vault stores
  messages, except the four timing columns on `vault_imports`.
- Do not add Playwright coverage. Import is desktop-only. UI proof is
  Vitest.
- Do not change Import Errors grouping rules (same `kind` + `step` +
  `reason`). Only the allowed `step` values change.

## Decisions

1. **Four steps, every mode.** Parse backup → Copy/Convert/Skip
   attachments → Preparing messages → Upload to vault. Skip still shows
   the attachments line.
2. **Two-pass extract, then write, then upload.** Parse records pending
   attachment jobs and does not write media. A shared runner then
   copies, converts, compresses, or skips those jobs. Then conversation
   files are written once. Then `vault-push` runs.
3. **Attachments before Preparing messages.** The `.jsonl` file needs
   the final path, hash, and size. Convert/compress hashes the
   transcoded file, which does not exist until the attachment pass
   finishes.
4. **All desktop sources.** iMessage (iPhone, Mac, jailbreak), WhatsApp
   (Android, iPhone), SMS Backup & Restore, SMS Backup+, GO SMS Pro,
   iMazing, and OpenExtract all use the shared runner. The page has one
   progress shape. Command-line tools that call the same exporter crates
   use the same parse-then-runner-then-write order. Only the desktop
   Import page shows the four-step list.
5. **Attachment progress is file count plus size, and says
   attachments.** Example: `Copied 120/840 attachments (1.2 GB / 4.0 GB)`.
   Convert/Compress uses `Converted`. Skip uses `Skipped`. A large video
   and a tiny sticker each count as one file.
6. **Unknown sizes.** If parse does not know a file’s size, that file
   still counts as one. The byte total starts as the sum of known sizes.
   When an unknown file is measured, that size is added to the total.
   The file count total does not change after parse.
7. **Four stored times, schema recreation.** Columns are `parse_ms`,
   `attachments_ms`, `prepare_ms`, `upload_ms`. `convert_ms` is removed.
   Vault `SCHEMA_VERSION` goes from 1 to 2. An old database is wiped and
   created empty. Old import history is not kept or mapped.
8. **Shared attachment runner.** The copy/convert/skip loop lives in the
   shared export library, not in each exporter. Each exporter only
   appends pending jobs during parse.
9. **Preparing messages does not run ffmpeg.** Media convert/compress
   moves out of `FormatSink::finish` and into the runner. Obfuscation,
   if on, still runs at write time (it replaces bytes with placeholders).
10. **Progress and issue step names.** `parse`, `attachments`,
    `prepare`, `upload`. The old name `convert` is not produced.

## Architecture

```text
Vendor backup
    │
    ▼
1. Parse          Exporter reads messages.
                  Builds conversations in memory.
                  Records a pending attachment job per media item
                  (source path or backup file id, owning message,
                  original name, MIME type, size if known).
                  Does not copy or convert files.
    │
    ▼
2. Attachments    Shared runner.
                  Copy, convert, compress, or skip each job.
                  Fills staged path, SHA-256, and size.
                  Progress: file count + bytes, word "attachments".
    │
    ▼
3. Prepare        FormatSink writes each .jsonl once.
                  No second media pass.
                  Progress: conversation counts.
    │
    ▼
4. Upload         vault-push sends the staging folder.
```

Progress events use four step names: `parse`, `attachments`, `prepare`,
`upload`. An attachments event also carries byte counts.

Import-error rows use the same four step names. Grouping still keys on
`kind` + `step` + `reason`.

## Components

**Pending attachment job.** One record per media item found during
parse: source location, which message owns it, original name, MIME type
if known, and size if the backup already has it. No dest path or
SHA-256 yet.

**Shared attachment runner.** Input is the job list plus the form mode
(copy / convert / compress / skip). It writes files under
`staging/attachments/`, updates the in-memory conversations, and emits
`attachments` progress with `done/total` files and
`bytes_done/bytes_total`. Skip mode records `missing_reason` and does
not copy.

**Conversation writer.** Today’s `FormatSink` still buffers
conversations and writes them. Media convert/compress is no longer
applied in `finish`. Obfuscation, if enabled, stays at write time.

**Progress events.** Tauri stops treating `Writing N conversation
file(s)…` as `convert`. Message-count log lines map to `parse`.
Attachment callbacks map to `attachments`. Conversation-write lines map
to `prepare`. Upload events stay `upload`.

**Import job hook and summary.** The progress page and Settings history
always show four lines. Attachment step title still follows the form
(`Copy attachments`, `Convert attachments`, `Skip attachments`). The
third title is always **Preparing messages**. Durations posted on
complete are `parse_ms`, `attachments_ms`, `prepare_ms`, `upload_ms`.

**Vault schema.** `vault_imports` drops `convert_ms` and adds
`attachments_ms` and `prepare_ms`. The same change applies to SQLite
(`schema/sql/accounts.sql`) and Postgres (`schema/sql/pg_accounts.sql`).
`SCHEMA_VERSION` becomes 2.

**Exporters.** Desktop extract sources stop calling `persist_attachment`
or `copy_if_missing` inside the message loop. They only append pending
jobs. The runner is the only code that copies or converts.

## Data flow

A run still creates a vault import session, picks a staging folder, then
calls extract.

**Parse.** The exporter walks the backup once. Each message becomes the
shared conversation structure and stays in memory. Each attachment
becomes a pending job. Progress is message counts, for example
`Parsing 500/12345`. `attachments/` is empty (or missing). No `.jsonl`
file exists yet.

**Attachments.** The runner walks the pending jobs. Copy reads the
source and writes the content-addressed file. Convert or compress
transcodes first, then writes that result. Skip records that the file
was skipped and does not write bytes.

After each job, that message’s attachment gets the staged path, SHA-256,
and size. Progress lines:

- Copy: `Copied 120/840 attachments (1.2 GB / 4.0 GB)`
- Convert/Compress: `Converted 120/840 attachments (1.2 GB / 4.0 GB)`
- Skip with media found: `Skipped 840/840 attachments (0 B / 0 B)`
- Skip with no media: `Skipped 0/0 attachments (0 B / 0 B)`

**Preparing messages.** `FormatSink` writes each conversation once.
Paths and hashes are already final. Progress is conversation counts, for
example `Preparing 40/200`.

**Upload.** `vault-push` sends the staging folder. When the job
finishes, `/v1/imports/{id}/complete` stores the four times.

**Settings history.** The same four steps and four times are shown
later. There is no `convert` step and no `convert_ms` field.

## Error handling

A failure is still one issue row: `kind` (`error` or `skip`), `step`,
`item`, and `reason`. `step` is `parse`, `attachments`, `prepare`, or
`upload`.

**Parse.** A single bad message is skipped and recorded (`step: parse`,
`item` is a row id or guid). The rest of the backup continues. A broken
database or missing backup file stops extract. The attachment runner
does not start. Preparing messages and upload do not run. The active
step is marked error.

**Attachments.** A missing source file is a skip on `step: attachments`.
`item` is the original filename, or the source path if the name is
unknown. The conversation keeps that attachment with `missing_reason`
set. A
convert/compress failure (ffmpeg missing or a file that will not
transcode) is an error on that file. Other jobs still run. A disk-full
or permission error on the staging folder stops the runner. Preparing
messages does not start, so no `.jsonl` is written with half-finished
hashes.

**Preparing messages.** A conversation that cannot be written is an
error on `step: prepare`. `item` is the chat id or `.jsonl` name. Other
conversations still write. If the writer cannot create the staging
folder, extract stops and upload does not run.

**Upload.** Vault-push failures stay `step: upload`. Staging files
already written are left on disk. The Staging directory link still
opens that folder.

**Cancel.** The existing cancel flag is checked in parse, in the
attachment runner (between jobs), in the conversation writer (between
files), and during upload. The current step is marked error. Later
steps stay pending. Staging is not deleted.

## Files

| Path | Change |
|------|--------|
| `crates/core/message-vault-io-core` (pending job type + runner, next to existing attachment helpers) | New. Copy/convert/skip and attachment progress. |
| `crates/libs/ir-format` (`FormatSink::finish`) | Stop running convert/compress here. Write only (plus obfuscation). |
| Desktop exporters (iMessage, WhatsApp, SMS Backup & Restore, SMS Backup+, GO SMS Pro, iMazing, OpenExtract) | Parse records jobs; do not persist files in the message loop. |
| `src-tauri/src/commands/progress.rs` | Map log lines to `parse` / `attachments` / `prepare`. |
| `web/src/lib/types.ts` | Progress and issue `step` unions; byte fields on progress. |
| `web/src/screens/import/useImportJob.ts` | Four steps, four durations, new progress verbs. |
| `web/src/lib/attachmentStepCopy.ts` | Keep step titles; detail lines must say attachments and include size. |
| `web/src/components/import/ImportSummaryPanel.tsx` | Four history steps; drop Convert attachments / `convert_ms`. |
| `schema/sql/accounts.sql`, `schema/sql/pg_accounts.sql` | Replace `convert_ms` with `attachments_ms` and `prepare_ms`. |
| `crates/vault/server/src/db/schema.rs` | `SCHEMA_VERSION` 2. |
| Vault import complete / history types | New field names only. |
| `tests/fixtures/schema/` | Match the new columns. |

## Testing

Import is desktop-only. Do not add Playwright against Vite.

**Shared attachment runner**

- Copy mode writes the staged file and fills path, SHA-256, and size.
- Convert/compress remaps the path and hash to the transcoded file
  before any `.jsonl` exists.
- Skip writes no bytes and sets `missing_reason`.
- A missing source is a skip; the next job still runs.
- Progress callbacks see file `done/total` and byte `done/total`.
- Cancel between jobs stops the runner and does not start Preparing
  messages.

**Exporters**

- After parse, `attachments/` is empty (or missing) and there are no
  `.jsonl` files.
- After the runner, files exist and conversation objects have hashes.
- After Preparing messages, each `.jsonl` matches those hashes on the
  first write.
- A fixture with a few messages and one photo is enough per source.
  Do not add personal backups.

**Progress parsing (Tauri)**

- Message lines (`…500/12345`) map to `parse`.
- Attachment lines map to `attachments` and include byte counts.
- Conversation-write lines map to `prepare`.
- The old `convert` step name is not produced.

**Web (Vitest)**

- Four steps always: Parse backup, Copy/Convert/Skip attachments,
  Preparing messages, Upload to vault.
- Active attachment detail contains the word `attachments` and both a
  file count and a size.
- Skip still shows the attachments step.
- Summary and Settings history read `parse_ms`, `attachments_ms`,
  `prepare_ms`, `upload_ms`. Nothing reads `convert_ms`.

**Vault**

- Completing an import stores the four times.
- `SCHEMA_VERSION` is 2. A version-1 database is rebuilt empty.
- Server tests and fixtures use the new column names only.
