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

- Building per-import delete. Removing an imported conversation from the
  vault afterwards is planned and is substantial work of its own — staging
  has to be right first. This spec depends on it only for one line of
  Gate 2 copy (decision 16) and builds none of it. Today the sole delete
  is `delete_user_messages_handler`, which is admin-only and removes every
  message an account owns; `ix_messages_import_id` is the groundwork for a
  scoped one.
- Letting push run speculatively. A future delete is a cleanup path, not a
  licence to upload before the user has said yes.
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
   | `stage` | `parse`, `write`, `awaiting_gate_1`, `transcode`, `awaiting_gate_2`, `pushing` |
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

6. **Six stages, two of which are waits.**

   ```
   parse → write → awaiting_gate_1 → transcode → awaiting_gate_2 → pushing
   ```

   Under `copy` and `skip` there is no `transcode`, and the two waits
   collapse into one — the session goes `parse → write →
   awaiting_gate_1 → pushing`, and approving at Gate 1 starts the upload.

7. **Stage names are internal.** `transcode` in particular never reaches
   the screen; see decision 17. Progress events keep the existing `parse`
   / `attachments` / `prepare` vocabulary where it already exists, but the
   session's `stage` column is the list above.

8. **The progress display shows four steps: Read backup, Copy to staging,
   Convert (or Compress) media, Upload to vault.** Under `copy` and `skip`
   the media step is absent and there are three. The attachment byte
   counter belongs to Copy to staging, and the media step gets its own —
   `apply_convert_or_compress` currently reports nothing at all, so this
   depends on decision 41.

### The approval contract

9. **Two gates, and the media mode decides how many.** Under `convert` and
   `compress`: **Gate 1** after write, before the media step, and **Gate 2**
   after it, before push. Under `copy` and `skip` there is no media step,
   so Gate 1 is the only gate and its numbers are already exact.

   The two approve different things. Gate 1 approves **spending time** —
   converting or compressing every media file is the most expensive work
   in the pipeline, and nobody should pay for it before seeing that the
   import is worth running. Gate 2 approves **what lands in the vault**.

10. **Parse and write run unasked.** Clicking Import starts them
    immediately. Abandoning before Gate 1 costs a folder deletion, so
    there is nothing to protect the user from yet.

11. **Gate 1 is measured counts plus estimated verdicts.** The counts —
    conversations, messages, contacts, attachments, bytes on disk — are
    read from the staging folder and are exact. Only the per-file verdict
    for attachments over `asset_max_bytes` is a guess, because the media
    step has not run.

    It reports: conversations, messages, contacts (with how many match no
    contact in the vault), attachments, bytes copied, and a breakdown of
    the files over the limit into four states — fits as-is, likely to fit
    after the media step, probably still too big, and cannot be processed
    because it is not audio or video.

12. **The verdict heuristic runs only on files over the limit.** Those are
    by definition few, so each can afford a real `ffprobe` — already a hard
    requirement for `convert` and `compress`, so no new dependency. The
    estimate is

    ```
    estimate = size × (target_pixels / source_pixels)
                    × (target_fps / source_fps)
                    × codec_factor
    ```

    capped at the original size, with targets taken from the compress
    settings the form already carries and `codec_factor` of `0.7`. Below
    80% of the limit reads as *likely to fit*; above it as *probably still
    too big*. The margin stops a near miss from reading as a promise. The
    screen says it is an estimate.

13. **Gate 2 leads with the delta, not a fresh summary.** The question it
    answers is where Gate 1 was wrong: how many files we said would fit
    did, how many we wrote off came in under after all, and what failed
    that nobody flagged. The final upload state follows underneath.

14. **The outcome is diffed against Gate 2's approval, not Gate 1's.**
    Gate 1 approved spending time; only Gate 2 gated what enters the
    vault. A skip approved at Gate 2 is an expected omission. A skip
    nobody forecast is an error even if there is only one. This is what
    makes "12 attachments too big" a normal import rather than a failure.

15. **Declining at either gate is terminal.** The session closes, the
    staging folder is deleted, and the next Import opens a clean form. A
    user who changes their mind pays for the work again. Keeping staging
    for a re-push with different settings would leave large folders on
    disk with no owner.

16. **Gate 2 says imported conversations can be removed later.** In full:
    *"Messages are always uploaded. A skipped attachment leaves a
    placeholder in the conversation, and the message text is kept.
    Imported conversations can later be removed from your vault in the
    messages area."* This assumes the per-import delete named in the
    non-goals. The copy is written for the finished product; it is not to
    be hedged into a warning about irreversibility.

### Wording on screen

17. **"Transcode" never appears in the interface.** It is an internal
    stage name only. The user sees **Convert** or **Compress** according
    to the media mode, and those are two different jobs: convert changes
    the format, compress changes the format *and* targets a smaller size.

