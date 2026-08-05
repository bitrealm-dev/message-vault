# Message Vault GUI

Living design notes for the cross-platform desktop GUI.

**Framework:** [Slint](https://slint.dev) 1.17, implemented in
[`crates/message-vault-io-gui`](../../crates/message-vault-io-gui).

## Goals

- One app for Linux, macOS, and Windows with retained-mode widgets.
- Drive exporters via their Rust libraries in-process (`ExporterConfig` + `run`);
  each crate also ships a thin standalone CLI with the same pipeline.
- Present tabs as **Extract Messages** | **Format** | **Vault** | **Contacts** | **Log**.
- Keep extraction predictable: Extract Messages always writes JSONL; Format
  converts a prior output folder to CSV, EML, MBOX, JSON, JSONL, or XML.
- Show only the controls that apply to the selected backup source; validate before run.
- Stream library log lines in the UI; support cancel (cooperative flags; WhatsApp’s
  external `wtsexporter` step is not killable mid-run).
- Dense desktop form layout (fixed label column, compact spacing) using Slint's
  platform `native` widget style.

## Widget style

Compiled with Slint's `native` style in `build.rs`:

| Platform | Style |
|----------|-------|
| Windows | Fluent |
| macOS | Cupertino |
| Linux | Qt when Qt 5.15+ is available; otherwise Fluent |

This crate stays pure Rust (no Qt SDK dependency). On Linux without Qt, Fluent is
the intentional fallback. Override at compile time with `SLINT_STYLE`
(for example `SLINT_STYLE=fluent cargo build -p message-vault-io-gui`).

Layout density lives in `ui/widgets.slint` (`FormMetrics`, horizontal `FormRow`
fields, tight row gaps). Ordinary fields do not stretch; only the Log tab's viewer
grows when the window is resized vertically.

## Architecture

- `ui/app-window.slint` — root window with `TabWidget`, error banner, status bar,
  and an About dialog that hosts Slint's `AboutSlint` widget (Royalty-free
  license attribution).
- `ui/pages/*.slint` — one page per tab; each exports a `*Adapter` global for
  properties and callbacks.
- `ui/widgets.slint` — dense form rows (`LabeledLineEdit`, `LabeledPathField`,
  `LabeledComboBox`, `AdvancedSection`, …) with a fixed label column.
- `src/main.rs` — constructs `AppWindow`, wires adapter callbacks, runs the
  event loop; persists `export.ini` on exit.
- `src/state.rs` — `AppState` holding `ExportIniState` + `Form` + job control
  (behind `Arc<Mutex<_>>` so the log bridge thread can wake the UI).
- `src/jobs.rs` — in-process `LibraryJob` dispatch for exporters, contacts,
  Format, and vault.
- `src/sync.rs` — push `AppState` into Slint adapters / pull adapter values back
  into `Form` before validate/save.
- `src/browse.rs` — `rfd` file/folder dialogs on a background thread, results
  applied via `Weak::upgrade_in_event_loop` (WSL opens Windows-native dialogs).
- `src/session_log.rs` — timestamped session log next to `export.ini`.
- `src/wsl.rs` — Windows interop when the Linux GUI runs under WSL (browser / help).

Jobs run via `message_vault_io_core::spawn_job` on a `std::thread` with a
`CancelFlag` + `mpsc::Sender<ProcessEvent>`. A bridge thread drains the receiver
and marshals each line onto the Slint UI thread (`upgrade_in_event_loop`),
appending to the Log tab's `VecModel` and the session log file.

Contacts, Extract Messages, Format, and Vault are linked libraries (no sibling
Rust exporter CLIs in the release archive). WhatsApp’s `wtsexporter` resolves
under `cli/` next to the GUI; media tools `ffmpeg` / `ffprobe` under `lib/`;
either may also be found via `MESSAGE_VAULT_IO_BIN` or `PATH`.

## Persistence

Reads/writes `export.ini` via `ExportIniState::load_or_default()` / `save()`.
Prefer an existing file in the working directory, else beside the GUI binary;
otherwise create `./export.ini` on first save. Template:
[`export.example.ini`](../../crates/message-vault-io-gui/export.example.ini).
Backup passwords are never written. The vault key is persisted in plain text under
`[vault]`.

