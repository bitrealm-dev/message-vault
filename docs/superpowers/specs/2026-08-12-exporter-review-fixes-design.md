# Exporter review fixes — design

**Date:** 2026-08-12  
**Status:** Approved for implementation (review findings 1–8)

## Problem

A code review of `crates/exporters/*` found correctness and safety gaps: OpenExtract can delete its own input CSVs when `--output` equals `--input`; WhatsApp treats the process working directory as an allowed media root; iMessage can keep truncated attachment files after a crash; GO SMS Pro group IDs can collide; iMazing message IDs change when a missing attachment is later found; WhatsApp international numbers and “no copy” attachment metadata are wrong; SMS Backup+ ignores cancel during parallel parse and can merge distinct photo MMS; several smaller consistency gaps remain.

## Goals

1. Refuse unsafe input/output overlap before any export clean runs.
2. Keep WhatsApp media copies inside explicit backup/work roots only.
3. Make iMessage attachment file writes crash-safe (temp file then rename).
4. Make GO SMS Pro group conversation IDs unambiguous.
5. Stabilize iMazing message GUIDs across re-runs when attachment digests exist.
6. Store WhatsApp phone handles as E.164 with `+` when the JID is a phone number; stop treating path strings as content digests when media is not copied.
7. Honor cancel during SMS Backup+ parallel parse; reduce peak memory by chunking; keep distinct same-second photo MMS separate when digests differ.
8. Align remaining should-fix items: iMessage output prep and `missing_reason`, OpenExtract row dedupe, GO SMS multi-peer MMS as groups, plus tests for the above.

## Non-goals

- Killing the external `wtsexporter` child mid-run (platform-specific; leave as a follow-up).
- Changing iMazing’s ambiguous basename matching heuristics beyond GUID digest use.
- Streaming every exporter’s conversation write path (FormatSink still buffers documents).
- Building a full synthetic `chat.db` smoke fixture for iMessage in this pass (unit tests cover the write path).

## Approach

Copy patterns that already exist in hardened exporters:

- Input/output identity check from GO SMS Pro / iMazing (`canonicalize`, then refuse equal or nested paths) into OpenExtract and SMS Backup+.
- Atomic write+rename from SMS Backup & Restore staging into iMessage `persist_attachment`.
- Length-prefixed group keys from SMS Backup+ `flat_eml.rs` into GO SMS Pro `chat_id_group`.
- Content digests in `stable_guid` material (already used by GO SMS / SMS Backup+) into iMazing.

WhatsApp media roots become: wtsexporter work dir, backup input, optional absolute media paths — never a blanket current working directory.

SMS Backup+ parallel parse checks the cancel flag at the start of each file; EML paths are processed in chunks so outcomes are not all held at once; `cover_identity` appends sorted attachment digests when the message has media and empty/normalized-empty text would otherwise collide.

## Success criteria

- OpenExtract / SMS Backup+ smoke or unit tests prove `output == input` fails and source files survive.
- WhatsApp unit tests reject media under CWD when CWD is not an explicit allowed root.
- iMessage unit test proves partial/temp writes do not leave a final truncated dest that later runs accept as complete.
- GO SMS unit test: `["12","34"]` and `["123","4"]` get different chat IDs.
- iMazing GUID uses digests when present.
- WhatsApp non-US JID maps to `+…`; no-copy attachments omit content digests and host absolute paths as IR `path`.
- SMS Backup+ cancel returns during parallel work; two same-second empty-text MMS with different digests stay two messages.
- All touched exporter crates pass `cargo test -p <crate>`.
