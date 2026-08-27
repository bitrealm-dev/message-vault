# Libs follow-up implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix all 13 Libs findings from the product Rust audit — the `missing_docs` gate and full public-surface documentation in all 11 lib crates, the three doc-quality fixes, both error-handling fixes, the shared attachment-metadata consolidation, the shared test fixture, and the go-sms-mms decode split — with zero change to serialized output or user-visible behavior.

**Architecture:** One `AttachmentMeta` struct in `message-ir` becomes the shared attachment-metadata core reused by the CSV cell and mail MIME layers via composition and `From` impls, replacing the hand-written field mappings. Each lib crate gains `#![warn(missing_docs)]` in the same task as its documentation sweep. `ir-format` exposes the unsafe-attachment-path message as a `pub const` that the server's test asserts import. A `testutil` feature in `message-ir` carries the shared `sample_document()` fixture.

**Tech Stack:** Rust workspace, serde (flatten for the CSV cell JSON shape), anyhow 1.0.103, chrono, clap (reexport only), rustdoc + `missing_docs` lint.

## Global Constraints

From `docs/superpowers/specs/2026-08-23-libs-followup-design.md` — every task's requirements implicitly include this section:

- **Behavior-preserving.** Byte-identical serialized output (JSON, CSV, EML, XML) and identical error text everywhere. Public Rust API changes are allowed within the workspace (all consumers are workspace members) when compiler-guided.
- **Green after every task.** `cargo test --workspace` (67 targets), `cargo fmt --check`, `cargo check --workspace`, and `cargo clippy --workspace -- -D warnings` all clean after every task commit.
- **Docs gate.** `cargo doc --no-deps -p <crate>` emits zero warnings for every crate a task gates. The gate is `#![warn(missing_docs)]` (warn, not deny). No `#[allow(missing_docs)]` anywhere.
- **No generated artifacts change.** `openapi.json` and the CLI reference pages stay byte-identical unless a task explicitly regenerates them (only the reexport task may touch CLI pages; see Task 13). The committed-dump tests must stay green.
- **Doc style.** `docs/src/content/docs/vault/developer/rustdoc-style.md` governs all doc text: first sentence states what the item is; no filler or invented terms; real links; every public item documented.
- **Surface.** Each crate's `lib.rs` re-exports remain the curated public API.
- **No new crates, no dependency version bumps.** Adding a dependency at an existing pinned version is allowed (csv and mail gain dependencies in Tasks 2–3).
- **Line anchors.** The audit's line numbers are quoted for context only; the compiler and `cargo doc` are authoritative — if a cited line has drifted, find the item by name.

---

### Task 1: message-ir documentation, quality fixes, and gate

Findings 1 (high — Core IR model types have no doc comments), 5 ("Absent when present"), 6 (broken doc link), and the `message-ir` part of finding 8 (no missing_docs gate).

**Files:**
- Modify: `crates/libs/ir/src/lib.rs`

**Interfaces:**
- Produces: a documented, gated `message-ir` with the fixed architecture link; `IrAttachment` keeps its flat field layout (Tasks 2, 8 build on this).
- Consumes: nothing new.

- [ ] **Step 1: Add the gate and fix the broken link**

At the top of `crates/libs/ir/src/lib.rs`, directly after the `//!` intro block (which ends at line 11) and before the `use` statements, insert:

```rust
#![warn(missing_docs)]
```

Replace line 7's link text: `See the [message-ir architecture](../../../docs/maintainers/architecture/message-ir.md).` becomes `See the [common message](https://bitrealm.io/vault/developer/architecture/common-message/) page.`

Run `cargo doc --no-deps -p message-ir 2>&1 | grep -c "missing documentation"` — expect a large count (the RED: the gate now flags every undocumented pub item; the link fix alone does not clear warnings).

- [ ] **Step 2: Document every public item**

Add the following doc comments to `crates/libs/ir/src/lib.rs` at the exact items named. The item's current line number is given for orientation; place the `///` immediately above the item (above its `#[derive]` if present). Copy the text verbatim — first sentence states what the item is, per the style guide.

- `ConversationDocument` (line 20): `/// One exported chat: export metadata, conversation roster and stats, and messages.\n///\n/// This is the common-message schema every exporter writes and every reader\n/// parses. See the [common message](https://bitrealm.io/vault/developer/architecture/common-message/) page.` Fields (add above each field):
  - `schema_version`: `/// Schema version written into this document (currently 3).`
  - `export`: `/// Where and how this export was produced.`
  - `conversation`: `/// Roster and computed stats for this chat.`
  - `messages`: `/// Messages in timestamp order.`
  - `packaging_stem_suffix`: already documented — leave it.
- `ExportMeta` (line 31): `/// Provenance of an export: which backup tool and account it came from.` Fields:
  - `source`: `/// Backup source id (e.g. \`sms-backup-restore\`).`
  - `tool`: `/// Human tool name (e.g. \`SMS Backup & Restore\`).`
  - `tool_version`: `/// Version string of the tool.`
  - `owner_handle`: `/// Owner handle used for outgoing rows; \`None\` when the backup has no owner identity.`
  - `owner_display_name`: already documented — leave it.
- `IrConversationType` (line 42): `/// Individual or group chat.` Variants:
  - `Individual`: `/// One-on-one chat with a single peer.`
  - `Group`: `/// Chat with multiple peers.`
- `IrConversationType::as_str` (line 48): `/// Lowercase storage id (\`individual\` / \`group\`).`
- `IrConversationType::parse` (line 55): `/// Parse a storage id; anything but \`group\` (case-insensitive) is \`Individual\`.`
- `HandleType` (line 65): `/// Kind of a participant handle.` Variants:
  - `Phone`: `/// Telephone number.`
  - `Email`: `/// Email address.`
  - `Username`: `/// App username (e.g. Telegram \`@user\`).`
  - `Other`: `/// Any handle that is not phone, email, or username.`
- `HandleType::as_str` (line 73): `/// Lowercase storage id (\`phone\` / \`email\` / \`username\` / \`other\`).`
- `HandleType::parse` (line 82): `/// Parse a storage id; unknown values map to \`Other\`.`
- `ConversationMeta` (line 93): `/// Roster and computed stats for one chat.` Fields:
  - `chat_identifier`: `/// Stable chat id from the source (E.164, group key, or app thread id).`
  - `conversation_type`: `/// Individual or group.`
  - `group_title`: `/// Group display title; \`None\` for individuals and untitled groups.`
  - `participants`: `/// Roster of handles and display names.`
  - `stats`: `/// Computed counts and first/last timestamps.`
- `ConversationStats` (line 102): `/// Message and attachment counts plus first and last message timestamps,\n/// computed from \`messages\` at write time.` Fields:
  - `message_count`: `/// Number of messages in the chat.`
  - `attachment_count`: `/// Total attachments across all messages.`
  - `first_timestamp_unix_ms`: `/// Earliest message timestamp; \`None\` when the chat has no messages.`
  - `last_timestamp_unix_ms`: `/// Latest message timestamp; \`None\` when the chat has no messages.`
- `IrParticipant` (line 110): `/// One chat member: handle, optional display name and handle type.` Fields:
  - `handle`: `/// Phone, email, or username string.`
  - `display_name`: `/// Display name shown in UIs; \`None\` when the source has none.`
  - `handle_type`: `/// Known kind of \`handle\`; \`None\` when the source did not record one.`
- `IrService` (line 119): `/// Transport a message arrived on.` Variants:
  - `Sms`: `/// SMS text.`
  - `IMessage`: `/// Apple iMessage (serialized as \`imessage\`).`
  - `Whatsapp`: `/// WhatsApp.`
  - `Rcs`: `/// RCS (Android).`
  - `Discord`: `/// Discord.`
  - `Signal`: `/// Signal.`
  - `Telegram`: `/// Telegram.`
  - `Slack`: `/// Slack.`
  - `Unknown`: `/// Unrecognized or unset service.`
- `IrService::as_str` (line 133): `/// Lowercase storage id (\`sms\` / \`imessage\` / \`whatsapp\` / …).`
- `IrService::parse` (line 147): `/// Parse a storage id; unknown values map to \`Unknown\`.`
- `HandleService` — documented (lines 162-165). Add variant docs:
  - `Phone` (line 168): `/// Phone platform (SMS/iMessage/RCS are transports, not platforms).`
  - `Whatsapp` (line 169): `/// WhatsApp platform.`
- `IrMessageKind` (line 209): `/// Shape of one message row.` Variants:
  - `Sms`: `/// Plain SMS text.`
  - `Mms`: `/// Multimedia message.`
  - `IMessage`: `/// iMessage (serialized as \`imessage\`).`
  - `Tapback`: `/// iMessage tapback reaction.`
  - `StickerTapback`: `/// iMessage sticker tapback.`
  - `Announcement`: `/// iMessage announcement (e.g. group rename).`
  - `LocationShare`: `/// iMessage shared location.`
  - `Balloon`: `/// iMessage Digital Touch balloon.`
  - `Unknown`: `/// Unrecognized or unset kind.`
- `IrMessageKind::as_str` (line 223): `/// Lowercase storage id (\`sms\` / \`mms\` / \`imessage\` / \`tapback\` / …).`
- `IrMessageKind::parse` (line 237): `/// Parse a storage id; unknown values map to \`Unknown\`.`
- `IrMessage` (line 253): `/// One message in a conversation: sender, body text, and attachments.` Fields:
  - `guid`: `/// Stable message id; derived from content when the source has no id\n/// (see \`message_csv::stable_guid\`).`
  - `timestamp_unix_ms`: `/// Unix milliseconds; the chronological sort key.`
  - `direction`: `/// Incoming or outgoing.`
  - `service`: `/// Transport the message arrived on.`
  - `message_kind`: `/// Row shape.`
  - `sender_handle`: `/// Handle of the actual sender (the owner's handle for outgoing).`
  - `sender_display_name`: `/// Display name of the actual sender.`
  - `subject`: `/// Message subject line (rare).`
  - `text`: `/// Plain-text body; never includes attachment data.`
  - `attachments`: `/// Attachment metadata in order; bytes live on disk or in \`bytes\`.`
  - `imessage`: `/// Apple extensions; \`None\` for non-iMessage messages.`
  - `source`: `/// Vendor leftovers (Android type code and raw fields).`