18. **In `convert` mode Gate 1 drops the size estimate.** Converting
    targets a format, not a size, so there is no meaningful forecast to
    approve. Gate 1 becomes "here is what we found" rather than "here is
    what we predict", the estimate column disappears, and files over the
    limit are reported as over the limit without a verdict. Gate 2 still
    runs, because sizes still change.

19. **Each screen's heading names the stage it is on.** *Review what was
    copied*, *Compressing media* (or *Converting media*), *Ready to
    upload*. Every state is titled "Import Messages" today, which tells
    the user nothing about where they are.

### Outcome

20. **Three outcomes with a zero floor.** `completed`, `completed with
    issues`, `failed`. Item-level problems are issues. A session is
    `failed` when it was interrupted, threw, or inserted nothing at all —
    zero conversations succeeded is a failure regardless of what the report
    says.

21. **The verdict is read from the push report.** `conversations_ok`,
    `conversations_failed`, `messages_inserted`. Not from whether
    `invokePush` returned.

### Prepare

22. **Parse must finish before anything is written.** The message stream is
    not grouped by conversation, so no conversation is provably complete
    until the scan ends. This is not negotiable and not worth working
    around.

23. **Write is a queue drained by worker threads.** Once parse is done,
    walk the tree, build a queue, and let writers pull from it.

24. **The queue unit is the conversation.** A worker writes a
    conversation's attachments, then its conversation file last. That gives
    the invariant resume depends on: if a conversation file exists,
    everything it references exists. Per-file queue items would allow a
    conversation file pointing at bytes that were never written, and the
    resume check would be a lie.

25. **Writers do not transcode.** Writing is cheap, transcoding is not, and
    mixing them puts the expensive work inside the checkpoint. Writers copy
    originals only.

26. **Transcode is a separate pass that patches the conversation files
    afterward.** It updates four fields per attachment: path,
    `digest_sha256`, `size_bytes`, and mime. The digest matters because the
    vault dedupes assets by sha256.

27. **Transcode commits per file, through a rename.** For each attachment:
    transcode to `<derivative>.in_progress`, patch the conversation file,
    rename `.in_progress` to the final name, delete the original. The final
    name never exists until the conversation file already points at it.

    Two invariants fall out, and they are the whole of transcode resume:

    - A file under its final derivative name is fully patched.
    - An original still on disk means work remains.

    So the resume list is every original still present. There is no state
    to classify and no progress to record.

    A resumed run always re-transcodes the file it interrupted rather than
    adopting the `.in_progress` bytes. This is required, not merely
    simpler: a crash during the write leaves a truncated file, and nothing
    distinguishes a complete `.in_progress` from a partial one without
    hashing it. The cost is one file.

    Reversing the order — deleting an original before its conversation file
    commits — leaves conversation files pointing at bytes that no longer
    exist, a staging folder that looks complete and is not.

28. **The patch reads the file on disk; it never replays a captured
    remap.** ffmpeg output is not guaranteed byte-identical across runs, so
    a re-transcoded file can carry a different sha256. The vault dedupes
    assets by sha256, so writing a stale digest would corrupt silently.
    Digest, size, and mime are recomputed from the derivative each time.

29. **The `.in_progress` marker must survive existing cleanup.**
    `process_attachments_dir` calls `remove_msgmedia_temps` on entry to
    clear leftovers from a failed ffmpeg run. The marker has to be named
    distinctly from ffmpeg's scratch files and be exempt from that sweep,
    or the resume signal is deleted on the way in.

30. **Accepted costs.** The initial copy paid before transcoding, one
    attachment held in two forms while its own patch is in flight, and a
    re-read of the conversation files to patch them. Bought with them: a
    durable checkpoint, and a transcode failure that destroys nothing.

31. **Disk headroom is checked before the write phase.** Parse already
    knows total attachment bytes. Because decision 27 commits and deletes
    per file, peak usage is roughly the original total plus one in-flight
    derivative, not originals plus derivatives — each original is released
    as soon as its own patch lands.

32. **Worker counts differ by phase.** Writing is IO and hashing — it
    parallelizes. Transcoding shells out to ffmpeg, which is already
    multithreaded, so one process per core is often slower than sequential
    and makes the machine unusable. Writers scale; transcode uses a small
    bounded pool.

33. **`persist_clone` needs a unique temp suffix.** It builds `{name}.tmp`
    from the content digest, so two workers handling identical bytes would
    collide on the same temp path. Content-addressed dedup is otherwise
    unaffected by parallelism, because it is enforced through the
    filesystem.

### Resume

34. **Resume asks the vault, then goes where it says.** Get the active
    session, open `staging_dir`, confirm it exists and the fingerprint
    still matches, continue at `stage`. Entering Import runs this
    reconciliation first. The form is what appears when it finds nothing —
    not the default.