Saved after exporter switch / Run / Clear, appearance changes, and again when the
window exits. Running Extract Messages sets the shared output format to JSONL; the
Format tab keeps its own output format under `[format]`. Older files with
`[message-reexport]` are still loaded; the next save writes `[format]` only.

## Appearance (four-seed theme)

Matches message-vault-rs Fastmail-style seeds. Rust derives surfaces (no CSS
`color-mix`); Slint reads them from `global Theme` in `ui/theme.slint`.

| Seed (preset) | Default Graphite Blue |
|---------------|------------------------|
| lightHeader | `#e6e9ee` |
| lightAccent | `#2b7fff` |
| darkHeader | `#222426` |
| darkAccent | `#5ea1ff` |

**Mode:** `light` | `dark` | `system` (default `dark`). System uses the
`dark-light` crate (OS color scheme; unspecified → dark).

**Presets:** Graphite Blue, Slate Sky, Forest, Dusk, Rose, Amber, Ocean, Mono.

**INI** (`[appearance]`):

```ini
[appearance]
mode = dark
preset = graphite-blue
```

Home screen has Theme / Colors combos. Custom chrome uses `Theme.*` tokens
(`bg`, `panel`, `elevated`, `border`, `text`, `muted`, `accent`, `danger`, …).
Native `std-widgets` (Button, LineEdit, …) still follow the Slint `native` style.

## Licensing

Slint is used under the **Royalty-free** license. The About dialog displays the
`AboutSlint` widget to satisfy the attribution requirement. No registration or
paid commercial license is required for this desktop app.

## Running it

```bash
cargo build --workspace
# optional: cp crates/message-vault-io-gui/export.example.ini export.ini
cargo run -p message-vault-io-gui
```

## Layout

1. Top tabs — **Extract Messages** | **Format** | **Vault** | **Contacts** | **Log**
2. **Extract Messages:** backup source picker + global options + per-source form
3. **Format:** format a prior Message Vault output (`message-reexporter`) —
   input dir, output dir, output format, attachments, obfuscate. Input format is
   auto-detected.
4. **Vault:** import a JSONL export folder into Message Vault — URL, vault key
   (Import API token), input dir, continue-on-error / force.
5. **Contacts:** contacts file, USA numbers checkbox, Check / Update / Cancel
6. Shared run log (Log tab)

### Contacts

Runs [`contacts::validate_contacts_file`](../../crates/message/contacts)
in-process via the `message-contacts` library.

- **Check**: dry run — no files written; the run log shows the same UNCERTAIN /
  DUPLICATE / summary content as a validate log.
- **Update**: write `<stem>-update.<ext>` (or `<stem>-update-N` when re-updating)
  (+ `.log`; CSV also `.vcf`). Only unambiguous phones are rewritten; uncertain
  values stay as-is.
- **Cancel**: cooperative cancel for the in-process job.

### Format — `message-reexporter`

Top tab (not an Extract Messages backup type). Converts a prior Message Vault
output folder to another packaging format (via the common message).

| Control | Type | Required | CLI |
|---------|------|:--------:|-----|
| Input directory | folder | yes | `--input` (auto-detect csv/eml/mbox/json/jsonl/xml) |
| Output format | enum | no | `--format` |
| Output directory | folder | yes | `--output` |
| Attachments | enum | no | `--media-mode` |
| Obfuscate / seed | checkbox + text | no | `--obfuscate` / `--obfuscate-seed` |

Persists under `[format]` in `export.ini` (loads legacy `[message-reexport]` if
present). Mixed or unrecognized input dirs fail with a clear error. See
[`crates/message/reexport/docs/MESSAGE_REEXPORTER.md`](../../crates/message/reexport/docs/MESSAGE_REEXPORTER.md).

### Guided Import Messages

Workflow screens (Home → Credentials → Import): Import Format choices are
**iMessage - iOS**, **iMessage - macOS**, and **Existing Archive (.jsonl)**.

- iOS / macOS: extract into a timestamped `staging-*` folder beside `export.ini`,
  then upload (`vault_push`). Message Attachments, filtering, and Obfuscate apply
  only to these formats.
