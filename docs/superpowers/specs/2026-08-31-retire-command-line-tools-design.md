# Retire the command-line tools — 2026-08-31

Delete every command-line binary in the workspace except the vault server,
after closing the one product gap that today only the command line fills.
This spec records decisions from the 2026-08-31 design conversation. It is
not an implementation plan.

The shape of the decision is recorded separately as
[ADR 0001](../../adr/0001-no-command-line-except-the-vault-server.md).

## Goal

The desktop app becomes the only surface a person uses to get messages out
of a backup and into files. The vault server keeps its command line because
a server needs one. Nothing else in the workspace ships a binary, and the
documentation site stops describing commands that no release has ever
contained.

## Why this came up

The exporters were command-line tools first, and the graphical interface
was built around them. That relationship no longer exists in the code: the
desktop app links each exporter as a library and calls `run` in process.
What remained was the shell, the generated documentation, and the
maintenance those two cost.

## Current product

Every exporter is already a library crate with a thin binary on top. The
binary is `main.rs` plus `cli.rs` behind a `cli` feature — between 58 and
160 lines per crate, 641 lines across the seven, plus `CommonCli`
and `run_cli` in `crates/core/message-vault-io-core/src/cli.rs`.

`src-tauri/src/commands/extract.rs` imports `run` from each exporter crate
and calls it on a background thread. Every exporter dependency in
`src-tauri/Cargo.toml` is declared `default-features = false`, so the
desktop app never compiles clap.

No release contains any of these binaries. The `release` job in
`.github/workflows/ci.yml` builds the Tauri installers; the `docker` job
builds the server image. Neither builds an exporter.

The documentation site nonetheless presents eleven commands as a product
surface. `dump-cli-docs` generates a page for each from its clap
definition, and the User Guide page `extract-to-files.md` tells a reader to
"build the workspace so the exporter commands are on your PATH."

Three gaps make the commands more than a redundant shell today:

- `src-tauri/src/commands/extract.rs` hardcodes `OutputFormat::Jsonl`, so
  the desktop Import path can only produce JSONL.
- `src-tauri/src/commands/format.rs` implemented format conversion for all
  six formats and was reachable from nothing. `web/src/lib/tauri.ts` had no
  `invokeFormat`. See the note below: that command was deleted on main after
  this conversation and has to be restored.
- `vault-pull` has no format option at all, so the Export screen writes
  JSONL and nothing else.

So the command line is currently the only way any person gets CSV, EML,
MBOX, Android XML, or indented JSON out of Message Vault.

`web/src/screens/ExportScreen.tsx` is 78 lines and exposes one field, the
save folder. The `pull` Tauri command accepts six arguments; the screen
hardcodes `query` to the empty string and `skip_attachments` to false.
`vault-pull`'s `--after`, `--before`, `--source`, and `--page-limit` stop
at the Tauri boundary and are not forwarded at all.

`docs/src/content/docs/vault/user/how-to/export-from-the-vault.md`
describes three features that do not exist: a scope picker offering entire
vault, current view, and selected; a format choice; and a browser download.
There is no download path in `web/src/lib/api.ts`.

### What main did after this conversation

Six pull requests landed while this spec was being written. Two of them
touch it.

