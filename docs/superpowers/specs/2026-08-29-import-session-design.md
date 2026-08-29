# Import session — 2026-08-29

Give Import a session with a lifecycle: one active session per vault
account, a summary the user approves before anything is uploaded, an
honest success or failure verdict, and resume after a crash, a logout, or
a lost connection.

This spec records decisions from the 2026-08-29 design conversation. It is
not an implementation plan.

## Goal

A user starts an import, walks away, closes the laptop, comes back an hour
later, and picks up where they left off. When something goes wrong they are
told so, rather than being shown a success screen for a run that stored
nothing.

Three things have to become true:

1. The vault knows there is an import in progress, knows what stage it is
   at, and refuses to start a second one.
2. Nothing reaches the vault until the user has seen what the import will
   do and said yes.
3. Work already paid for is not thrown away by a crash.

## Current product

A `vault_imports` row already exists per run, with `source`, `mode`,
`status`, timings, counts, and `summary_json`, plus `vault_import_issues`
for per-item problems. That is a record, not a lifecycle. Four gaps.

**Nothing enforces one active session.** `start_import` inserts a row.
Two `running` rows for the same account are possible, and the staging
tables would corrupt each other if they overlapped — `staging_conversations`
is keyed `UNIQUE(account_id, chat_handle_id)`, per account rather than per
session. The invariant is already assumed by storage and enforced nowhere.

**Success is not measured.** `useImportJob.ts` sets `importCompleted = true`
as soon as `invokePush` returns, and turns that into `status: "completed"`.
But push runs with `continue_on_error: true`, so it returns a report instead
of throwing. The 2026-08-27 iPhone run — 681 conversations failed, 0
succeeded, 0 messages inserted — reached that line normally and was recorded
as completed. `pushResult.report` held the truth and nothing read it.

**Half the session is not persisted.** `stagingDir` is React state. A page
reload loses it, so Import always reopens on the form even when a staging
folder is sitting on disk, complete.

**Extract has no durable intermediate state.** `FormatSink::write_document`
pushes onto a `Vec`; every conversation file is written at `finish()`.
`open_prepared` calls `clean_previous_ir_output`, which deletes prior
artifacts including `attachments/`. So a failure anywhere before `finish()`
loses everything, and re-running deletes whatever was staged. An extract
that dies at 95% is worth exactly what one that died at 1% is worth.

Two smaller faults found while tracing this:

- `run_attachment_jobs` treats a *missing* attachment as a per-item issue
  (`missing_reason = "file_missing"`, continue) but a *read error* on one
  attachment as fatal to the whole extract, via `let loaded = load(i)?`.
  One unreadable file kills a multi-hour run.
- `apply_convert_or_compress` calls `media::process_attachments_dir`, not
  `process_attachments_dir_with_log`. The per-file log callback exists and
  is unused, which is why progress sits frozen through transcoding.

## Non-goals

- Undoing an import after it has been pushed. The gate sits before the
  irreversible step instead. `ix_messages_import_id` makes this cheap to
  add later if it is ever wanted.
- Sharing or resuming a session across machines. The staged work is local.
  Another device is told where the session belongs.
- Making parse resumable. Its output is in memory; there is nothing to
  resume from.
- Changing JSONL `schema_version` or `export.source`.
- Changing what `vault_imports.source` holds. The 2026-08-27 import session
  source spec settled that: the IR source (`imessage`, `whatsapp`), not the
  method id. Method ids live in `form_json` under this spec, which does not
  disturb that decision.
- A pre-extract scan of the vendor backup. It is accurate for iMessage,
  slow for SMS Backup & Restore, and impossible for WhatsApp, whose
  `msgstore.db` must be decrypted by an external tool before anything is
  countable. One screen with three reliability stories teaches users not to
  trust it.

## Decisions

### The session record

1. **The database is authoritative; the filesystem holds work products.**
   A session cannot live in the staging folder, because nothing about a
   directory listing identifies which folder is the session being resumed.
   The staging root accumulates folders from past runs, users change that
   root in Settings, and folders get renamed or deleted.

2. **`vault_imports` gains five columns.**

   | Column | Holds |
   |---|---|
   | `stage` | `parse`, `write`, `transcode`, `awaiting_approval`, `pushing` |
   | `staging_dir` | Absolute path to the staging folder on the client |
   | `device_id` | Which install created the session |
   | `form_json` | Form snapshot, to restore the screen and to restart with the same settings |
   | `source_fingerprint` | Source path, size, mtime, and message count |

   `status` says how a session ended. `stage` says where it is. Both are
   needed: a session can be at `stage = write` with `status = running`, or
   at `stage = write` with `status = failed`.

   `summary_json` already exists and carries the approved plan.