- **Existing Archive (.jsonl):** upload-only. Pick an **Archive Directory**
  (folder of `.jsonl` files and optional sibling `attachments/`). Processing
  Options are Continue on error and Force reprocessing (no Obfuscate). Use this
  to resume a retained staging folder after a failed upload. Persists under
  `[vault]` as `input` and `import_format=existing-archive`.

### Guided Vault Export

Workflow screens (Home → Credentials → Export): pulls matching messages from
Message Vault via `vault-pull` into a timestamped folder.

| Control | Type | Required | Notes |
|---------|------|:--------:|-------|
| Exporter Type | enum | yes | Currently **iMessage** (covers iOS and macOS). |
| Output directory | folder | no | Parent folder; defaults to the process working directory (where the app was launched). Export writes `export-<type>-YYMMDD-HHMMSS` under it (same timestamp shape as import `staging-*`). |
| Search | text | no | Fastmail-style operators (`with:`, `has:attachment`, …). |
| Start / End date | date | no | Optional inclusive/exclusive bounds. |

Run **Query** first to preview counts (uses `GET /v1/export/messages/count`
when the vault supports it; otherwise pages export messages); **Export** then
creates the timestamped directory and downloads message-ir JSONL + attachments,
showing `Fetched k (of N)` when a Query count is available.

### Vault — `vault-push`

Top tab. Two-step workflow after Extract Messages: push message-ir v3 JSONL +
`attachments/` to a running Message Vault.

| Control | Type | Required | Notes |
|---------|------|:--------:|-------|
| Vault URL | text | yes | Full origin including port when needed, e.g. `http://127.0.0.1:8080` or `https://app.bitrealm.dev`. |
| Vault key | text | yes | Import API token from Vault Settings; saved to `export.ini` as `[vault] key` (plain text). On Linux/macOS the file is written mode `0600` (owner read/write only). |
| Input directory | folder | yes | JSONL export folder (prefills from last Extract Messages output when empty) |
| Continue on error | checkbox | no | Default on |
| Force reprocessing | checkbox | no | Ignore `.vault-import-state.jsonl` for this run (see below) |

#### Force reprocessing

When off (default), `vault-push` loads `.vault-import-state.jsonl` and skips
conversations, messages, and assets already recorded for this vault URL +
username. That makes re-runs resume-safe.

When on, the journal is ignored for the run (`JournalState::default()`), so every
conversation/message is submitted again and every unique attachment is offered
for upload again. Import mode stays **append**; this is not a vault database wipe.

Server-side behavior:

- Messages already stored are deduped (`messages_deduped` on the import response).
- Assets already stored by SHA-256 return `already_present` and are not re-PUT.

Use force when a prior pass left messages without attachments (vault reported
`assets_missing`, or an asset PUT failed) and the attachment file still exists
under the export/staging tree. Reprocessing retries the asset upload; a successful
digest on the vault fills the gap. It cannot repair attachments that are missing
from disk, or media omitted by skip-attachments / text-only import.

End-user write-up: [Import to Message Vault](../src/content/docs/work-with-exports/import-to-vault.mdx).

Persists under `[vault]` in `export.ini`. See
[`crates/vault-push/docs/MANPAGE.md`](../../crates/vault-push/docs/MANPAGE.md).

## Shared / global controls

| Control | Widget | CLI mapping | Notes |
|---------|--------|-------------|-------|
| Backup source | labeled selector | which binary | Supported first (iPhone backup, SMS Backup & Restore, WhatsApp), then experimental alphabetically with `(experimental)` suffix |
| Obfuscate | checkbox (global) | `--obfuscate` | When enabled, show seed field |
| Seed | text (exactly 8 hex) | `--obfuscate-seed` | Optional; blank = generate at run time |
| Start date | text (global) | `--start-date` | Optional `YYYY-MM-DD`, inclusive |
| End date | text (global) | `--end-date` | Optional `YYYY-MM-DD`, exclusive |
| Product title | hyperlink | — | Opens the upstream product/tool site |
| Input | path picker (file and/or folder) | `--input` / `-p` / etc. | Single path only |
| Output | folder picker | `--output` / `-o` | Required; choose explicitly (not derived from input) |
| Contacts | path picker | `--contacts` / `--vcf` / `-n` | Format depends on exporter; optional with warning |
| Run / Cancel | actions | in-process library `run` | Stream logs; cooperative cancel |