**The `format` command was deleted**, by
[#268](https://github.com/bitrealm-io/message-vault/pull/268), which reads:
"delete the format command, which nothing in web/ invokes. `SourceConfig::Format`
stays for the message-reexporter CLI." That is the opposite of the direction
decided here — it removed the desktop entry point and kept the command line as
the reason. The decision below is unchanged; the first change now restores
`src-tauri/src/commands/format.rs`, its `pub mod format;` line, and its
`invoke_handler` registration rather than only calling into it. The command was
80 lines and is recoverable whole from the commit before that deletion.
`SourceConfig::Format` and `FormatConfig` were left in place, and
`src-tauri/Cargo.toml` still depends on `message-reexport`, so restoring is a
revert rather than a rewrite.

**Part of the second change is already done.** The same pull request dropped
io-core's `EXPORTERS` registry and its accessors, which included
`Exporter::binary()`. Nothing there needs deleting again.

Two other facts moved without changing anything decided here.
[#277](https://github.com/bitrealm-io/message-vault/pull/277) moved
`HttpSession`, `auth_check`, `bearer_header`, and `trim_base_url` into
`crates/libs/vault-http/src/session.rs` — the library half of issue #264 — and
did not build the shared clap connection struct that the same issue proposed,
which this design would have made dead. `VaultPullConfig` also lost
`expected_messages`, `force`, and `journal_path`, the dead options that issue
flagged.

## Decisions

**1. Headless use is the eventual audience for a command line, and it is
deferred.** Nobody is served by the current commands. When headless matters
again, the answer is one `message-vault` command with subcommands, not
seven exporter binaries. Until then there is no command line for people
outside the server.

**2. Multi-format output stays a product capability, and it belongs in the
desktop app.** Deleting the exporters without it would be a quiet
capability regression, not a simplification.

**3. The Export screen is where the format picker goes.** Export means vault
to files, and it is the surface that gains a format choice first.

Converting one folder of files into another folder stays a capability the
product wants. `message-reexport` keeps all of its functionality; what it
loses is only its command-line entry point. The Convert screen that will
expose it to a person has not been built yet, and is deferred rather than
declined (see the follow-on below).

Until that screen exists, the route from a backup to CSV is to import into
the vault and then export with a format. That is a gap in what a person can
reach, not a decision that folder-to-folder conversion should not exist.

**4. All ten documented binaries are deleted.** The seven exporters,
`message-reexporter`, `vault-push`, and `vault-pull`. `vault-push` and
`vault-pull` stay as libraries — the desktop app's Import and Export run
through them — but lose their `bin/` and `cli.rs`. They were kept in an
earlier draft of this decision on the grounds that they already worked;
that grounds did not survive checking, because no test, script, or workflow
has ever run either one as a command.

**5. The work lands as two changes, the gap first.** Between deleting the
exporters and shipping the format picker there would otherwise be a period
where the product can write nothing but JSONL. The two halves also fail
differently: the picker needs verifying in the running app, the deletion is
verified by the compiler.

**6. A non-JSONL export stages under the configured staging directory.**
`vault-pull` writes JSONL into a folder under the staging parent,
`message-reexport` converts from there into the folder the person chose,
and the staging folder is deleted. Converting in place is not available:
`crates/libs/reexport/src/lib.rs` canonicalizes both paths and refuses to
write into its own input. Choosing JSONL skips staging entirely and pulls
straight into the chosen folder.

**7. The setting is renamed to the staging directory.** It is no longer
specific to Import once Export uses it. The rename covers the identifiers
in `web/src/lib/system-settings.ts`, the field label and helper text in
`web/src/screens/settings/SystemSection.tsx`, the four error strings in
`src-tauri/src/commands/paths.rs`, `web/src/lib/openPath.ts`, and the
localStorage key.

**8. The first change builds the format picker only.** The scope picker and
the browser download that `export-from-the-vault.md` already describes
become separate issues. Correcting that page is not optional in any case:
it is wrong in three ways today, and fixing one while leaving two would
still mislead a reader.

**9. `extract-to-files.md` is deleted with a redirect; `convert-formats.md`
keeps its format table and loses the command.** Every sentence in
`extract-to-files.md` is about running a command that will not exist. The
format table is different — it describes the output files rather than the
tool, and a person choosing EML in the new picker needs to know it embeds
media and writes one file per message. That table moves alongside
`reference/export-structure.md` and `reference/csv-columns.md`.

`convert-formats.md` should say that Convert has no screen yet rather than
implying the operation was withdrawn.

**10. `crates/cli/` is deleted.** `vault-push` and `vault-pull` move to
`crates/libs/`, which is what they are. `dump-cli-docs` is folded into
`message-vault-server` as a subcommand beside `dump-openapi`, which already
does the same job for the OpenAPI document. After the deletion the crate
would generate exactly one page, from the server's own clap definition, and
`message-vault-server` would be its only dependency.

**11. `imazing-obfuscate` is deleted, along with the library code behind
it.** It rewrites an iMazing vendor CSV on the input side. Obfuscation is
now an output-side transform applied uniformly by
`crates/libs/ir-format/src/pipeline.rs` for every source, so an iMazing-only
pre-pass has nothing left to do. `obfuscate_imazing` and its four private
helpers in `crates/libs/obfuscate/src/lib.rs` have one caller, that binary.
The iMazing test fixture is hand-authored, so fixture generation does not
justify keeping it either.

**12. `contacts-validate` is deleted, along with `validate_contacts_file`,
`ValidateMode`, and `ValidateReport`.** It validates the contacts file
format, and that format is going to change (see the follow-on below).
`ContactsFormat` and `detect_contacts_format` stay: they also live in
`validate.rs` and `book.rs` uses them to choose a loader.

**13. `demo-seed` stays.** It generates the sample dataset, and the
server's `reset-demo` subcommand points at its directory.

## What ships

### First change: close the gap

- Restore `src-tauri/src/commands/format.rs` from the commit before
  [#268](https://github.com/bitrealm-io/message-vault/pull/268), along with
  `pub mod format;` in `commands/mod.rs` and the `commands::format::format`
  entry in `main.rs`'s `invoke_handler`.
- Add `invokeFormat` to `web/src/lib/tauri.ts`.
- Add a format control to `web/src/screens/ExportScreen.tsx` offering the
  six formats `format.rs` accepts.
- Switch that screen from `useTauriJob`'s `start` to its `run`, and call
  `invokePull` then `invokeFormat`. The Import screen already sequences two
  jobs this way, so the pattern exists.
- Stage under the staging parent for any format other than JSONL, and
  delete the staging folder afterwards with `delete_staging`.
- Make `vault-pull` write the export sentinel into its output folder.
  `delete_staging` refuses any folder without `.message-vault-export`
  (`resolve_staging_child` in `src-tauri/src/commands/staging.rs`, called with
  `require_sentinel` true), and vault-pull writes an export folder today
  without marking it as one. Loosening that guard is not an option: it is the
  only check between a path bug and a `remove_dir_all` somewhere else on disk,
  and `staging.rs` has a test pinning it. Writing the sentinel also makes
  vault-pull consistent with every other producer of an export folder.
- Rename the staging setting throughout.
- Correct `export-from-the-vault.md`: remove the scope list, remove the
  browser download sentence, and change the format list from three formats
  to six.

Both jobs emit the same `extract:*` events, so the conversion lines
continue the existing log. No step interface is added.

### Second change: delete

- Remove `[[bin]]`, `main.rs`, `cli.rs`, and the `cli` feature from the
  seven exporter crates, from `crates/libs/reexport`, and from
  `crates/cli/vault-push` and `crates/cli/vault-pull`.
- Remove `CommonCli` and `run_cli`. `Exporter::binary()` went with the
  `EXPORTERS` registry in #268 and needs nothing further.
- Remove the `imazing-obfuscate` and `contacts-validate` binaries and the
  library code that only they call.
- Move `vault-push` and `vault-pull` to `crates/libs/`; delete
  `crates/cli/`.
- Add a `dump-cli-docs` subcommand to `message-vault-server` and delete the
  `dump-cli-docs` crate. The documentation command becomes
  `cargo run -p message-vault-server -- dump-cli-docs --output ...`.
- Delete the eleven files under
  `docs/src/content/docs/vault/developer/reference/cli/` and the sidebar
  group in `docs/astro.config.mjs`. `server-cli.md` stays where it is.
- Delete `extract-to-files.md` and add a redirect to
  `export-from-the-vault.md`. Rewrite `convert-formats.md` as format
  reference without the command.
- Edit the twenty remaining source pages that name a deleted command, and
  the ten crate READMEs.

This change touches the same crates as the exporter migration currently in
flight, though different files. It is easier to land after that migration
settles.

## Follow-on work

**Convert screen.** `message-reexport` keeps every capability it has and
loses only its command-line entry point, so after this work the library has
no user-facing route at all. Building the screen that exposes
folder-to-folder conversion is deferred, not declined. Until it lands, the
only way to reach a format other than JSONL is Export from the vault.

**Contacts file format.** The largest of these, and its own design
conversation. The contacts file is phone-only by construction: `load_vcf`
in `crates/libs/contacts/src/book.rs` reads `FN`, `N`, and `TEL`, discards
the parsed `EMAIL` and `CATEGORIES`, skips any card with no phone number,
and hardcodes `HandleType::Phone`. The vCard CSV path matches seven column
names, three of which are fax columns, and scrapes E.164 tokens out of the
Notes cell. Meanwhile the vault keys an identity on three fields —
`UNIQUE(account_id, normalized, handle_type, service)` in
`schema/sql/contacts.sql` — so an email handle or a WhatsApp identity
cannot be expressed in the file at all.

The starting proposal is a file whose columns match what the vault stores:
display name, service, handle type, identity. Handle type is a separate
column rather than inferred from the identity string, because a WhatsApp
JID contains an `@` and would be misread as an email by any inference rule.
The conversation should start further back than the columns, with whether
the contacts file remains a file at all or becomes something the vault
stores.

**Export scope.** `export-from-the-vault.md` describes entire vault,
current view, and selected. `query` already reaches the server, so current
view needs only the screen to send the browsing query. Selected needs a
query that names several conversations; the language has a `conversation:`
operator taking one value, and whether several can be combined is
unverified.

**Browser download.** The same page promises a download from the browser.
That needs a server endpoint that streams an export. `export_api.rs` has
not been read against this question.

**`scripts/test/smoke-vault-push.sh` is misnamed and unwired.** It never
runs `vault-push`; it starts the server and drives the HTTP API with
`curl`. No workflow or script calls it.

**Generated documentation is not checked for drift.** No workflow runs
`dump-cli-docs`, so the generated pages can fall behind the clap
definitions. After this work only one generated page remains, which makes
the check cheap enough to add.

## Vocabulary

`CONTEXT.md` gains these once the first change ships:

**Export**: moving messages out of the vault into files on disk, in a
format the person chooses. _Avoid_: Extract, Pull, Download.

**Convert**: rewriting a folder of already-exported files into a different
format, without reading the original backup or the vault. It is a distinct
operation from Export, which reads the vault. _Avoid_: Reexport, Transcode,
Reformat.

**Staging Directory**: the folder where Message Vault writes intermediate
files that neither the person nor the vault keeps — a backup being prepared
for import, or JSONL waiting to be converted into the format an export
asked for. Its contents are deleted when the job finishes. _Avoid_: Import
Staging Directory, Temp Folder, Working Directory.

Extract stops being a word for something a person does. It survives as the
internal name of the Tauri command that reads a backup during Import.
Convert stays in the vocabulary because the operation is still wanted; it
simply has no screen yet.