3. **One active session per account is a storage constraint, not
   application logic.** A partial unique index on `account_id` over the
   non-terminal statuses. Supported by both SQLite and Postgres, and it
   holds even against a racing client.

4. **Progress within a stage is recomputed, never stored.** For `write`,
   the conversation files already on disk are the completed set. For
   `pushing`, nothing is needed at all: dedupe no-ops messages that landed
   and `preflight_existing_assets` HEAD-skips assets the vault already
   holds, so restarting a push costs cheap requests rather than re-uploads.
   This is deliberate. Resume correctness never depends on a progress
   record being accurate, so a stale one costs time and nothing else.

   Reading files the session itself wrote is not state on disk. It is
   reading your own output.

5. **A client filesystem path in the vault database is unusual and
   intentional.** The vault may be remote, but it is the user's own vault
   and the desktop app is the only writer. It is a bookmark. Noted here so
   it is not mistaken for an oversight later.

### Stages

6. **Five stages: `parse` → `write` → `transcode` → `awaiting_approval` →
   `pushing`.** `transcode` exists only under `convert` and `compress`
   media modes. Under `copy` and `skip` the write phase is the whole of
   prepare.

7. **`prepare` stays the user-facing word.** It already labels this work
   ("Preparing messages") and already names the progress stage. `write` and
   `transcode` are its internal phases, not new vocabulary on screen.

8. **The four-step progress display becomes three.** Attachment progress
   moves inside the prepare step as sub-progress rather than its own row.
   The byte counter is useful and must not be lost in the merge.

### The approval contract

9. **Extract runs unasked; push waits.** Clicking Import starts prepare
   immediately. Abandoning during prepare costs a folder deletion, so
   there is nothing to protect the user from. Push is different: there is
   no rollback, so it does not begin until the user says yes.

10. **The summary is computed, not forecast.** After prepare, the staging
    folder holds the final bytes. Scanning it against `asset_max_bytes`
    gives the exact list of attachments that will be skipped. Reading the
    conversation files gives conversation count, message count, and the
    participant handles to match against vault contacts. No estimates.

11. **The summary reports, at minimum:** conversations, messages,
    attachments that will be skipped for size and which ones, and
    conversations whose participants match no contact in the vault.

12. **Approval is a contract, and the outcome is diffed against it.** A
    skip the user approved is an expected omission, not an error. A skip
    nobody forecast is an error even if there is only one. This is what
    makes "12 attachments too big" a normal import rather than a failure.

13. **Declining is terminal.** The session closes, the staging folder is
    deleted, and the next Import opens a clean form. A user who changes
    their mind pays for the work again. Keeping staging for a re-push with
    different settings would leave large folders on disk with no owner.

### Outcome

14. **Three outcomes with a zero floor.** `completed`, `completed with
    issues`, `failed`. Item-level problems are issues. A session is
    `failed` when it was interrupted, threw, or inserted nothing at all —
    zero conversations succeeded is a failure regardless of what the report
    says.

15. **The verdict is read from the push report.** `conversations_ok`,
    `conversations_failed`, `messages_inserted`. Not from whether
    `invokePush` returned.

### Prepare

16. **Parse must finish before anything is written.** The message stream is
    not grouped by conversation, so no conversation is provably complete
    until the scan ends. This is not negotiable and not worth working
    around.

17. **Write is a queue drained by worker threads.** Once parse is done,
    walk the tree, build a queue, and let writers pull from it.

18. **The queue unit is the conversation.** A worker writes a
    conversation's attachments, then its conversation file last. That gives
    the invariant resume depends on: if a conversation file exists,
    everything it references exists. Per-file queue items would allow a
    conversation file pointing at bytes that were never written, and the
    resume check would be a lie.

19. **Writers do not transcode.** Writing is cheap, transcoding is not, and
    mixing them puts the expensive work inside the checkpoint. Writers copy
    originals only.

20. **Transcode is a separate pass that patches the conversation files
    afterward.** It updates four fields per attachment: path,
    `digest_sha256`, `size_bytes`, and mime. The digest matters because the
    vault dedupes assets by sha256.

21. **Order: transcode, patch the conversation files, then delete
    originals.** Reversed, a crash between the last two leaves conversation
    files pointing at bytes that no longer exist — a staging folder that
    looks complete and is not. Holding originals until the patch lands is
    the extra copy this design already accepts.

    A consequence for resume: an interrupted transcode can leave an
    original whose derivative already exists but whose conversation file
    was never patched. A resumed run must treat a present derivative as
    converted and re-patch, not re-transcode. Distinguishing the three
    states — unconverted original, converted but unpatched, patched — is a
    requirement on the transcode pass, not an implementation detail to
    settle later.