- `IrDirection` (line 270): `/// Whether the owner sent or received the message.` Variants:
  - `Incoming`: `/// Received from a peer.`
  - `Outgoing`: `/// Sent by the owner.`
- `IrDirection::as_str` (line 276): `/// Lowercase storage id (\`incoming\` / \`outgoing\`).`
- `IrAttachment` (line 285): `/// Metadata for one attachment.\n///\n/// Bytes are never serialized: JSON, JSONL, and CSV carry only this metadata,\n/// and the bytes live in a sidecar file under \`attachments/\` (or in \`bytes\`\n/// for in-memory EML/MBOX/XML embedding).` Fields:
  - `path`: `/// Relative path under \`attachments/\` to the staged file.`
  - `original_name`: `/// Filename the sender's device had for the file.`
  - `mime_type`: `/// Detected or declared MIME type.`
  - `digest_sha256`: `/// 64-hex SHA-256 of the file contents (content addressing).`
  - `is_sticker`: `/// Sticker flag.`
  - `transcription`: `/// OCR text of an image attachment.`
  - `sticker_effect`: `/// iMessage sticker effect name.`
  - `size_bytes`: already documented — leave it.
  - `missing_reason` (line 296): replace the existing doc with `/// None when the attachment was imported; set (\`too_large\` / \`file_missing\`)\n/// only when bytes were skipped.`
  - `bytes`: already documented — leave it.
- `IrSource` — documented (line 304). Add field docs:
  - `android_type`: `/// Android \`type\` attribute from the source (e.g. 1 = received, 2 = sent).`
  - `fields`: `/// Raw vendor attributes; display names never live here.`
- `IrImessage` — documented (lines 322-323). Add field docs:
  - `is_reply`: `/// This message is a reply to an earlier message.`
  - `in_reply_to_guid`: `/// GUID of the message this replies to.`
  - `thread_originator_part`: `/// Part index of the thread originator.`
  - `num_replies`: `/// Number of replies under this message.`
  - `is_deleted`: `/// Sender deleted the message.`
  - `send_effect`: `/// iMessage send effect (e.g. \`slam\`).`
  - `shared_location`: `/// Shared-location payload.`
  - `announcement`: `/// Announcement payload (e.g. group rename).`
  - `read_receipt_rfc3339`: `/// RFC 3339 timestamp of the read receipt.`
  - `parts`: `/// Apple \`parts\` blob as a JSON value.`
  - `edits`: `/// Apple \`edits\` blob as a JSON value.`
  - `tapbacks`: `/// Apple \`tapbacks\` blob as a JSON value.`
  - `app`: `/// Apple \`app\` blob as a JSON value.`
  - `balloon_bundle_id`: `/// Digital Touch balloon bundle id.`
  - `balloon_kind`: `/// Digital Touch balloon kind.`
  - `associated_guid`: `/// Tapback target message GUID.`
  - `associated_part`: `/// Tapback target part index.`
  - `tapback_kind`: `/// Tapback kind string.`
  - `tapback_emoji`: `/// Tapback emoji.`
  - `tapback_action`: `/// Tapback action string.`
- `ConversationHeader` (line 452): `/// Export and conversation metadata without messages (JSONL header line\n/// and CSV header row).` Fields:
  - `schema_version`: `/// Schema version written into this header (currently 3).`
  - `export`: `/// Where and how this export was produced.`
  - `conversation`: `/// Roster and computed stats for this chat.`

Items not listed already carry docs — do not rewrite them.

- [ ] **Step 3: Verify the gate is clean**

Run `cargo doc --no-deps -p message-ir 2>&1 | grep -E "warning|error"` — expect **zero** lines (GREEN: every pub item documented, no broken links, no `missing_docs` warnings). If warnings remain, each names the item and line — document that item per the style guide and re-run until clean.

- [ ] **Step 4: Run the tests**

Run: `cargo test -p message-ir` and `cargo clippy -p message-ir -- -D warnings` and `cargo fmt --check`
Expected: all pass, output pristine (no stray warnings).

- [ ] **Step 5: Commit**

```bash
git add crates/libs/ir/src/lib.rs
git commit -m "docs(libs): document message-ir model types, fix link and wording"
```

---

### Task 2: Shared AttachmentMeta and composition in csv and mail layers

Finding 11 (medium — attachment metadata shape triplicated across libs).