## Show / hide by backup source

| Section | GO SMS Pro | Backup & Restore | SMS Backup+ | OpenExtract | iMazing | WhatsApp | iPhone backup |
|---------|:----------:|:----------------:|:-----------:|:-----------:|:-------:|:--------:|:-------------:|
| Global anon + dates | yes | yes | yes | yes | yes | yes | yes |
| Input / Output | yes | yes | yes | yes | yes | output only | yes |
| DB path / Platform | — | — | — | — | — | platform (+ advanced) | primary |
| Your phone number(s) | required | required | required\* | — | — | — | — |
| Your email address(es) | — | — | required\* | — | — | — | — |
| Contacts VCF / vCard CSV | yes | yes | yes | yes | — | — (Contacts field) | — |
| Contacts vCard CSV | — | — | — | — | yes | — | — |
| Contacts Apple AddressBook | — | — | — | — | — | — | advanced |
| Timezone | — | — | — | — | yes | — | — |
| Name mapping | — | — | advanced | — | — | — | — |
| Verbose logging | — | — | always on | — | — | — | — |
| Attachments (copy/convert/compress/do not copy) | yes | yes | yes | yes†† | yes | yes | yes |
| Compress options (resolution/fps/…) | when Compress | when Compress | when Compress | — | when Compress | when Compress | when Compress |
| Advanced (attachment root, …) | — | — | name mapping | — | — | Android key / backup / wa / media / db / business | yes |

Convert/Compress need `ffmpeg`/`ffprobe` on PATH. **Do not copy** skips writing
attachment files (`--media-mode disabled` / iPhone `--copy-method disabled`).

\* Required unless filled from Plus `config/owner.toml` (source-relative today);
GUI collects fields explicitly.

†† OpenExtract has no media in its source CSVs yet, so attachment modes are a
no-op for files; the control is still shown.

Extract Messages always packages as **JSONL**. Schema v3 applies to every
exporter. See [What’s inside an export](../src/content/docs/understand-output/export-structure.md)
and the [message-ir architecture](architecture/message-ir.md). Attachment modes
and obfuscate apply to every output format via `FormatSink` (including Format-tab
re-exports).

## Per-exporter options

### GO SMS Pro — `go-sms-pro-exporter`