35. **Behaviour per case.**

    | Case | Resume |
    |---|---|
    | Died in `parse` | None. Form reopens with settings restored; folder deleted. |
    | Died in `write` | Re-parse, skip conversations already written. |
    | Died in `transcode` | Re-run it over every original still on disk. |
    | Died at either gate | Back to the summary, recomputed from the folder. |
    | Died in `pushing` | Re-push the folder. Dedupe and asset HEAD-skip absorb the overlap. |
    | Declined | Terminal. See decision 15. |
    | Cancelled mid-run | Same recovery as a crash at that stage; different wording on screen. |
    | Never answered at the gate | Session stays. Broken by an explicit discard, never a timeout. |
    | No folder at the recorded path | Before approval: restart with settings restored. After: discard only. |
    | Different `device_id` | Told where the session belongs. Discard is offered, never silent. |
    | Source changed or missing | Fatal for `write`; irrelevant at either gate and during `pushing`. |

36. **No timeout reclaims a session.** A timer cannot distinguish an
    abandoned approval gate from a running three-hour transcode, and
    reclaiming a live session would corrupt an import in progress. The
    blocked screen offers an explicit discard instead.

37. **A fingerprint mismatch forces a clean restart, and says so.** A
    `chat.db` that grew since the last attempt has different conversation
    boundaries. Mixing old output with a new parse produces a corrupt
    export.

38. **The summary is recomputed on resume, not read back from
    `summary_json`.** The folder is the truth. `summary_json` records what
    the user approved, which is a different question and is used for the
    diff in decision 14.

### Required fixes

39. **A read error on one attachment becomes a per-item issue**, matching
    how a missing file is already handled. It is currently the most
    expensive failure mode in the system.

40. **The missing-attachment reasons become a closed set with an explicit
    unknown.** Today `missing_reason` is a free-form `Option<String>` with
    no enum, and the display side flattens anything it does not recognize
    to a bare `"missing"`. There is no `other` — unrecognized reasons are
    swallowed rather than categorized.

    | Reason | Set where | Shown as |
    |---|---|---|
    | `file_missing` | `attachment_jobs.rs` — unreadable or empty | The file could not be read from the backup |
    | `too_large` | `vault-push` — over `asset_max_bytes` | Larger than the 50 MB limit |
    | `not_copied` | `attachment_jobs.rs` and the iMessage exporter | Not copied, by your import setting |
    | `convert_failed: <detail>` | `attachment_jobs.rs` — carries ffmpeg's reason | Could not be converted — `<detail>` |
    | `unknown: <raw>` | anything unrecognized | Could not be imported — `<raw>` |

    Three changes follow. `skipped` and `embed_disabled` are one condition
    written two ways and collapse to `not_copied`. `convert_failed` gets
    matched by prefix so its detail survives to the screen instead of
    falling through — it is the only reason carrying a real explanation
    and today it is the one guaranteed to be discarded. And the fallback
    keeps the raw string rather than replacing it, so an unrecognized
    reason is visible and reportable instead of silently uniform.

    `no_path` is not a stored reason. It is a display default in
    `run.rs` for an attachment with no path and no reason, and should not
    be added to the set.

41. **`open_prepared` gains a resume mode** that does not call
    `clean_previous_ir_output`.

42. **`apply_convert_or_compress` calls the logging variant** so transcode
    reports progress. The callback already exists.

43. **`media::process_attachments_dir` takes an explicit file list.**
    Directory-wide operation is what prevents transcode from being scoped
    to a known set of files. The caller builds the list; on a resumed run
    that list is every original still on disk, per decision 27.

## Sequencing

This is more than one change. Suggested order, each independently
shippable:

1. **Verdict and reason vocabulary.** Read the push report for the outcome
   (decision 21); the three-way status; the read-error fix (decision 39);
   the closed reason set with its explicit unknown (decision 40). Fixes a
   live bug where failed imports report success, and needs none of the
   rest.
2. **The session record.** New columns, the partial unique index, the
   active-session endpoint, and reconciliation on entering Import. Delivers
   resume at both gates and during `pushing`, which are the cheap cases
   and cover the logout scenario, without touching the exporters.
3. **The gates and the screens.** Both gates, the verdict heuristic
   (decision 12), the contract diff (decision 14), the convert/compress
   wording (decisions 17–19), and the redesigned Import screens.
4. **The prepare restructure.** Write/transcode split, the queue and
   writers, and the four library fixes (decisions 39–43). Largest,
   riskiest, and the only one that touches shared crates every exporter
   depends on.

Steps 1 through 3 leave a coherent product on their own: a session that
resumes from the first gate onward and tells the truth about outcomes.
Step 4 extends resume backward into prepare.

Per-import delete is not in this sequence. It is separate work, named in
the non-goals, and only decision 16's wording anticipates it.