22. **Accepted costs.** A second copy of every attachment while both exist,
    the initial copy paid before transcoding, and a re-read of the
    conversation files to patch them. Bought with them: a durable
    checkpoint, and a transcode failure that destroys nothing.

23. **Disk headroom is checked before the write phase.** Parse already
    knows total attachment bytes, and originals and derivatives now coexist
    by design.

24. **Worker counts differ by phase.** Writing is IO and hashing — it
    parallelizes. Transcoding shells out to ffmpeg, which is already
    multithreaded, so one process per core is often slower than sequential
    and makes the machine unusable. Writers scale; transcode uses a small
    bounded pool.

25. **`persist_clone` needs a unique temp suffix.** It builds `{name}.tmp`
    from the content digest, so two workers handling identical bytes would
    collide on the same temp path. Content-addressed dedup is otherwise
    unaffected by parallelism, because it is enforced through the
    filesystem.

### Resume

26. **Resume asks the vault, then goes where it says.** Get the active
    session, open `staging_dir`, confirm it exists and the fingerprint
    still matches, continue at `stage`. Entering Import runs this
    reconciliation first. The form is what appears when it finds nothing —
    not the default.

27. **Behaviour per case.**

    | Case | Resume |
    |---|---|
    | Died in `parse` | None. Form reopens with settings restored; folder deleted. |
    | Died in `write` | Re-parse, skip conversations already written. |
    | Died in `transcode` | Re-run it. A rescan finds exactly the unconverted originals. |
    | Died in `awaiting_approval` | Back to the summary, recomputed from the folder. |
    | Died in `pushing` | Re-push the folder. Dedupe and asset HEAD-skip absorb the overlap. |
    | Declined | Terminal. See decision 13. |
    | Cancelled mid-run | Same recovery as a crash at that stage; different wording on screen. |
    | Never answered at the gate | Session stays. Broken by an explicit discard, never a timeout. |
    | No folder at the recorded path | Before approval: restart with settings restored. After: discard only. |
    | Different `device_id` | Told where the session belongs. Discard is offered, never silent. |
    | Source changed or missing | Fatal for `write`; irrelevant for `awaiting_approval` and `pushing`. |

28. **No timeout reclaims a session.** A timer cannot distinguish an
    abandoned approval gate from a running three-hour transcode, and
    reclaiming a live session would corrupt an import in progress. The
    blocked screen offers an explicit discard instead.

29. **A fingerprint mismatch forces a clean restart, and says so.** A
    `chat.db` that grew since the last attempt has different conversation
    boundaries. Mixing old output with a new parse produces a corrupt
    export.

30. **The summary is recomputed on resume, not read back from
    `summary_json`.** The folder is the truth. `summary_json` records what
    the user approved, which is a different question and is used for the
    diff in decision 12.

### Required fixes

31. **A read error on one attachment becomes a per-item issue**, matching
    how a missing file is already handled. It is currently the most
    expensive failure mode in the system.

32. **`open_prepared` gains a resume mode** that does not call
    `clean_previous_ir_output`.

33. **`apply_convert_or_compress` calls the logging variant** so transcode
    reports progress. The callback already exists.

34. **`media::process_attachments_dir` takes an explicit file list.**
    Directory-wide operation is what prevents transcode from being scoped
    to a known set of files. The caller builds the list, which is also how
    a resumed run expresses "everything not yet converted" — see decision
    21 for the states it has to tell apart.

## Sequencing

This is more than one change. Suggested order, each independently
shippable:

1. **Verdict and issues.** Read the push report for the outcome; the
   three-way status; the read-error fix from decision 31. Fixes a live bug
   where failed imports report success, and needs none of the rest.
2. **The session record.** New columns, the partial unique index, the
   active-session endpoint, and reconciliation on entering Import. Delivers
   resume for `awaiting_approval` and `pushing`, which are the cheap cases
   and cover the logout scenario, without touching the exporters.
3. **The approval gate.** Summary computed from staging, approval before
   push, the contract diff from decision 12.
4. **The prepare restructure.** Write/transcode split, the queue and
   writers, and the four library fixes. Largest, riskiest, and the only one
   that touches shared crates every exporter depends on.

Steps 1 through 3 leave a coherent product on their own: a session that
resumes from the gate onward and tells the truth about outcomes. Step 4
extends resume backward into prepare.