Product: [GO SMS Pro](https://play.google.com/store/apps/details?id=com.jb.gosms)

In-process via `go_sms_pro_exporter::run`. Cancel is cooperative (between XML/PDU files).

| Control | Type | Required | Library / CLI equivalent |
|---------|------|:--------:|-----|
| Input | folder (backup root with XML + PDU) | yes | `--input` |
| Output | folder | yes | `--output` |
| Your phone numbers | multi-value text | yes | `--owner-phone` (repeat) |
| Contacts CSV | file | no† | `--contacts` |
| Contacts VCF | file | no† | `--vcf` |
| Attachments | enum | no | `--media-mode` (`clone` / `convert` / `compress` / `disabled`); all formats via FormatSink |
| Max resolution / fps / min size / skip efficient | when Compress | no | `--media-max-resolution`, `--media-max-fps`, `--media-min-size`, `--media-skip-efficient` |

† At most one of `--contacts` / `--vcf`. Global Obfuscate and Start/End date apply
for every format via FormatSink. Convert → `.jpg`/`.mp4`/`.mp3`; Compress
re-encodes (needs ffmpeg).

### SMS Backup & Restore — `sms-backup-restore-exporter`

Product: [SMS Backup & Restore](https://www.synctech.com.au/sms-backup-restore/)

| Control | Type | Required | CLI |
|---------|------|:--------:|-----|
| Input | XML file or folder of XML | yes | `--input` |
| Output | folder | yes | `--output` |
| Your phone numbers | multi-value text | yes | `--owner-phone` |
| Contacts CSV / VCF | file | no† | `--contacts` / `--vcf` |
| Attachments | enum | no | `--media-mode` (+ compress flags; same as GO SMS Pro); all formats |

Encrypted ZIP backups must be unlocked/extracted before selecting input. The
exporter builds the [shared conversation structure](../src/content/docs/understand-output/export-structure.md),
then writes JSONL (or the Format-tab target). Media modes and obfuscate apply
through FormatSink for every format.

### SMS Backup+ — `sms-backup-plus-exporter convert`

Product: [SMS Backup+](https://github.com/jberkel/sms-backup-plus)

GUI always runs the `convert` subcommand and always passes `--verbose`.

| Control | Type | Required | CLI |
|---------|------|:--------:|-----|
| Input | one EML file or folder | yes | `--input` |
| Output | folder | yes | `--output` |
| Your phone numbers | multi-value text | yes\* | `--owner-phone` |
| Your email addresses | multi-value text | yes\* | `--owner-email` |
| Contacts CSV / VCF | file | no† | `--contacts` / `--vcf` |
| Name mapping CSV | file | no | `--name-mapping` (`Phone,Incorrect Name`) |
| Verbose | — | always | `--verbose` |
| Attachments | enum | no | `--media-mode` (+ compress flags; same as GO SMS Pro); all formats |

\* Or from crate-relative `config/owner.toml` — GUI does not rely on that; collect
explicitly. Media modes and obfuscate apply through FormatSink for every format.

### OpenExtract — `openextract-exporter`

Product: [OpenExtract](https://www.openextract.app/)

| Control | Type | Required | CLI |
|---------|------|:--------:|-----|
| Input | CSV file or folder | yes | `--input` |
| Output | folder | yes | `--output` |
| Contacts VCF / vCard CSV | file | no† | `--vcf` / `--contacts` |

Media modes and obfuscate apply through FormatSink for every format. Mail is
text-only (no media in source).

### iMazing — `imazing-exporter`

Product: [iMazing](https://imazing.com/)

| Control | Type | Required | CLI |
|---------|------|:--------:|-----|
| Input | Messages/WhatsApp CSV, chat folder, `Messages/`, `WhatsApp/`, or device export root | yes | `--input` |
| Output | folder | yes | `--output` |
| Contacts | vCard CSV only | no | `--contacts` |
| Timezone | IANA text | no | `--timezone` (default: host local) |

Media modes and obfuscate apply through FormatSink for every format. WhatsApp
chats use the `__whatsapp` stem suffix. See
[`crates/exporters/imazing-exporter/docs/DESIGN.md`](../../crates/exporters/imazing-exporter/docs/DESIGN.md).

### WhatsApp — `whatsapp-exporter`

Product: [WhatsApp Chat Exporter](https://github.com/KnugiHK/WhatsApp-Chat-Exporter)
(`wtsexporter`)

Requires `wtsexporter` under `cli/` next to the GUI, on `PATH`, in `MESSAGE_VAULT_IO_BIN`, or
via `WTSEXPORTER` (pip install or release-bundled binary).

No Input directory and no Contacts file row in the GUI. `wtsexporter` runs in a
temporary directory under the Output folder (so extract junk is not written into
the GUI launch directory).

**iOS field order:** Backup type → Platform → Backup path → Contacts → Output →
Attachments → Advanced (WhatsApp Business).

**Android field order:** Backup type → Platform → Backup path → Contacts → Output →
Attachments → Decryption key → Advanced (media folder, Message Database, WhatsApp
Business).

| Control | Type | Required | CLI |
|---------|------|:--------:|-----|
| Platform | Android / iOS | yes | `--platform` |
| Backup path | folder (iOS) or crypt file (Android) | iOS yes / Android no | `--backup` |
| Contacts | file (hint: Optional wa.db / Optional ContactsV2.sqlite) | no | `--wa` |
| Decryption key | text (Android only; not saved) | no | `--key` |
| Output | folder | yes | `--output` |
| Attachments | enum | no | `--media-mode`; all formats |
| Media folder | folder (advanced, Android only) | no | `--media` |
| Message Database | file (advanced, Android only; hint: Optional msgstore.db override) | no | `--db` |
| WhatsApp Business | checkbox (advanced) | no | `--business` |

Media modes and obfuscate apply through FormatSink for every format. Output stems
use the `__whatsapp` suffix. Optional CLI `--input` (defaults to cwd for resolving
`msgstore.db` / media folders) is not sent by the GUI; extraction always uses a
temp dir under Output.

### iPhone backup — `imessage-ir-exporter`

Form link label: **imessage-ir-exporter** →
[imessage-ir-exporter](https://github.com/bitrealm-dev/message-vault-io/tree/main/crates/exporters/imessage-ir-exporter).
Dropdown stays **iPhone backup**.

GUI defaults: JSONL for Extract Messages, `--copy-method clone` (or `disabled`),
always `--use-caller-id`. Honors dates, conversation filter, contacts, attachment
embed, and caller-id on From. Convert/Compress/obfuscate apply through FormatSink
for every format (same as other exporters).

| Control | Type | Required | CLI |
|---------|------|:--------:|-----|
| Database / iOS backup path | file/folder | no | `--input` |
| Backup password | password | no | `--backup-password` |
| Platform | macOS / iOS / auto | no | `--platform` |
| Output / export path | folder | yes | `--output` |
| Attachments | enum | no | `--copy-method` / media mode via FormatSink |
| Max resolution / fps / min size / skip efficient | when Compress | no | compress options on FormatSink |
| Attachment root | folder | no | `--attachment-root` (advanced) |
| Conversation filter | text | no | `--conversation` (advanced) |
| Contacts (AddressBook DB) | file | no | `--contacts` (advanced) |

Media modes and obfuscate apply through FormatSink for every format. Caller ID is
always on.

Advanced panel uses a chevron toggle (**Show advanced options**), not a checkbox.

## Validation rules

1. **Contacts mutual exclusion:** for Android/OpenExtract, allow at most one of
   `--contacts` vs `--vcf`.
2. **Contacts format:** label and file filters must match the exporter (VCF /
   vCard CSV vs Apple AddressBook).
3. **Phone numbers:** required for GO SMS Pro and SMS Backup & Restore before Run;
   Plus also requires email address(es).
4. **Path existence:** input must exist; output folder may be created on run.
5. **Obfuscate seed:** if provided, must be exactly 8 hex characters; empty means
   generate.
6. **Timezone (iMazing):** if set, must be a valid IANA name (or defer to converter
   error).
7. **iPhone backup:** output directory is required; always passes `--use-caller-id`;
   obfuscate / convert / compress apply via FormatSink for every format.
8. **SMS Backup+:** exactly one input path; `SourceConfig::SmsBackupPlus` sets
   `verbose` / `include_summary`.
9. **Date range:** optional start/end `YYYY-MM-DD`; end is exclusive; blank means
   unbounded (`DateRange` on `ExporterConfig`).
10. **Media convert/compress:** require `ffmpeg` and `ffprobe` on PATH; Compress
    options validated (fps number, min size like `20M`).
11. **Warn (non-blocking):** missing contacts → same warning language as CLIs
    (“phones will not be resolved to names”).

## Form flow

```text
Tabs: Extract Messages | Format | Vault | Contacts | Log
  Extract Messages → pick backup source → Obfuscate/dates → per-source form
         → Form::to_config → ExporterConfig (JSONL) → library run / Cancel → log
  Format → input dir → output format → output dir → media/obfuscate
           → ir::reexport::run → log
  Vault → URL / user / key / input → vault_push → log
  Contacts → contacts file, USA checkbox → Check / Update / Cancel → log
```

End-user walkthrough: [First export with the app](../src/content/docs/get-started/first-export.mdx).

## Known gaps

| Gap | Detail | Suggested fix |
|-----|--------|---------------|
| Plus `owner.toml` | Resolved via `CARGO_MANIFEST_DIR`, not user cwd | GUI collects phone/email/input explicitly |
| iMazing attachments | Filename-only; no media copy | Document in UI; optional future media join |
| Encrypted backup password | Still held in memory on `AppleConfig` during run | Prefer env/stdin if CLI grows support; warn in UI |
| ffmpeg / ffprobe stderr | Media tools discard detailed stderr; failures become short media-report lines | Optional capture into `LogSink` |
| WhatsApp `wtsexporter` | Nested CLI is buffered until extract exits (then appended to `messages`) | Stream subprocess lines like Contacts validate |

Interactive GUI smoke tests still need a display; CI verifies compile/link only.

## Non-goals

- Packaging / installers.

## Next steps

- Add application icons and native installers/packages.
- Add platform CI builds and GUI smoke tests.