**Refinement of the spec (smaller ripple, same fix):** `IrAttachment` itself keeps its flat fields (composing it would touch 34 construction sites across 5 exporters, the CLI, and demo-seed, and the audit's suggestion only requires the csv and mail layers to reuse a shared type). The shared `AttachmentMeta` is composed by `csv::AttachmentCell` (serde-flattened so the CSV `attachments_json` shape is byte-identical) and by `mail::MailAttachment` (which has no serde derive). `From` impls replace the hand-written field mappings.

**Files:**
- Modify: `crates/libs/ir/src/lib.rs`, `crates/libs/csv/src/lib.rs`, `crates/libs/mail/src/lib.rs`, `crates/libs/mail/src/parse.rs`, `crates/libs/mail/Cargo.toml`, `crates/libs/ir-format/src/write.rs`, `crates/libs/ir-format/src/read_csv.rs`, `crates/libs/ir-format/src/read_mail.rs`, `crates/exporters/imazing-exporter/src/attachments.rs` (5 construction sites, lines ~119-172), `crates/exporters/imessage-ir-exporter/src/emit.rs` (3 construction sites, lines 728, 849, 1142)

**Interfaces:**
- Consumes: `message_ir::IrAttachment` (flat, from Task 1).
- Produces: `message_ir::AttachmentMeta { path, original_name, mime_type, digest_sha256 }` with `From<&IrAttachment> for AttachmentMeta` and `From<AttachmentCell> for IrAttachment`; composed `AttachmentCell` and `MailAttachment`; an ir-format-local `From<&MailAttachment> for IrAttachment` (mail cannot depend on ir without a cycle — ir already depends on csv, and the reverse is impossible for mail).

- [ ] **Step 1: Add AttachmentMeta to message-ir**

In `crates/libs/ir/src/lib.rs`, place this struct directly above `IrAttachment` (line 284):

```rust
/// Core attachment metadata shared by the IR attachment, the CSV cell, and the
/// mail MIME layer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttachmentMeta {
    /// Relative path under `attachments/` to the staged file.
    pub path: Option<String>,
    /// Filename the sender's device had for the file.
    pub original_name: Option<String>,
    /// Detected or declared MIME type.
    pub mime_type: Option<String>,
    /// 64-hex SHA-256 of the file contents (content addressing).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub digest_sha256: Option<String>,
}

impl From<&IrAttachment> for AttachmentMeta {
    fn from(a: &IrAttachment) -> Self {
        Self {
            path: a.path.clone(),
            original_name: a.original_name.clone(),
            mime_type: a.mime_type.clone(),
            digest_sha256: a.digest_sha256.clone(),
        }
    }
}
```

- [ ] **Step 2: Compose the struct into csv::AttachmentCell**

In `crates/libs/csv/src/lib.rs`, replace the `AttachmentCell` definition (lines 13-25) with:

```rust
/// One attachment object written into `attachments_json`.
#[derive(Debug, Serialize, Deserialize)]
pub struct AttachmentCell {
    /// Shared attachment metadata (serialized inline — same JSON shape as before).
    #[serde(flatten)]
    pub meta: message_ir::AttachmentMeta,
    #[serde(default)]
    pub is_sticker: bool,
    pub transcription: Option<String>,
    pub sticker_effect: Option<String>,
}
```

Add to `crates/libs/csv/Cargo.toml` `[dependencies]`:

```toml
message-ir = { path = "../ir" }
```

- [ ] **Step 3: Add the From<AttachmentCell> for IrAttachment impl**

In `crates/libs/ir/src/lib.rs`, after the `AttachmentMeta` impl added in Step 1 (ir already depends on message-csv, so this impl lives here):

```rust
impl From<message_csv::AttachmentCell> for IrAttachment {
    fn from(cell: message_csv::AttachmentCell) -> Self {
        let message_csv::AttachmentCell {
            meta,
            is_sticker,
            transcription,
            sticker_effect,
        } = cell;
        Self {
            path: meta.path,
            original_name: meta.original_name,
            mime_type: meta.mime_type,
            digest_sha256: meta.digest_sha256,
            is_sticker,
            transcription,
            sticker_effect,
            size_bytes: None,
            missing_reason: None,
            bytes: None,
        }
    }
}
```

- [ ] **Step 4: Compose the struct into mail::MailAttachment**

In `crates/libs/mail/src/lib.rs`, replace the `MailAttachment` definition (lines 57-67) with:

```rust
/// Attachment bytes plus metadata for MIME parts / `X-ME-Attachment-Meta`.
#[derive(Debug, Clone)]
pub struct MailAttachment {
    /// Raw file bytes attached as a MIME part.
    pub bytes: Vec<u8>,
    /// Shared attachment metadata (`path` is always `None` — mail archives never
    /// store an on-disk path; readers restore the IR path separately).
    pub meta: message_ir::AttachmentMeta,
    pub is_sticker: bool,
    pub transcription: Option<String>,
    pub sticker_effect: Option<String>,
}
```

Add to `crates/libs/mail/Cargo.toml` `[dependencies]`:

```toml
message-ir = { path = "../ir" }
```

- [ ] **Step 5: Update the four mail/csv construction sites in this workspace**

1. `crates/libs/ir-format/src/write.rs` lines 414-422: replace the `MailAttachment { ... }` literal with:

```rust
            attachments.push(MailAttachment {
                bytes,
                meta: a.into(),
                is_sticker: a.is_sticker,
                transcription: a.transcription.clone(),
                sticker_effect: a.sticker_effect.clone(),
            });
```

2. `crates/libs/ir-format/src/write.rs` lines 258-270: replace the `AttachmentCell { ... }` literal with:

```rust
            .map(|a| AttachmentCell {
                meta: a.into(),
                is_sticker: a.is_sticker,
                transcription: a.transcription.clone(),
                sticker_effect: a.sticker_effect.clone(),
            })
```

3. `crates/libs/mail/src/parse.rs` lines 252-260: replace the `MailAttachment { ... }` literal with:

```rust
        out.push(MailAttachment {
            bytes,
            meta: message_ir::AttachmentMeta {
                path: None,
                original_name: m.and_then(|c| c.original_name.clone()).or(name_fallback),
                mime_type: m.and_then(|c| c.mime_type.clone()).or(mime_fallback),
                digest_sha256: m.and_then(|c| c.digest_sha256.clone()),
            },
            is_sticker: m.map(|c| c.is_sticker).unwrap_or(false),
            transcription: m.and_then(|c| c.transcription.clone()),
            sticker_effect: m.and_then(|c| c.sticker_effect.clone()),
        });
```

and delete the now-dead lines 261-262 (`// path from meta is unused ...` and `let _ = m.and_then(|c| c.path.clone());`).

4. `crates/libs/mail/src/lib.rs` lines 994 and 1200 (test fixtures): update the two `MailAttachment { ... }` literals to the composed shape (`meta: message_ir::AttachmentMeta { path: None, original_name, mime_type, digest_sha256 }, ...` — copy the exact field values the tests already use).

5. `crates/exporters/imazing-exporter/src/attachments.rs` (5 sites, lines ~119-172) and `crates/exporters/imessage-ir-exporter/src/emit.rs` (3 sites, lines 728, 849, 1142): update every `AttachmentCell { ... }` / `MailAttachment { ... }` literal to the composed shape, carrying the exact same values (the compiler guides; the files already import both types).

- [ ] **Step 6: Replace the two reader mappings with From impls**

In `crates/libs/ir-format/src/read_csv.rs`, replace the body of `parse_attachments` (lines 279-300) with:

```rust
fn parse_attachments(raw: &str) -> Result<Vec<IrAttachment>> {
    if raw.trim().is_empty() || raw.trim() == "null" {
        return Ok(Vec::new());
    }
    let cells: Vec<AttachmentCell> =
        serde_json::from_str(raw).with_context(|| format!("parse attachments_json: {raw}"))?;
    Ok(cells.into_iter().map(Into::into).collect())
}
```

In `crates/libs/ir-format/src/read_mail.rs`, replace the body of `attachment_from_mail` (lines 204-222) with:

```rust
/// Map one mail attachment into [`IrAttachment`].
fn attachment_from_mail(a: &mail::MailAttachment) -> IrAttachment {
    let mut att: IrAttachment = a.into();
    att.bytes = if a.bytes.is_empty() {
        None
    } else {
        Some(a.bytes.clone())
    };
    att
}

impl From<&mail::MailAttachment> for IrAttachment {
    fn from(a: &mail::MailAttachment) -> Self {
        Self {
            path: None,
            original_name: a.meta.original_name.clone(),
            mime_type: a.meta.mime_type.clone(),
            digest_sha256: a.meta.digest_sha256.clone(),
            is_sticker: a.is_sticker,
            transcription: a.transcription.clone(),
            sticker_effect: a.sticker_effect.clone(),
            size_bytes: None,
            missing_reason: None,
            bytes: None,
        }
    }
}
```

Remove the now-unused `use message_ir::IrAttachment;`-adjacent imports only if the compiler flags them.

- [ ] **Step 7: Verify behavior is byte-identical**

Run: `cargo test -p message-ir -p message-csv -p mail -p message-ir-format`
Expected: all pass, including the CSV/EML round-trip tests in ir-format — these pin the serialized `attachments_json` and `X-ME-Attachment-Meta` shapes.
Run: `cargo test -p imazing-exporter -p imessage-ir-exporter`
Expected: all pass — the exporters' tests pin their CSV/EML output.

- [ ] **Step 8: Commit**

```bash
git add crates/libs/ir/src/lib.rs crates/libs/csv/src/lib.rs crates/libs/csv/Cargo.toml crates/libs/mail/src/lib.rs crates/libs/mail/src/parse.rs crates/libs/mail/Cargo.toml crates/libs/ir-format/src/write.rs crates/libs/ir-format/src/read_csv.rs crates/libs/ir-format/src/read_mail.rs crates/exporters/imazing-exporter/src/attachments.rs crates/exporters/imessage-ir-exporter/src/emit.rs
git commit -m "refactor(libs): share AttachmentMeta across ir, csv, and mail layers"
```

---

### Task 3: csv error types → anyhow, plus docs and gate

Findings 10 (low — String error types at the crate boundary) and the `message-csv` part of finding 8.

**Files:**
- Modify: `crates/libs/csv/src/utc_offset.rs`, `crates/libs/csv/src/date_range.rs`, `crates/libs/csv/src/lib.rs`, `crates/libs/csv/Cargo.toml`, `crates/core/message-vault-io-core/src/pipeline.rs` (test adaptations only — the wrappers keep their signatures)

**Interfaces:**
- Consumes: nothing new.
- Produces: `message_csv::parse_utc_offset` and `DateRange::parse` / `parse_in_offset` / `parse_optional_tz` returning `anyhow::Result`; a gated `message-csv`.
- Constraint: every error message text stays byte-identical — only the error type changes. `message-vault-io-core`'s `parse_date_range` / `parse_date_range_tz` keep their public `Result<DateRange, String>` signatures (their `map_err(|e| format!("invalid date range: {e}"))` compiles unchanged because `anyhow::Error` implements `Display`).

- [ ] **Step 1: Add the dependency**

In `crates/libs/csv/Cargo.toml` `[dependencies]`, add:

```toml
anyhow = "1.0.103"
```

- [ ] **Step 2: Switch utc_offset.rs to anyhow**

In `crates/libs/csv/src/utc_offset.rs`:
- Change `use chrono::FixedOffset;` to also import `use anyhow::{Result, bail};`
- Signature: `pub fn parse_utc_offset(raw: &str) -> Result<FixedOffset>` (anyhow `Result`).
- Replace `Err("empty UTC offset".into())` with `bail!("empty UTC offset")`; `ok_or_else(|| "invalid UTC offset".into())` with `ok_or_else(|| anyhow::anyhow!("invalid UTC offset"))`; every `Err(format!(...))` / `return Err(format!(...))` with `bail!(...)` carrying the **identical message text**.
- `fn parse_hh_mm(body: &str) -> Result<(i32, i32)>` — same conversion.

- [ ] **Step 3: Switch date_range.rs to anyhow**

In `crates/libs/csv/src/date_range.rs`:
- `use anyhow::{Result, bail};`
- Signatures: `pub fn parse(...) -> Result<Self>`, `pub fn parse_in_offset(...) -> Result<Self>`, `pub fn parse_optional_tz(...) -> Result<Self>`.
- `parse_with`'s `midnight_secs: impl Fn(NaiveDate) -> Result<i64>`; the two closures change `ok_or_else(|| format!("ambiguous or invalid local midnight for {date}"))` → `ok_or_else(|| anyhow::anyhow!("ambiguous or invalid local midnight for {date}"))` (identical text).
- The start-before-end check: `return Err("start-date must be before end-date (end is exclusive)".into());` → `bail!("start-date must be before end-date (end is exclusive)");`
- `fn parse_ymd(value: &str) -> Result<NaiveDate>` with `bail!("invalid date '{value}' (expected YYYY-MM-DD)")`.

- [ ] **Step 4: Adapt the two tests that call `.contains` on the error**

Run: `cargo test -p message-csv`
Expected: compile errors at `date_range.rs` tests `start_must_precede_end` (line 158) — `err.contains(...)` on an `anyhow::Error` has no `contains` method (the RED: the error type changed, the text must not). Fix: `err.to_string().contains("before end-date")`. All other csv tests use `.is_err()` and pass unchanged.

- [ ] **Step 5: Document the three bare DateRange methods and add the gate**

In `crates/libs/csv/src/date_range.rs`:
- `is_unbounded` (line 92): `/// True when neither bound is set.`
- `contains_secs` (line 96): `/// True when \`secs\` falls inside \`[start, end)\`.`
- `contains_secs_f64` (line 110): `/// Like \`contains_secs\`, flooring a float timestamp first; non-finite\n/// values are outside the range.`

In `crates/libs/csv/src/lib.rs`, after the `//!` intro (line 1), insert:

```rust
#![warn(missing_docs)]
```

- [ ] **Step 6: Verify**

Run: `cargo doc --no-deps -p message-csv 2>&1 | grep -E "warning|error"` — expect zero lines.
Run: `cargo test -p message-csv -p message-vault-io-core` — all pass, output pristine. The core wrappers compile unchanged; their doc text ("Returns an error string when a date cannot be parsed") is still accurate because they still return `String`.
Run: `cargo clippy -p message-csv -p message-vault-io-core -- -D warnings` and `cargo fmt --check` — clean.

- [ ] **Step 7: Commit**

```bash
git add crates/libs/csv/src/utc_offset.rs crates/libs/csv/src/date_range.rs crates/libs/csv/src/lib.rs crates/libs/csv/Cargo.toml crates/core/message-vault-io-core/src/pipeline.rs
git commit -m "refactor(csv): anyhow error types, docs, and missing_docs gate"
```

---

### Task 4: sbr documentation, filename_attr rename, and gate

Findings 2 (medium — re-exported sbr reader types undocumented), 7 (low — `fn_attr`), and the `sbr` part of finding 8.

**Files:**
- Modify: `crates/libs/sbr/src/read.rs`, `crates/libs/sbr/src/lib.rs`

**Interfaces:**
- Produces: a documented, gated `sbr` with `MmsPart.filename_attr` (renamed from `fn_attr`; no consumers outside `read.rs` — verified).
- Consumes: nothing new.

- [ ] **Step 1: Rename fn_attr → filename_attr and document MmsPart**

In `crates/libs/sbr/src/read.rs`, rename the field (lines 35, 129, 180, 245, 289 — the compiler finds any stragglers) and add docs:

```rust
/// Raw `<part>` element: content-type, name, location, and payload columns
/// plus the full attribute map.
#[derive(Debug, Clone, Default)]
pub struct MmsPart {
    /// MIME type from the `ct` attribute.
    pub ct: String,
    /// Content name from the `name` attribute.
    pub name: String,
    /// Content-Location from the `cl` attribute.
    pub cl: String,
    /// Filename from the XML `fn` attribute (not a function attribute).
    pub filename_attr: String,
    /// Text body (SMIL) when present.
    pub text: String,
    /// Base64 payload when present.
    pub data: String,
    /// All raw attributes.
    pub attrs: BTreeMap<String, String>,
}
```

(Replace the existing struct block, lines 30-39.)

- [ ] **Step 2: Document the remaining read.rs pub items**

Add these doc comments in `crates/libs/sbr/src/read.rs` at the named items:

- `ConversationKind` (line 24): `/// Individual or group conversation classification.` Variants: `Individual` `/// One-to-one conversation (default).`, `Group` `/// Group conversation with multiple participants.`
- `AttachmentBlob` (line 49): `/// Decoded MMS attachment with a content-addressed filename.` Fields: `filename` `/// Content-addressed filename (\`<sha256><ext>\`).`, `original_name` `/// Original part name from the XML, when present.`, `mime_type` `/// MIME type from the part's \`ct\`.`, `data` `/// Decoded payload bytes shared by reference.`, `digest_hex` `/// Lowercase hex SHA-256 of the payload.`
- `SourceFields` (line 59): `/// Serde-tagged raw source bag (\`kind: sms|mms\`) preserved for write-back.` Variant fields: `Sms.attrs` `/// Raw SMS attributes.`, `Mms.attrs` `/// Raw MMS attributes.`, `Mms.parts` `/// Raw \`<part>\` attribute maps.`, `Mms.addrs` `/// Raw \`<addr>\` attribute maps.`
- `Record` (line 71): `/// One parsed SMS/MMS message record.` Fields (each with the given text): `chat_key` `/// Conversation key (single peer number or group key).`, `conversation_kind` `/// Individual or group classification.`, `group_title` `/// Generated group title, if group.`, `participant_digits` `/// (Sanitized digits, display-name hint) pairs for participants.`, `timestamp_secs` `/// Message timestamp in seconds.`, `is_from_me` `/// Whether the message is outgoing.`, `sender_digits` `/// Sender digits for incoming messages.`, `sender_display_name` `/// Sender display-name hint, when present.`, `text` `/// Message body text (HTML-entity decoded).`, `subject` `/// Message subject, if any.`, `attachments` `/// Decoded attachment blobs.`, `message_kind` `/// \`"sms"\` or \`"mms"\`.`, `date_ms` `/// Raw \`date\` attribute in milliseconds.`, `contact_name` `/// Raw \`contact_name\` attribute (may be \`"null"\`).`, `android_type` `/// Raw \`type\` (SMS) or \`msg_box\` (MMS) attribute string.`, `source_fields` `/// Serde-tagged raw source bag for write-back.`
- `ParseStats` (line 91): `/// Counters for seen and skipped messages.` Fields: `sms_seen` `/// Number of \`<sms>\` elements encountered.`, `mms_seen` `/// Number of \`<mms>\` elements encountered.`, `skipped_invalid_date` `/// Records dropped for an unparseable \`date\`.`, `skipped_unknown_address` `/// Records dropped because no usable phone address.`, `skipped_unknown_type` `/// SMS records dropped for an unknown \`type\`.`, `skipped_draft_or_outbox` `/// Records dropped as draft/outbox/failed/queued.`, `skipped_empty_participants` `/// MMS records dropped with no participants.`, `skipped_bad_attachment` `/// Parts with undecodable base64 \`data\`.`
- `parse_file` (line 588): add to its doc, after the memory note: `\n///\n/// # Errors\n///\n/// Returns an error when the file cannot be opened or the XML cannot be parsed.`
- `infer_owner_phones` (line 653): add: `\n///\n/// # Errors\n///\n/// Returns an error when the file cannot be opened or parsed.`

- [ ] **Step 3: Document the sbr lib.rs writer surface**

In `crates/libs/sbr/src/lib.rs`, add docs at the named items (read the file for exact current shape; the item names are exact):
- `SbrMessage::Sms` variant + `attrs` field: `/// One \`<sms>\` element carrying a raw attribute map.` / `/// Raw XML attributes for the \`<sms>\` element.`
- `SbrMessage::Mms` variant + fields: `/// One \`<mms>\` element carrying attrs, parts, and addrs.` / `attrs` `/// Raw XML attributes for the \`<mms>\` element.` / `parts` `/// Raw \`<part>\` attribute maps.` / `addrs` `/// Raw \`<addr>\` attribute maps.`
- `SbrMessage::sms` / `SbrMessage::mms` constructors: `/// Wrap a raw attribute map as an SMS element.` / `/// Wrap attrs/parts/addrs as an MMS element.`
- `SbrBackupWriter::create` — add `# Errors` note (directory creation, stale body removal, or file open failures).
- `SbrBackupWriter::count` (line 84): `/// Number of messages written so far.`
- `SbrBackupWriter::write_message` (line 88): `/// Serialize one SMS/MMS element into the sidecar body file and increment\n/// the count.` plus `# Errors` (body write failures).
- `SbrBackupWriter::finish` — add `# Errors` note (flush/read/write/rename failures).
- `default_backup_path` (line 207): `/// Join \`smses.xml\` onto an output directory (the default full-backup filename).`

- [ ] **Step 4: Add the gate and verify**

Insert `#![warn(missing_docs)]` in `crates/libs/sbr/src/lib.rs` after its `//!` intro.
Run: `cargo doc --no-deps -p sbr 2>&1 | grep -E "warning|error"` — expect zero lines; fix any named item per the style guide.
Run: `cargo test -p sbr` — all pass, output pristine.

- [ ] **Step 5: Commit**

```bash
git add crates/libs/sbr/src/read.rs crates/libs/sbr/src/lib.rs
git commit -m "docs(sbr): document reader and writer surfaces, rename fn_attr"
```

---

### Task 5: media documentation and gate

Findings 3 (medium — re-exported ffmpeg probe API undocumented) and the `media` part of finding 8.

**Files:**
- Modify: `crates/libs/media/src/lib.rs`, `crates/libs/media/src/process.rs`, `crates/libs/media/src/tools.rs`

**Interfaces:**
- Produces: a documented, gated `media`.
- Consumes: nothing new.

- [ ] **Step 1: Add the gate**

Insert `#![warn(missing_docs)]` in `crates/libs/media/src/lib.rs` after its `//!` intro.

- [ ] **Step 2: Document the gaps**

In `crates/libs/media/src/lib.rs`:
- `MediaMode::Clone` (line 29): `/// Copy attachment files through unchanged; the default (a no-op after export).`
- `MediaMode::Convert` (line 31): `/// Rewrite images to \`.jpg\`, videos to \`.mp4\`, audio to \`.mp3\`.`
- `MediaMode::Compress` (line 32): `/// Re-encode attachments to shrink them per \`CompressOptions\`.`
- `MediaMode::as_str` (line 35): `/// Canonical lowercase CLI string (\`disabled\` / \`clone\` / \`convert\` / \`compress\`).`
- `MediaMode::parse` (line 44): `/// Parse a CLI string (case- and whitespace-insensitive); \`None\` for unknown input.`
- `MediaMode::needs_tools` (line 54): `/// True when the mode requires ffmpeg/ffprobe (Convert or Compress).`
- `MaxResolution::P720` (line 83): `/// Cap the video long edge at 1280 px.`
- `MaxResolution::P1080` (line 85): `/// Cap the video long edge at 1920 px; the default.`
- `MaxResolution::P4k` (line 86): `/// Cap the video long edge at 3840 px.`
- `MaxResolution::as_str` (line 90): `/// Canonical string (\`720p\` / \`1080p\` / \`4k\`).`
- `MaxResolution::max_long_edge` (line 98): `/// Pixel length of the long-edge cap.`
- `MaxResolution::parse` (line 106): `/// Parse \`720p\`/\`1080p\`/\`4k\` (or bare numbers); \`None\` for unknown input.`
- `compress_options_from_cli` (line 133): add to its doc: `\n///\n/// # Errors\n///\n/// Returns an error when \`min_size\` is not a parseable size (like \`20M\`).`
- `CompressOptions` fields (lines 150-153): `max_resolution` `/// Long-edge cap applied when compressing video.`, `max_fps` `/// Target frame rate for video compression.`, `min_size_bytes` `/// Videos smaller than this are not compressed.`, `skip_efficient` `/// Skip already-efficient (HEVC, low bitrate) videos.`

In `crates/libs/media/src/process.rs`:
- `MediaReport` (line 11): `/// Aggregate counts and errors from one media convert/compress pass.` Fields: `processed` `/// Number of files converted or compressed.`, `skipped` `/// Number of files left unchanged.` (`bytes_before` / `bytes_after` already documented.)
- `errors` (line 18): `/// Per-file error messages (\`path: error\`) from the pass.`
- `process_attachments_dir` and `process_attachments_dir_with_log` (lines 28, 37): add to each doc: `\n///\n/// # Errors\n///\n/// Returns an error when ffmpeg/ffprobe are missing or fail, an input path\n/// escapes the output directory, or IO fails.`

In `crates/libs/media/src/tools.rs`:
- `FfmpegToolsProbe` (line 63): `/// Result of locating ffmpeg and ffprobe (the GUI's probe result type).` Fields: `ok` `/// Whether both tools were found and pass \`-version\`.`, `ffmpeg_path` `/// Resolved ffmpeg path, if found.`, `ffprobe_path` `/// Resolved ffprobe path, if found.`, `error` `/// Human-readable list of missing tools when \`ok\` is false.`
- `ffmpeg_available` (line 70): `/// True when both ffmpeg and ffprobe resolve from the search path.`
- `probe_ffmpeg_tools` (line 141): `/// Probe both tools in an explicit directory, or fall back to the default\n/// resolution path (tools-dir override, beside the executable, \`MESSAGE_VAULT_IO_BIN\`, PATH).`

- [ ] **Step 3: Verify**

Run: `cargo doc --no-deps -p media 2>&1 | grep -E "warning|error"` — expect zero lines; fix any remaining named item.
Run: `cargo test -p media` — all pass, output pristine.

- [ ] **Step 4: Commit**

```bash
git add crates/libs/media/src/lib.rs crates/libs/media/src/process.rs crates/libs/media/src/tools.rs
git commit -m "docs(media): document modes, resolution, probe API, and reports"
```

---

### Task 6: contacts documentation and gate

Findings 4 (medium — ValidateMode/ValidateReport/VcfCard lack docs) and the `contacts` part of finding 8.

**Files:**
- Modify: `crates/libs/contacts/src/book.rs`, `crates/libs/contacts/src/mapping.rs`, `crates/libs/contacts/src/validate.rs`, `crates/libs/contacts/src/vcard_csv.rs`, `crates/libs/contacts/src/vcf.rs`, `crates/libs/contacts/src/lib.rs`

**Interfaces:**
- Produces: a documented, gated `contacts`.
- Consumes: nothing new.

- [ ] **Step 1: Add the gate**

Insert `#![warn(missing_docs)]` in `crates/libs/contacts/src/lib.rs` after its `//!` intro.

- [ ] **Step 2: Document the gaps**

In `crates/libs/contacts/src/validate.rs`:
- `ValidateMode` (line 13): `/// Check-only or write-updates mode for the contacts-validate tool.`
- `ValidateReport` (line 22): `/// Full result of a validate run.` Fields: `rewritten` `/// Count of phones rewritten to a certain E.164.`, `uncertain` `/// Count of phones left unchanged as uncertain.`, `duplicate_groups` `/// Count of E.164 values shared by more than one contact.`
- `ContactsFormat` (line 55): `/// Recognized contacts input formats.` Variants: `Vcf` `/// vCard \`.vcf\`/\`.vcard\` input.`, `VcardCsv` `/// First/Last Name plus phone-column CSV input.`
- `ContactsInputError` (line 65): `/// Short UI message plus optional log details for probe failures.` Fields: `message` `/// Short human-readable error (e.g. \`"Unrecognized contacts format."\`).`, `details` `/// Optional verbose detail lines for logs.`
- `detect_contacts_format` (line 288): add to its doc: `\n///\n/// # Errors\n///\n/// Returns a \`ContactsInputError\` when the path is missing, the extension is\n/// unknown, or the content is not a recognized contacts format.`

In `crates/libs/contacts/src/vcf.rs`:
- `VcfCard` (line 8): `/// One parsed vCard.` Fields: `fn_raw` `/// Unescaped \`FN\` value.`, `n_family` `/// \`N\` family (last) name component.`, `n_given` `/// \`N\` given (first) name component.`, `n_middle` `/// \`N\` middle name component.`, `phones` `/// Deduplicated raw \`TEL\` values.`, `email` `/// First \`EMAIL\` value, if any.`
- `parse_vcf` (line 20): add: `\n///\n/// # Errors\n///\n/// Returns an error when the file cannot be read.`
- `parse_vcf_str` (line 27): add: `\n///\n/// # Errors\n///\n/// The \`Result\` is for a stable API; parsing text currently always returns \`Ok\`.`

In `crates/libs/contacts/src/book.rs`:
- `ContactsBook::empty` (line 22): `/// Construct an empty index.`
- `ContactsBook::len` (line 177): `/// Number of (handle, type) entries indexed.`
- `ContactsBook::is_empty` (line 181): `/// Whether the book has no entries.`
- Add `# Errors` sections to `load_contacts_file` (line 30, format detection or parse failures), `load_vcf` (line 46, read/parse failures), `load_vcard_csv` (line 83, read/parse failures), `resolve_contacts_cli` (line 203, load failure when both flags are passed).

In `crates/libs/contacts/src/mapping.rs`:
- `NameMapping::empty` (line 20): `/// Construct an empty mapping.`
- `NameMapping::load` (line 27): add `# Errors` (file open/read or a missing required header).
- `NameMapping::load_optional` (line 100): `/// Load from a path option, returning the path when loaded.` plus `# Errors` as `load`.
- `NameMapping::len` (line 116): `/// Number of incorrect-name entries.`
- `NameMapping::is_empty` (line 120): `/// Whether the mapping has no entries.`

In `crates/libs/contacts/src/vcard_csv.rs`:
- `ContactCsvRow` fields (lines 21-23): `first` `/// First-name cell.`, `middle` `/// Middle-name cell.`, `last` `/// Last-name cell.`
- `VcardCsvColumns` (line 45): `/// Resolved column indexes for a vCard CSV header.` Fields: `first` `/// Index of the first-name column, if present.`, `middle` `/// Index of the middle-name column, if present.`, `last` `/// Index of the last-name column, if present.`, `notes` `/// Index of the notes column, if present.`, `phones` `/// Indexes of phone/fax columns.`
- `read_vcard_csv_rows` (line 97): add `# Errors` (open/parse failures or not a vCard CSV shape).

- [ ] **Step 3: Verify**

Run: `cargo doc --no-deps -p contacts 2>&1 | grep -E "warning|error"` — expect zero lines; fix any remaining named item (the gate is authoritative — items this step missed will be named).
Run: `cargo test -p contacts` — all pass, output pristine.

- [ ] **Step 4: Commit**

```bash
git add crates/libs/contacts/src/book.rs crates/libs/contacts/src/mapping.rs crates/libs/contacts/src/validate.rs crates/libs/contacts/src/vcard_csv.rs crates/libs/contacts/src/vcf.rs crates/libs/contacts/src/lib.rs
git commit -m "docs(contacts): document validate, vcf, book, and mapping surfaces"
```

---

### Task 7: ir-format unsafe-attachment-path const, server asserts, docs, and gate

Findings 9 (medium — cross-crate string contract) and the `ir-format` part of finding 8.

**Files:**
- Modify: `crates/libs/ir-format/src/util.rs`, `crates/libs/ir-format/src/lib.rs`, `crates/libs/ir-format/src/export_transforms.rs`, `crates/libs/ir-format/src/format_sink.rs`, `crates/libs/ir-format/src/read_sbr.rs`, `crates/libs/ir-format/src/clean.rs`, `crates/libs/ir-format/src/write.rs`, `crates/vault/server/src/import/mod.rs`

**Interfaces:**
- Produces: `message_ir_format::UNSAFE_ATTACHMENT_PATH_PREFIX` (a `pub const &str`, exact text `unsafe attachment path (contains ..)`); a gated `message-ir-format`.
- Consumes: the server test asserts (Task 7 changes them to import the const).

- [ ] **Step 1: Add the const and use it in the bail**

In `crates/libs/ir-format/src/util.rs`, add above `read_attachment_file` (line 68):

```rust
/// Shared message prefix for unsafe-attachment-path errors.
///
/// The ir-format path check and the server's `safe_rel_path` both format
/// their bail from this const, and the server's import tests match it —
/// keep the exact text stable.
pub const UNSAFE_ATTACHMENT_PATH_PREFIX: &str = "unsafe attachment path";
```

Change the bail at line 82 from:

```rust
anyhow::bail!("unsafe attachment path (contains ..): {rel}");
```

to:

```rust
anyhow::bail!("{UNSAFE_ATTACHMENT_PATH_PREFIX} (contains ..): {rel}");
```

In `crates/libs/ir-format/src/lib.rs`, add to the `pub use` block:

```rust
pub use util::UNSAFE_ATTACHMENT_PATH_PREFIX;
```

- [ ] **Step 2: Point the server's two asserts at the const, and format the server's own bail from it**

In `crates/vault/server/src/import/mod.rs` lines 2105 and 2166, replace the hardcoded text:

```rust
err.to_string().contains("unsafe attachment path")
```

with:

```rust
err.to_string().contains(message_ir_format::UNSAFE_ATTACHMENT_PATH_PREFIX)
```

(Use the fully qualified path — no new imports.)

In `crates/vault/server/src/config.rs`, change the `safe_rel_path` bail from
`bail!("unsafe attachment path: {name}")` to
`bail!("{UNSAFE_ATTACHMENT_PATH_PREFIX}: {name}")` (with a
`use message_ir_format::UNSAFE_ATTACHMENT_PATH_PREFIX;`) — the emitted text
is byte-identical. The server gains a `message-ir-format` dependency to
consume the const. Both bails and both asserts now reference one const, so
the contract is compile-time and every emitted text is unchanged.

- [ ] **Step 3: Add the gate and document the ir-format gaps**

Insert `#![warn(missing_docs)]` in `crates/libs/ir-format/src/lib.rs` after its `//!` intro.

In `crates/libs/ir-format/src/export_transforms.rs`:
- `ExportTransforms.media` (line 21): `/// Media mode applied at finish (clone/convert/compress/disabled).`
- `ExportTransforms.compress` (line 22): `/// Video/audio compression options used with Compress mode.`
- `ExportTransforms.obfuscate` (line 23): `/// Whether to replace PII and media with obfuscated placeholders.`
- `ExportTransforms.obfuscate_seed` (line 24): `/// Seed for deterministic obfuscation; \`None\` generates one.`
- `from_configs` (line 42): `/// Build transforms from a \`MediaConfig\` and \`ObfuscateConfig\`\n/// (obfuscation is enabled when either obfuscation flag or the seed is set).`
- `none` (line 52): `/// All-defaults transform set (clone, no obfuscation, no log).`
- `needs_media_tools` (line 56): `/// True when ffmpeg/ffprobe will be required (false when obfuscating,\n/// which replaces media with placeholders).`
- `copies_attachments` (line 61): `/// True when attachment bytes should be staged under \`attachments/\`\n/// (false when obfuscating).`

In `crates/libs/ir-format/src/format_sink.rs`:
- `FormatSinkResult.xml_path` (line 17): `/// Path of the written \`smses.xml\` when the format is XML.`
- `FormatSinkResult.media` (line 18): `/// Media pass report from the finish step.`
- `FormatSinkResult.obfuscated_docs` (line 19): `/// Number of documents obfuscated.`

In `crates/libs/ir-format/src/read_sbr.rs`:
- `SbrReadReport` fields (lines 25-38): `conversations` `/// Number of conversation documents produced.`, `sms_seen` `/// SMS elements parsed.`, `mms_seen` `/// MMS elements parsed.`, `attachments_saved` `/// Attachment files staged under \`attachments/\`.`, `sent` `/// Outgoing messages in produced documents.`, `received` `/// Incoming messages in produced documents.`, `skipped_invalid_date` `/// Messages dropped for an invalid date.`, `skipped_out_of_range` `/// Messages dropped outside the configured date range.`, `skipped_unknown_address` `/// Messages dropped with no usable address.`, `skipped_unknown_type` `/// SMS dropped for an unknown \`type\`.`, `skipped_draft_or_outbox` `/// Draft/outbox/failed/queued messages dropped.`, `skipped_empty_participants` `/// MMS dropped with no participants.`, `skipped_bad_attachment` `/// Parts with undecodable base64.`, `errors` `/// Per-file error messages from parsing/staging.`
- `SbrReadOptions` fields (lines 43-48): `owner_phones` `/// Known owner phone numbers (empty triggers inference).`, `date_range` `/// Date window messages must fall inside.`, `attachments_dir` `/// Directory staged attachments are written to.`, `copy_attachments` `/// Whether to write staged attachment files.`, `keep_attachment_bytes` `/// Whether to retain decoded bytes in memory on the records.`, `cancel` `/// Cancellation flag checked between files.`

In `crates/libs/ir-format/src/clean.rs`:
- `write_export_sentinel` (line 15): add: `\n///\n/// # Errors\n///\n/// Returns an error when the sentinel cannot be written.`

In `crates/libs/ir-format/src/write.rs`:
- `document_to_mail_messages` (line 394): add to its doc: `\n///\n/// # Errors\n///\n/// Returns an error when an attachment file cannot be read from disk.`

- [ ] **Step 4: Verify**

Run: `cargo doc --no-deps -p message-ir-format 2>&1 | grep -E "warning|error"` — expect zero lines; fix any remaining named item.
Run: `cargo test -p message-ir-format` — all pass.
Run: `cargo test -p message-vault-server import` — the server import tests (including the two `contains` asserts) pass unchanged — the const's text is identical to the old literal, so behavior is byte-identical.

- [ ] **Step 5: Commit**

```bash
git add crates/libs/ir-format/src/util.rs crates/libs/ir-format/src/lib.rs crates/libs/ir-format/src/export_transforms.rs crates/libs/ir-format/src/format_sink.rs crates/libs/ir-format/src/read_sbr.rs crates/libs/ir-format/src/clean.rs crates/libs/ir-format/src/write.rs crates/vault/server/src/import/mod.rs
git commit -m "refactor(ir-format): const unsafe-attachment-path contract, docs, and gate"
```

---

### Task 8: Shared test fixture behind a testutil feature

Finding 12 (low — ConversationDocument test fixtures duplicated across crates).

**Files:**
- Create: `crates/libs/ir/src/testutil.rs`
- Modify: `crates/libs/ir/src/lib.rs`, `crates/libs/ir/Cargo.toml`, `crates/libs/ir-format/Cargo.toml`, `crates/libs/ir-format/src/format_sink.rs`, `crates/libs/ir-format/src/lib_tests.rs`, `crates/libs/reexport/Cargo.toml`, `crates/libs/reexport/src/lib.rs`

**Interfaces:**
- Produces: `message_ir::testutil::sample_document(text: &str) -> ConversationDocument` behind the `testutil` feature of `message-ir` (enabled via dev-dependencies by `message-ir-format` and `message-reexport`).
- Consumes: nothing new.
- The builder is the exact shape of the current `lib_tests.rs` fixture (one incoming SMS from `+15555550101` with a `source` bag), with the message text as a parameter — the richest common shape of the three copies, so `format_sink`'s and `reexport`'s tests keep passing unchanged.

- [ ] **Step 1: Create the testutil module**

Create `crates/libs/ir/src/testutil.rs`:

```rust
//! Shared test fixture for crate tests (behind the `testutil` feature).

use crate::{
    ConversationDocument, ConversationMeta, ConversationStats, ExportMeta, IrConversationType,
    IrDirection, IrMessage, IrMessageKind, IrParticipant, IrService, IrSource, SCHEMA_VERSION,
};

/// One-message conversation fixture: an incoming SMS from `+15555550101`.
///
/// `text` becomes the message body. Stats are computed on return.
pub fn sample_document(text: &str) -> ConversationDocument {
    let mut doc = ConversationDocument {
        schema_version: SCHEMA_VERSION,
        export: ExportMeta {
            source: "sms-backup-restore".into(),
            tool: "SMS Backup & Restore".into(),
            tool_version: "10.26.003".into(),
            owner_handle: Some("+15555550100".into()),
            owner_display_name: Some("Me".into()),
        },
        conversation: ConversationMeta {
            chat_identifier: "+15555550101".into(),
            conversation_type: IrConversationType::Individual,
            group_title: None,
            participants: vec![IrParticipant {
                handle: "+15555550101".into(),
                display_name: Some("Sam".into()),
                handle_type: Some(crate::HandleType::Phone),
            }],
            stats: ConversationStats::default(),
        },
        messages: vec![IrMessage {
            guid: "aabbccddeeff00112233445566778899".into(),
            timestamp_unix_ms: 1_400_773_261_000,
            direction: IrDirection::Incoming,
            service: IrService::Sms,
            message_kind: IrMessageKind::Sms,
            sender_handle: Some("+15555550101".into()),
            sender_display_name: Some("Sam".into()),
            subject: None,
            text: text.into(),
            attachments: vec![],
            imessage: None,
            source: Some(IrSource {
                android_type: Some(1),
                fields: {
                    let mut m = serde_json::Map::new();
                    m.insert("address".into(), serde_json::json!("+15555550101"));
                    m
                },
            }),
        }],
        packaging_stem_suffix: None,
    };
    doc.finalize_stats();
    doc
}
```

In `crates/libs/ir/src/lib.rs`, add after the `use` block:

```rust
#[cfg(feature = "testutil")]
pub mod testutil;
```

In `crates/libs/ir/Cargo.toml`, add:

```toml
[features]
testutil = []
```

- [ ] **Step 2: Wire the dev-dependency features**

In `crates/libs/ir-format/Cargo.toml` `[dev-dependencies]`, add:

```toml
message-ir = { path = "../ir", features = ["testutil"] }
```

In `crates/libs/reexport/Cargo.toml` `[dev-dependencies]`, add the same line:

```toml
message-ir = { path = "../ir", features = ["testutil"] }
```

- [ ] **Step 3: Replace the three local fixtures**

1. `crates/libs/ir-format/src/lib_tests.rs`: delete `fn sample_doc()` (lines 14-~75 — through its closing brace) and replace every call `sample_doc()` with `message_ir::testutil::sample_document("hello ir")` (the current text is `"hello ir"`). Remove the now-unused `use` items the compiler flags (e.g. `serde_json::{Map, Value, json}` may become unused).
2. `crates/libs/ir-format/src/format_sink.rs`: delete `fn tiny_doc(text: &str)` (lines 195-234) and replace every call `tiny_doc(...)` with `message_ir::testutil::sample_document(...)` keeping the same argument. Remove unused imports the compiler flags.
3. `crates/libs/reexport/src/lib.rs`: delete `fn sample_doc()` (lines 398-437) and replace every call `sample_doc()` with `message_ir::testutil::sample_document("hello reexport")` (the current text is `"hello reexport"`). Remove unused imports the compiler flags.

- [ ] **Step 4: Verify**

Run: `cargo test -p message-ir-format -p message-reexport -p message-ir`
Expected: all pass — the fixture shape is the same document the tests already round-trip (the `source` bag and participant shape are byte-identical to the old `lib_tests` fixture, and the other two fixtures' tests make no assertions that differ).
Run: `cargo doc --no-deps -p message-ir --features testutil 2>&1 | grep -E "warning|error"` — expect zero lines (the gate covers the new module).

- [ ] **Step 5: Commit**

```bash
git add crates/libs/ir/src/testutil.rs crates/libs/ir/src/lib.rs crates/libs/ir/Cargo.toml crates/libs/ir-format/Cargo.toml crates/libs/ir-format/src/format_sink.rs crates/libs/ir-format/src/lib_tests.rs crates/libs/reexport/Cargo.toml crates/libs/reexport/src/lib.rs
git commit -m "refactor(libs): share one sample_document test fixture behind testutil"
```

---

### Task 9: Split mms_enc.rs decoders into a private module, document pdu.rs, and gate

Findings 13 (low — mms_enc.rs is a 2021-line monolith) and the `go-sms-mms` part of finding 8.

**Files:**
- Create: `crates/libs/go-sms-mms/src/decoders.rs`
- Modify: `crates/libs/go-sms-mms/src/lib.rs`, `crates/libs/go-sms-mms/src/mms_enc.rs`, `crates/libs/go-sms-mms/src/pdu.rs`

**Interfaces:**
- Consumes: nothing new.
- Produces: a private `decoders` module holding the WAP-209/WAP-230 unit decoders; `mms_enc.rs` keeps the PDU assembly, the fragment scanners, and the `pub(crate)` surface that `pdu.rs` imports (`decode_mms_best_effort`, `extension_for_content_type`, `normalize_content_id`, `scan_mms_addresses`) with unchanged signatures.
- Constraint: crate-internal move only — no public API change, no behavior change (the extensive PDU decode tests in `mms_enc.rs` and `pdu.rs` are the behavior pin).

- [ ] **Step 1: Create decoders.rs and move the unit decoders**

Create `crates/libs/go-sms-mms/src/decoders.rs`:

```rust
//! WAP-209 / WAP-230 unit decoders moved out of `mms_enc`.

use crate::mms_enc::*;
use std::collections::HashMap;
```

Move these items **verbatim** from `mms_enc.rs` into `decoders.rs`, changing `fn` to `pub(crate) fn` (and `impl` blocks' fns to `pub(crate) fn`) so `mms_enc.rs` can keep calling them:

- `Cursor` and its impl fns (`new`, `remaining`, `peek`, `next_byte`, `take`) — lines 355-381
- `is_mms_short_integer_field` (281), `yes_no_token` (311), `priority_token` (319)
- `decode_uint_var` (383), `decode_value_length` (395), `decode_text_string` (408), `decode_short_integer` (421), `decode_long_integer` (430), `decode_integer_value` (444), `trim_encoded_string_junk` (451), `decode_encoded_string_value` (481), `decode_delta_seconds` (556), `decode_expiry_or_delivery_time` (561), `decode_mms_version` (583), `decode_message_class_value` (590), `decode_status_value` (606), `decode_response_status_value` (621), `decode_sender_visibility_value` (637), `decode_from_value` (655), `decode_date_value` (684), `decode_message_type_value` (688), `well_known_content_type` (707), `decode_constrained_media` (711), `decode_wsp_text_param` (720), `decode_wsp_typed_param` (726), `decode_wsp_parameters` (747), `decode_content_type_value` (773), `decode_content_disposition_value` (806), `decode_application_header_value` (831), `skip_unknown_mms_value` (857), `decode_mms_header_field` (876), `decode_multipart_body` (1036), `apply_mms_header_field` (1400)

Keep **in place** in `mms_enc.rs` (do not move): the module `//!` intro, `StructuredMms` / `MmsPart` / `NamedPart`, all `MMS_*` / `WSP_*` / `CHARSET_*` / `WELL_KNOWN_CONTENT_TYPES` consts, `merge_opt`, `merge_from`, `part_dedupe_key`, `merge_parts_into`, `scan_message_type_starts`, `is_printable_name_byte`, `looks_like_part_name`, `try_parse_cloc_name_at`, `find_next_cloc_name`, `is_text_part_name`, `text_part_payload_end`, `merge_named_parts`, `scan_mms_addresses`, `decode_mms_best_effort`, `extension_for_content_type`, `normalize_content_id`, and the entire `#[cfg(test)]` module.

- [ ] **Step 2: Wire the module and visibility**

In `crates/libs/go-sms-mms/src/lib.rs`, add `mod decoders;` next to `mod mms_enc;`.

In `crates/libs/go-sms-mms/src/mms_enc.rs`, add near the top:

```rust
use crate::decoders::*;
```

Change the consts `decoders.rs` uses from `const` to `pub(crate) const` (the `MMS_*`, `WSP_*`, `CHARSET_*`, and `WELL_KNOWN_CONTENT_TYPES` blocks — all of them, so the glob import works).

The compiler is authoritative for the residual fixups: if a kept fn in `mms_enc.rs` calls a moved helper, or a moved fn uses a const still private, adjust visibility (`pub(crate)`) — never change signatures or bodies.

- [ ] **Step 3: Document pdu.rs and add the gate**

In `crates/libs/go-sms-mms/src/pdu.rs`:
- `ParsedAttachment` (line 52): `/// One attachment decoded from a PDU.` Fields: `ext` `/// File extension including the leading dot (e.g. \`.jpg\`).`, `data` `/// Decoded attachment bytes.`, `smil_name` `/// SMIL \`src\` reference the part binds to, when matched.`
- `ParsedPdu` (line 65): `/// One decoded PDU message.` Fields: `path` `/// Source \`.pdu\` file path.`, `timestamp` `/// Message time in Unix seconds (structured Date header, else filename).`, `participants` `/// Deduplicated, sanitized participant numbers.`, `body` `/// Decoded message text (possibly emoji-decoded).`, `attachments` `/// Decoded attachment list.`, `is_sent` `/// Whether the direction is outgoing (owner was From).`, `is_group` `/// Whether there are at least 3 unique participants.`, `sender_number` `/// Inferred sender digits (owner when outgoing).`

In `crates/libs/go-sms-mms/src/lib.rs`, insert `#![warn(missing_docs)]` after the `//!` intro.

- [ ] **Step 4: Verify**

Run: `cargo doc --no-deps -p go-sms-mms 2>&1 | grep -E "warning|error"` — expect zero lines; fix any remaining named item.
Run: `cargo test -p go-sms-mms` — all pass, output pristine (the ~20 PDU/decode tests pin the decode behavior across the split).

- [ ] **Step 5: Commit**

```bash
git add crates/libs/go-sms-mms/src/decoders.rs crates/libs/go-sms-mms/src/mms_enc.rs crates/libs/go-sms-mms/src/lib.rs crates/libs/go-sms-mms/src/pdu.rs
git commit -m "refactor(go-sms-mms): split unit decoders into a private module"
```

---

### Task 10: mail documentation and gate

The `mail` part of finding 8. (`MailAttachment`'s shape was composed in Task 2; its `meta` and `bytes` fields are documented there. This task documents `is_sticker` / `transcription` / `sticker_effect` on the composed `MailAttachment` plus the rest of the surface.)

**Files:**
- Modify: `crates/libs/mail/src/lib.rs`

**Interfaces:**
- Produces: a documented, gated `mail`.
- Consumes: the composed `MailAttachment` from Task 2.

- [ ] **Step 1: Add the gate**

Insert `#![warn(missing_docs)]` in `crates/libs/mail/src/lib.rs` after its `//!` intro.

- [ ] **Step 2: Document the gaps**

Add doc comments in `crates/libs/mail/src/lib.rs`:
- `Direction::Incoming` (line 35): `/// Incoming message (sender is the peer).`
- `Direction::Outgoing` (line 36): `/// Outgoing message (sender is the owner).`
- `Participant.handle` (line 52): `/// Phone, email, or chat handle; also used for peer matching in From/To mapping.`
- `Participant.display_name` (line 54): `/// Optional display name, omitted from the JSON header when \`None\`.`
- On the composed `MailAttachment` (from Task 2): `is_sticker` `/// Sticker flag serialized in the attachment meta JSON.`, `transcription` `/// OCR/transcription text serialized in the attachment meta JSON.`, `sticker_effect` `/// Sticker effect name serialized in the attachment meta JSON.`
- `SmsMailFields` fields (lines 81-100) — each maps to the same-named `MailMessage` field and the same-named `X-ME-*` header: `chat_identifier` `/// Conversation id; drives the folder/mbox stem and \`X-ME-Chat-Identifier\`.`, `conversation_type` `/// \`individual\` or \`group\`.`, `group_title` `/// Group chat title (folder name / subject label).`, `participants` `/// Roster for \`X-ME-Participants\`.`, `guid` `/// Message guid used in Message-ID and the \`.eml\` filename.`, `timestamp_unix_ms` `/// Message time in ms; feeds the Date header, filenames, and sort order.`, `direction` `/// From/To mapping.`, `service` `/// SMS/iMessage/…; selects the Message-ID domain.`, `message_kind` `/// \`sms\` / \`mms\` / \`imessage\` / \`tapback\` / \`balloon\` / ….`, `sender_handle` `/// Sender's handle (From for incoming).`, `sender_display_name` `/// Sender display name for From/To.`, `owner_handle` `/// Owner E.164/handle for From/To mapping.`, `subject` `/// Goes to \`X-ME-Subject\`, not the mail Subject.`, `text` `/// Message body.`, `android_type` `/// Android message type code → \`X-ME-Android-Type\`.`, `source_fields_json` `/// Opaque source fields → \`X-ME-Source-Fields\`.`, `export_source` `/// Provenance string → \`X-ME-Export-Source\`.`, `export_tool` `/// Tool name → \`X-ME-Export-Tool\`.`, `export_tool_version` `/// Version → \`X-ME-Export-Tool-Version\`.`, `attachments` `/// MIME parts to attach.`
- `MailMessage` fields (lines 108-155) — same header-mapping descriptions: `chat_identifier` `/// Conversation id → \`X-ME-Chat-Identifier\`, folder stem, group chat address local part.`, `group_title` `/// Group title → \`X-ME-Group-Title\`, To display name, subject label.`, `participants` `/// Roster → \`X-ME-Participants\` JSON.`, `guid` `/// Message id for Message-ID and the \`.eml\` filename.`, `timestamp_unix_ms` `/// Unix ms timestamp → Date header, filenames, mbox asctime, sort order.`, `direction` `/// From/To mapping → \`X-ME-Direction\`.`, `service` `/// Selects the Message-ID domain (\`imessage.local\` vs default) → \`X-ME-Service\`.`, `sender_handle` `/// → \`X-ME-Sender-Handle\`; From for incoming.`, `sender_display_name` `/// → \`X-ME-Sender-Display-Name\`.`, `subject` `/// → \`X-ME-Subject\` (the mail Subject is always \`"Message with …"\`).`, `text` `/// Text body.`, `android_type` `/// → \`X-ME-Android-Type\`.`, `source_fields_json` `/// → \`X-ME-Source-Fields\`.`, `export_source` `/// → \`X-ME-Export-Source\`.`, `export_tool` `/// → \`X-ME-Export-Tool\`.`, `export_tool_version` `/// → \`X-ME-Export-Tool-Version\`.`, `attachments` `/// MIME parts plus the \`X-ME-Attachment-Meta\` JSON.`, `is_reply` `/// iMessage; → \`X-ME-Is-Reply\`.`, `in_reply_to_guid` `/// Sets In-Reply-To/References and \`X-ME-Thread-Originator-Guid\`.`, `thread_originator_part` `/// → \`X-ME-Thread-Originator-Part\`.`, `num_replies` `/// → \`X-ME-Num-Replies\`.`, `is_deleted` `/// → \`X-ME-Is-Deleted\`.`, `send_effect` `/// → \`X-ME-Send-Effect\`.`, `shared_location` `/// → \`X-ME-Shared-Location\`.`, `announcement` `/// → \`X-ME-Announcement\`.`, `read_receipt_rfc3339` `/// → \`X-ME-Read-Receipt\`.`, `parts_json` `/// → \`X-ME-Parts\`.`, `edits_json` `/// → \`X-ME-Edits\`.`, `app_json` `/// → \`X-ME-App\`.`, `balloon_bundle_id` `/// → \`X-ME-Balloon-Bundle-Id\`.`, `balloon_kind` `/// → \`X-ME-Balloon-Kind\`.`, `tapbacks_json` `/// → \`X-ME-Tapbacks\`.`, `associated_guid` `/// Tapback target message → \`X-ME-Associated-Guid\`.`, `associated_part` `/// Tapback target part index → \`X-ME-Associated-Part\`.`, `tapback_kind` `/// → \`X-ME-Tapback-Kind\`.`, `tapback_emoji` `/// → \`X-ME-Tapback-Emoji\`.`, `tapback_action` `/// → \`X-ME-Tapback-Action\`.`

(The items already documented — `conversation_type`, `message_kind`, `owner_handle`, `owner_display_name`, `filename_suffix`, `MailMessage::sms`, `clean_previous_mail_output`, `write_mail_package` — are left as-is.)

- [ ] **Step 3: Verify**

Run: `cargo doc --no-deps -p mail 2>&1 | grep -E "warning|error"` — expect zero lines; fix any remaining named item (the gate is authoritative).
Run: `cargo test -p mail` — all pass, output pristine.

- [ ] **Step 4: Commit**

```bash
git add crates/libs/mail/src/lib.rs
git commit -m "docs(mail): document the message and attachment surfaces"
```

---

### Task 11: obfuscate documentation and gate

The `obfuscate` part of finding 8.

**Files:**
- Modify: `crates/libs/obfuscate/src/lib.rs`

**Interfaces:**
- Produces: a documented, gated `obfuscate`.
- Consumes: nothing new.

- [ ] **Step 1: Add the gate**

Insert `#![warn(missing_docs)]` in `crates/libs/obfuscate/src/lib.rs` after its `//!` intro.

- [ ] **Step 2: Document the gaps**

Add doc comments in `crates/libs/obfuscate/src/lib.rs`:
- `MediaClass::Image` (line 104): `/// Image placeholder bucket.`
- `MediaClass::Video` (line 105): `/// Video placeholder bucket.`
- `MediaClass::Other` (line 106): `/// Everything-else placeholder bucket.`
- `Obfuscator::new` (line 127): `/// Build an obfuscator from a 32-byte HMAC key.`
- `Obfuscator::obfuscate_email` (line 237): `/// Map an email to \`first.last@example.invalid\`, keyed case-insensitively.`
- `classify_attachment` (line 470) and `placeholder_rel_path` (line 498) — if the gate flags them (they are `pub`): `/// Map a MIME type and/or file extension to its placeholder bucket.` and `/// Shared placeholder relative path for a \`MediaClass\`\n/// (\`attachments/placeholder.jpg|.mp4|.bin\`).`
- `OBFUSCATE_SEED_HEX_LEN` (line 540): `/// Hex length of a 32-byte seed.`

- [ ] **Step 3: Verify**

Run: `cargo doc --no-deps -p obfuscate 2>&1 | grep -E "warning|error"` — expect zero lines; the gate names anything this step missed — document it per the style guide.
Run: `cargo test -p obfuscate` — all pass, output pristine.

- [ ] **Step 4: Commit**

```bash
git add crates/libs/obfuscate/src/lib.rs
git commit -m "docs(obfuscate): document the remaining public items"
```

---

### Task 12: phone documentation and gate

The `phone` part of finding 8.

**Files:**
- Modify: `crates/libs/phone/src/lib.rs`

**Interfaces:**
- Produces: a documented, gated `phone`.
- Consumes: nothing new.

- [ ] **Step 1: Add the gate**

Insert `#![warn(missing_docs)]` in `crates/libs/phone/src/lib.rs` after its `//!` intro.

- [ ] **Step 2: Document the gaps**

Add doc comments in `crates/libs/phone/src/lib.rs`:
- `PhoneRegion::parse_cli` (line 34): `/// Parse a CLI string (\`usa\`/\`us\`/\`international\`/\`intl\`, case-insensitive);\n/// \`None\` for unknown input.`
- `GuardedNormalize.normalized` (line 156): `/// The phone value to store: E.164 (\`+1…\`) when the parse was certain,\n/// otherwise raw digits without a \`+\` prefix.`
- `GuardedNormalize.note` (line 157): `/// \`Some(reason)\` when the value was ambiguous and stored digits-as-is;\n/// \`None\` when certain.`
- `OwnerHandleSet::new` (line 189): `/// Build the set from raw \`(value, HandleType)\` pairs; errors when the list\n/// is empty or a phone has no usable digits.` (check the current error text and reflect it in a `# Errors` note if it is a `Result`).
- `OwnerHandleSet::is_owner` (line 212): `/// Whether a raw handle value plus type matches an owner in the set after\n/// the same normalization.`

- [ ] **Step 3: Verify**

Run: `cargo doc --no-deps -p phone 2>&1 | grep -E "warning|error"` — expect zero lines; the gate names anything missed.
Run: `cargo test -p phone` — all pass, output pristine.

- [ ] **Step 4: Commit**

```bash
git add crates/libs/phone/src/lib.rs
git commit -m "docs(phone): document the remaining public items"
```

---

### Task 13: reexport documentation, gate, and CLI-page check

The `reexport` part of finding 8.

**Files:**
- Modify: `crates/libs/reexport/src/cli.rs`, `crates/libs/reexport/src/lib.rs`

**Interfaces:**
- Produces: a documented, gated `message-reexport`.
- Consumes: nothing new.
- Constraint: the clap `--help` output must not change. The `Cli` struct's help "about" line comes from the explicit `#[command(about = "Convert an existing Message Vault output to another format")]` attribute, and every arg field already has a doc comment — so the added docs (struct-level + `clap_command`) do not feed clap. Verify with the committed-pages test.

- [ ] **Step 1: Add the gate and document the two gaps**

Insert `#![warn(missing_docs)]` in `crates/libs/reexport/src/lib.rs` after its `//!` intro.

In `crates/libs/reexport/src/cli.rs`:
- Above `pub struct Cli` (line 11): `/// Command-line flags for the \`message-reexporter\` binary; the about text\n/// comes from the \`#[command(about)]\` attribute.`
- Above `pub fn clap_command()` (line 57): `/// The clap \`Command\` for embedding \`--help\` output into GUI docs.`

- [ ] **Step 2: Verify — including the committed CLI pages**

Run: `cargo doc --no-deps -p message-reexport 2>&1 | grep -E "warning|error"` — expect zero lines; the gate names anything missed.
Run: `cargo test -p message-reexport` — all pass.
Run: `cargo test -p dump-cli-docs committed_cli_pages_match_dump`
Expected: PASS — the `message-reexporter` page renders identical help text.
If it fails **and the diff shows only this task's two doc comments changed the page**, regenerate and commit the page in this same task:
`cargo run -p dump-cli-docs -- --output-dir docs/src/content/docs/vault/developer/reference`
If it fails for any other reason, stop and report BLOCKED.

- [ ] **Step 3: Commit**

```bash
git add crates/libs/reexport/src/cli.rs crates/libs/reexport/src/lib.rs
git commit -m "docs(reexport): document the CLI surface"
```

---

### Task 14: CHANGELOG and final workspace verification

**Files:**
- Modify: `CHANGELOG.md`

- [ ] **Step 1: Add the CHANGELOG entry**

In `CHANGELOG.md`, under `[Unreleased]`, add a `### Changed` entry (matching the file's existing entry style):

```markdown
- **Libraries:** add the `missing_docs` gate to every lib crate and document
  the full public surface, share one `AttachmentMeta` across the IR, CSV,
  and mail layers, switch csv parsers to `anyhow` errors, expose the
  unsafe-attachment-path message as a const, share one test fixture, and
  split the go-sms-mms unit decoders into their own module. No API behavior
  change.
```

- [ ] **Step 2: Final verification**

Run each of these and confirm clean output:
- `cargo test --workspace` — all 67 targets pass
- `cargo fmt --check`
- `cargo check --workspace`
- `cargo clippy --workspace -- -D warnings`
- `cargo doc --no-deps -p message-ir -p message-ir-format -p sbr -p media -p contacts -p message-csv -p go-sms-mms -p obfuscate -p mail -p phone -p message-reexport 2>&1 | grep -E "warning|error"` — zero lines
- `cargo test -p message-vault-server committed_openapi_matches_dump` and `cargo test -p dump-cli-docs committed_cli_pages_match_dump` — both pass (no utoipa or clap changes anywhere in this plan)

- [ ] **Step 3: Commit**

```bash
git add CHANGELOG.md
git commit -m "docs: changelog for libs documentation and consolidation"
```
