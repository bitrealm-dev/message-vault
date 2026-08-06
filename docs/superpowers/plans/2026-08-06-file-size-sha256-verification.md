# File Size + SHA-256 Verification Across Export, Transport, and Server

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make attachment verification consistent across all layers — exporters hash once and write both `digest_sha256` + `size_bytes` into JSONL, vault-push always re-verifies every attachment before upload, and the server skips expensive SHA-256 hashing for large files (≥ 20 MB) where a size check provides equivalent confidence.

**Architecture:** Three independent verification layers. Exporters own the one-time expensive hash and write both fields to JSONL. vault-push runs a mandatory Phase 2 that hashes every file, compares against JSONL claims, and warns on mismatch (using actual values). `--trust-export` restores the old fast path (trust JSONL when sizes match). The server gates its own re-hash at `asset_hash_threshold_bytes` — files below the threshold are hashed and verified; files at or above it are accepted on declared size match alone.

**Tech Stack:** Rust, message-ir (serde), vault-push (reqwest + sha2), message-vault-rs (axum + sha2 + rusqlite), demo-seed (rand + sha2 + image)

**Repos:** `message-vault-io` (client/exporters) and `message-vault-rs` (server/demo-seed)

## Global Constraints

- `digest_sha256` and `size_bytes` fields already exist on `IrAttachment` in `message-ir` — do not change the IR schema
- `size_bytes` semantics: on-disk / vault asset length in bytes (not file contents), `None` when unknown
- `asset_hash_threshold_bytes` default: 20 MiB (20971520 bytes)
- Server config is static TOML, read once at startup. No restart-avoidance needed.
- `--verify-digests` changes semantics: was the only way to verify, now means "verify AND fail on mismatch"
- `--trust-export` is new: skip hash when `size_bytes` matches disk `stat`

---

### Task 1: Exporter — populate `size_bytes` in imessage-ir-exporter emit.rs

**Files:**
- Modify: `crates/exporters/imessage-ir-exporter/src/emit.rs:325-351` (attachment construction)
- Modify: `crates/exporters/imessage-ir-exporter/src/emit.rs:484-498` (`persist_attachment` return type)

**Interfaces:**
- Consumes: `IrAttachment { size_bytes: Option<u64>, .. }` from `message-ir` (existing)
- Produces: `persist_attachment` returns `(String, String, u64)` — relative path, sha256 hex, byte length

**Why:** `persist_attachment` already hashes bytes via `Sha256::digest(bytes)` and has `bytes.len()` available. It currently returns `(path, digest)` and the caller sets `size_bytes: None`. Returning the byte length and plumbing it through lets exported JSONL carry accurate `size_bytes` from the start.

- [ ] **Step 1: Change `persist_attachment` return type and return size**

In `crates/exporters/imessage-ir-exporter/src/emit.rs`, at line 489, change the return type from `Result<(String, String), RuntimeError>` to `Result<(String, String, u64), RuntimeError>` and return the byte length:

```rust
fn persist_attachment(
    attachments_dir: &Path,
    timestamp_unix_ms: i64,
    bytes: &[u8],
    original_name: Option<&str>,
) -> Result<(String, String, u64), RuntimeError> {
    let name = attachment_dest_name(timestamp_unix_ms, bytes, original_name);
    let dest = attachments_dir.join(&name);
    if !dest.is_file() {
        fs::write(&dest, bytes)?;
    }
    let byte_len = bytes.len() as u64;
    Ok((
        format!("attachments/{name}"),
        hex::encode(Sha256::digest(bytes)),
        byte_len,
    ))
}
```

- [ ] **Step 2: Update call site to consume the new size**

At line 325-340, update the destructuring and `size_bytes` field:

```rust
let (path, digest_sha256, file_size) = if persist_to_disk {
    if has_bytes {
        let (rel_path, digest, size) = persist_attachment(
            attachments_dir,
            mail.timestamp_unix_ms,
            &attachment.bytes,
            attachment.original_name.as_deref(),
        )?;
        (Some(rel_path), Some(digest), Some(size))
    } else {
        (None, attachment.digest_sha256.clone(), None)
    }
} else {
    let bytes = has_bytes.then(|| attachment.bytes.clone());
    let size = bytes.as_ref().map(|b| b.len() as u64);
    (None, attachment.digest_sha256.clone(), size)
};
```

And at line 349, change `size_bytes: None` to `size_bytes: file_size`.

- [ ] **Step 3: Tag the non-persist attachment sites with a comment**

Two other sites construct `IrAttachment` with `size_bytes: None` (around lines 722 and 827). These are references from the mail parser that carry `digest_sha256` but not size. Leave these as `None` — they'll be re-computed by vault-push Phase 2. Add a comment above each:

```rust
size_bytes: None, // computed by vault-push during transport
```

- [ ] **Step 4: Build and verify**

Run: `cargo build -p imessage-ir-exporter`
Expected: compiles cleanly

- [ ] **Step 5: Run existing tests**

Run: `cargo test -p imessage-ir-exporter`
Expected: all tests pass

- [ ] **Step 6: Commit**

```bash
git add crates/exporters/imessage-ir-exporter/src/emit.rs
git commit -m "feat: populate size_bytes on IrAttachment during export

persist_attachment already hashes bytes; now also returns byte length
so exported JSONL carries accurate size_bytes from the start.

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

### Task 2: vault-push — invert default verification behavior in resolve_attachment_digest

**Files:**
- Modify: `crates/vault-push/src/run.rs:520-563` (`resolve_attachment_digest`)

**Interfaces:**
- Consumes: `IrAttachment { digest_sha256, size_bytes, .. }` from JSONL reader
- Produces: `resolve_attachment_digest(abs, claimed_raw, claimed_size, verify_digests, trust_export, cache, name, rel, warn) -> Result<String>`
  — new parameters: `claimed_size: Option<u64>`, `trust_export: bool`, `warn: &mut dyn FnMut(String)`

**Why:** The current default trusts JSONL sha256 unconditionally. The new default must hash every file, compare against JSONL claims, warn on mismatch, and use the actual hash. This is the core behavioral change.

- [ ] **Step 1: Change the function signature**

In `crates/vault-push/src/run.rs`, replace the signature at line 520:

```rust
fn resolve_attachment_digest(
    abs: &Path,
    claimed_raw: Option<&str>,
    claimed_size: Option<u64>,       // new: from JSONL size_bytes
    verify_digests: bool,             // unchanged: fail on mismatch
    trust_export: bool,               // new: skip hash when sizes match
    cache: &DigestCache,
    name: &str,
    rel: &str,
    warn: &mut dyn FnMut(String),     // new: warning sink
) -> Result<String> {
```

- [ ] **Step 2: Rewrite the decision logic**

Replace the function body (lines 528-563) with:

```rust
    // Fast path: another conversation already hashed this absolute path
    // during this run. Always trust the cache — it was computed from disk.
    {
        let guard = cache.lock().expect("digest cache mutex poisoned");
        if let Some(digest) = guard.get(abs) {
            return Ok(digest.clone());
        }
    }

    // Normalize the claimed sha256 from JSONL (may be absent or malformed).
    let claimed = match claimed_raw {
        Some(raw) => match normalize_digest_sha256(raw) {
            Ok(d) => Some(d),
            Err(e) => {
                warn(format!("{name}: bad digest_sha256 for {rel}: {e}"));
                None
            }
        },
        None => None,
    };

    let disk_size = std::fs::metadata(abs)
        .with_context(|| format!("{name}: stat {rel}"))?
        .len();

    // trust_export fast path: skip hash when JSONL size matches disk.
    // The size match is a cheap proxy for "file unchanged since export."
    // Same-size-different-content is practically impossible on modern
    // filesystems; the vault server is the final verifier on upload.
    if trust_export && !verify_digests {
        if let (Some(ref dig), Some(cl_size)) = (claimed.as_ref(), claimed_size) {
            if cl_size == disk_size {
                cache
                    .lock()
                    .expect("digest cache mutex poisoned")
                    .insert(abs.to_path_buf(), dig.clone());
                return Ok(dig.clone());
            }
        }
    }

    // Hash from disk (the common path — always runs unless trust_export
    // short-circuited above).
    let disk_digest = hash_file(abs)
        .with_context(|| format!("{name}: hash {rel}"))?;

    // Compare against JSONL claim.
    if let Some(ref claimed_digest) = claimed {
        if claimed_digest != &disk_digest {
            let size_note = match claimed_size {
                Some(cs) if cs != disk_size => {
                    format!(", size changed from {cs} to {disk_size} bytes")
                }
                _ => String::new(),
            };
            let msg = format!(
                "{name}: sha256 mismatch for {rel}: \
                 claimed {claimed_digest}, got {disk_digest}{size_note}"
            );
            if verify_digests {
                bail!("{msg}");
            }
            warn(msg);
        }
    }

    cache
        .lock()
        .expect("digest cache mutex poisoned")
        .insert(abs.to_path_buf(), disk_digest.clone());
    Ok(disk_digest)
```

- [ ] **Step 3: Build — expect errors at call sites**

Run: `cargo build -p vault-push`
Expected: compilation errors at the `resolve_attachment_digest` call site in `prepare_file`. That's fine — fixed in Task 4.

---

### Task 3: vault-push — add `trust_export` to VaultPushConfig and CLI

**Files:**
- Modify: `crates/vault-push/src/run.rs:92-125` (`VaultPushConfig` struct)
- Modify: `crates/vault-push/src/bin/vault_push.rs:54-56` (CLI args)

**Interfaces:**
- Consumes: existing `VaultPushConfig` fields
- Produces: `VaultPushConfig { trust_export: bool, .. }`
- Exported: `--trust-export` CLI flag (new)

- [ ] **Step 1: Add `trust_export` field to VaultPushConfig**

In `crates/vault-push/src/run.rs`, add after the `verify_digests` field (line 112):

```rust
    /// If true, skip re-hashing attachments when the JSONL `size_bytes` matches
    /// the file size on disk. Default remains full verification of every file.
    pub trust_export: bool,
```

- [ ] **Step 2: Add `--trust-export` CLI flag**

In `crates/vault-push/src/bin/vault_push.rs`, add after the `--verify-digests` flag (around line 56):

```rust
    /// Trust export metadata: skip re-hashing attachments when size_bytes matches
    /// the file size on disk. Without this flag every attachment is re-hashed.
    #[arg(long, default_value_t = false)]
    trust_export: bool,
```

- [ ] **Step 3: Wire into the VaultPushConfig construction**

In `crates/vault-push/src/bin/vault_push.rs`, in `real_main` around line 129, add:

```rust
    let cfg = VaultPushConfig {
        // ... existing fields ...
        verify_digests: cli.verify_digests,
        trust_export: cli.trust_export,   // new
        max_retries: cli.max_retries,
        // ...
    };
```

- [ ] **Step 4: Build and verify compilation**

Run: `cargo build -p vault-push`
Expected: compiles (may warn about unused `trust_export` until Task 4)

- [ ] **Step 5: Commit**

```bash
git add crates/vault-push/src/run.rs crates/vault-push/src/bin/vault_push.rs
git commit -m "feat: add --trust-export flag to vault-push

Adds trust_export field to VaultPushConfig and --trust-export CLI flag.
When set, vault-push skips re-hashing attachments whose size_bytes
matches the file on disk. Not yet wired into resolve_attachment_digest.

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

### Task 4: vault-push — wire new verification into prepare_file call site

**Files:**
- Modify: `crates/vault-push/src/run.rs:1550-1635` (`prepare_file` — per-message digest collection and call to `resolve_attachment_digest`)

**Interfaces:**
- Consumes: `resolve_attachment_digest` new signature from Task 2, `VaultPushConfig { trust_export, .. }` from Task 3
- Produces: `per_message_digests: Vec<Vec<(usize, String, u64)>>` (unchanged structure)

- [ ] **Step 1: Create a warning accumulator in prepare_file**

In `prepare_file` (around line 1580), add a `warnings` vector alongside `log_lines`:

```rust
    let mut log_lines = Vec::new();
    let mut warnings: Vec<String> = Vec::new();  // new
```

- [ ] **Step 2: Update the call to resolve_attachment_digest**

At lines 1618-1625, update the call to pass the new parameters:

```rust
            let digest = resolve_attachment_digest(
                &abs,
                claimed,
                att.size_bytes,                // new: claimed size from JSONL
                cfg.verify_digests,
                cfg.trust_export,              // new: trust-export flag
                digest_cache,
                name,
                rel,
                &mut |msg| warnings.push(msg), // new: warning callback
            )?;
```

- [ ] **Step 3: Emit accumulated warnings after the scan loop**

After the `for msg in messages` loop and the `profile.attachment_scan_hash_ms` assignment (after line 1635), add:

```rust
    for warning in &warnings {
        log_lines.push(format!("WARN {warning}"));
    }
```

- [ ] **Step 4: Build and verify compilation**

Run: `cargo build -p vault-push`
Expected: compiles cleanly — the call site from Task 2 is now wired.

- [ ] **Step 5: Run existing vault-push tests**

Run: `cargo test -p vault-push`
Expected: all tests pass. In particular `normalize_digest_sha256_accepts_hex` and the progress/report tests are unaffected.

- [ ] **Step 6: Commit**

```bash
git add crates/vault-push/src/run.rs
git commit -m "feat: wire trust_export and mandatory verification into vault-push

resolve_attachment_digest now defaults to hashing every attachment and
comparing against JSONL claims. Warnings are emitted on mismatch and
actual values are used. --trust-export restores the old fast path
(trust JSONL when size_bytes matches disk). --verify-digests still
fails on mismatch.

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

### Task 5: Server config — add `asset_hash_threshold_bytes`

**Files:**
- Modify: `src/config.rs:16-41` (`ServerConfig` struct, new default function)
- Modify: `src/asset_uploads.rs:22-52` (`UploadLimits` struct, new field)
- Modify: `config/config.toml:14-15` (add config line)
- Modify: `config/config.docker.toml:14-15` (add config line)
- Modify: `src/server.rs` (or wherever `UploadLimits::resolve` is called — pass new field)

**Repo:** `message-vault-rs`

**Interfaces:**
- Consumes: existing `ServerConfig`, `UploadLimits`
- Produces: `ServerConfig { asset_hash_threshold_bytes: u64, .. }`, `UploadLimits { hash_threshold_bytes: u64, .. }`

- [ ] **Step 1: Add field and default to ServerConfig**

In `src/config.rs`, add after `asset_part_size` (line 27):

```rust
    /// Attachments at or above this size (in bytes) skip server-side SHA-256
    /// verification at upload completion. The server still verifies that the
    /// assembled file size matches the declared size. Default 20 MiB.
    #[serde(default = "default_asset_hash_threshold_bytes")]
    pub asset_hash_threshold_bytes: u64,
```

Add the default function after `default_asset_part_size` (after line 41):

```rust
fn default_asset_hash_threshold_bytes() -> u64 {
    20 * 1024 * 1024
}
```

- [ ] **Step 2: Add field to UploadLimits**

In `src/asset_uploads.rs`, after `max_bytes` field (line 25):

```rust
pub struct UploadLimits {
    pub part_size: usize,
    pub max_bytes: u64,
    /// Attachments at or above this size skip server-side SHA-256.
    /// Below this threshold the server hashes and verifies the digest.
    pub hash_threshold_bytes: u64,
}
```

Update `Default for UploadLimits` (line 28):

```rust
impl Default for UploadLimits {
    fn default() -> Self {
        Self {
            part_size: DEFAULT_PART_SIZE,
            max_bytes: DEFAULT_MAX_BYTES,
            hash_threshold_bytes: 20 * 1024 * 1024,
        }
    }
}
```

Update `UploadLimits::resolve` (line 40) to accept and forward the threshold:

```rust
    pub fn resolve(part_size: usize, max_bytes: u64, hash_threshold_bytes: u64) -> Self {
        let part_size = std::env::var("VAULT_ASSET_PART_SIZE")
            .ok()
            .and_then(|s| s.parse::<usize>().ok())
            .filter(|&n| n >= 1 && n <= part_size.max(1))
            .unwrap_or(part_size.max(1));
        let max_bytes = max_bytes.max(part_size as u64);
        Self {
            part_size,
            max_bytes,
            hash_threshold_bytes,
        }
    }
```

- [ ] **Step 3: Update config files**

In `config/config.toml` and `config/config.docker.toml`, after `asset_part_size`:

```toml
# Attachments at or above this size (bytes) skip server-side SHA-256
# verification at upload completion. 20 MiB default.
asset_hash_threshold_bytes = 20971520
```

- [ ] **Step 4: Find and update UploadLimits construction site**

Search for `UploadLimits::resolve` call sites:

```bash
grep -rn "UploadLimits::resolve" src/
```

Update each call to pass the new field:

```rust
let limits = UploadLimits::resolve(
    server_cfg.asset_part_size,
    server_cfg.asset_max_bytes,
    server_cfg.asset_hash_threshold_bytes,
);
```

- [ ] **Step 5: Build and verify**

Run: `cargo build`
Expected: compiles cleanly (may warn about unused `hash_threshold_bytes` until Task 6)

- [ ] **Step 6: Commit**

```bash
git add src/config.rs src/asset_uploads.rs config/config.toml config/config.docker.toml
# plus the server.rs (or wherever UploadLimits::resolve is called)
git commit -m "feat: add asset_hash_threshold_bytes to server config

New [server] config key controls whether the server hashes uploaded
assets on completion. Default 20 MiB. Below threshold: hash+verify.
At or above threshold: skip hash, trust declared size.

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

### Task 6: Server — gate SHA-256 verification in complete_upload

**Files:**
- Modify: `src/assets.rs:40-86` (`store_verified` — accept skip_hash parameter)
- Modify: `src/asset_uploads.rs:249-312` (`complete_upload` — pass threshold decision)
- Modify: all call sites of `store_verified` and `complete_upload` (server routes, import paths)

**Interfaces:**
- Consumes: `UploadLimits { hash_threshold_bytes, .. }` from Task 5
- Produces: `store_verified(source, claimed_sha256, assets_root, export_mime, consume_source, skip_hash) -> Result<(StoredAsset, bool)>`
  — new `skip_hash: bool` parameter

- [ ] **Step 1: Add `skip_hash` parameter to store_verified**

In `src/assets.rs`, change the function signature at line 40:

```rust
pub fn store_verified(
    source: &Path,
    claimed_sha256: &str,
    assets_root: &Path,
    export_mime: Option<&str>,
    consume_source: bool,
    skip_hash: bool,                          // new
) -> Result<(StoredAsset, bool)> {
```

In the function body, replace the hash-and-compare block (lines 64-68) with:

```rust
    if skip_hash {
        // Trust the claimed sha256 — caller verified the assembled file size
        // matches the declared size. For large files this avoids an expensive
        // full-file SHA-256 pass on the server.
    } else {
        let actual = hash_file(source)
            .with_context(|| format!("failed to hash {}", source.display()))?;
        if actual != claimed {
            anyhow::bail!("sha256 mismatch: claimed {claimed}, got {actual}");
        }
    }
```

- [ ] **Step 2: Update complete_upload to accept limits and pass the decision**

In `src/asset_uploads.rs`, change `complete_upload` signature (line 249):

```rust
pub fn complete_upload(
    assets_root: &Path,
    sha256: &str,
    upload_id: &str,
    limits: UploadLimits,                    // new
) -> Result<(StoredAsset, bool)> {
```

After the assembled size check (around line 299), compute `skip_hash`:

```rust
        if total != manifest.bytes {
            let _ = fs::remove_file(&assembled);
            bail!(
                "assembled size {total} does not match declared {}",
                manifest.bytes
            );
        }
        let skip_hash = total >= limits.hash_threshold_bytes;
    }

    let result = assets::store_verified(
        &assembled,
        &sha,
        assets_root,
        manifest.mime.as_deref(),
        true,
        skip_hash,                            // new
    );
```

- [ ] **Step 3: Update all call sites of complete_upload**

Search for `complete_upload` call sites:

```bash
grep -rn "complete_upload" src/
```

Update each to pass `limits`. If a site doesn't have limits available, use `UploadLimits::default()`.

- [ ] **Step 4: Update all call sites of store_verified**

Search for `store_verified` call sites outside `assets.rs`:

```bash
grep -rn "store_verified" src/
```

For import/CLI paths (not multipart upload), pass `false` for `skip_hash`:

```rust
let (stored, already) = store_verified(source, &sha, assets_root, mime, false, false)?;
```

- [ ] **Step 5: Update tests**

In `src/assets.rs` tests, update `store_verified` calls to pass `false` for `skip_hash`:

```rust
let (first, present) =
    store_verified(src.path(), &sha, root, Some("text/plain"), false, false).unwrap();
```

In `src/asset_uploads.rs` tests, update `complete_upload` calls and direct `store_verified` calls to pass `limits` and `skip_hash` respectively.

- [ ] **Step 6: Build and run all tests**

Run: `cargo build && cargo test`
Expected: all tests pass

- [ ] **Step 7: Commit**

```bash
git add src/assets.rs src/asset_uploads.rs src/server.rs
# plus any other call site files
git commit -m "feat: gate server-side SHA-256 on asset_hash_threshold_bytes

complete_upload skips the SHA-256 pass when the assembled file is at or
above the configured threshold. Size is still verified. store_verified
accepts an explicit skip_hash parameter; import/CLI paths always hash.

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

### Task 7: demo-seed — hash attachments at generation time

**Files:**
- Modify: `crates/demo-seed/src/assets.rs:58-78` (`write_attachment_blobs` — return hash/size map)
- Modify: `crates/demo-seed/src/conversations.rs:67-74` (`write_all` signature)
- Modify: `crates/demo-seed/src/conversations.rs:444-465` (`decorate_message` signature)
- Modify: `crates/demo-seed/src/conversations.rs:690-703` (`add_jpg_attachment` body)
- Modify: `crates/demo-seed/src/conversations.rs:706-729` (`add_attachment` body)
- Modify: `crates/demo-seed/src/lib.rs:39` (call site)

**Repo:** `message-vault-rs`

**Interfaces:**
- Consumes: `IrAttachment { digest_sha256, size_bytes, .. }` from `message-ir`
- Produces: `write_attachment_blobs(dir) -> Result<HashMap<String, (String, u64)>>` — maps `"attachments/filename.ext"` → `(sha256_hex, byte_length)`
- All attachment-writing functions now fill `digest_sha256: Some(...)` and `size_bytes: Some(...)` instead of `None`

- [ ] **Step 1: Hash blobs and return a lookup map from write_attachment_blobs**

In `crates/demo-seed/src/assets.rs`, add imports at the top:

```rust
use sha2::{Digest, Sha256};
use std::collections::HashMap;
```

Change `write_attachment_blobs` return type and hash each blob after writing:

```rust
pub fn write_attachment_blobs(dir: &Path) -> Result<HashMap<String, (String, u64)>> {
    let specs: &[(&str, [u8; 3])] = &[
        ("sunset.jpg", [255, 140, 60]),
        ("park.jpg", [72, 160, 95]),
        ("dinner.jpg", [180, 85, 70]),
        ("puppy.jpg", [210, 175, 130]),
        ("receipt.jpg", [245, 245, 240]),
        ("selfie.jpg", [90, 130, 200]),
        ("beach.jpg", [60, 175, 220]),
        ("flowers.jpg", [220, 100, 150]),
    ];
    let mut digests = HashMap::new();
    for (name, rgb) in specs {
        let path = dir.join(name);
        write_color_jpeg(&path, *rgb, 320, 240)?;
        let bytes = std::fs::read(&path)?;
        let sha = hex::encode(Sha256::digest(&bytes));
        digests.insert(format!("attachments/{name}"), (sha, bytes.len() as u64));
    }

    // Non-JPEG blobs
    fs::write(dir.join("landscape.png"), MINI_PNG)?;
    let sha_png = hex::encode(Sha256::digest(MINI_PNG));
    digests.insert("attachments/landscape.png".into(), (sha_png, MINI_PNG.len() as u64));

    fs::write(dir.join("sticker.gif"), MINI_GIF)?;
    let sha_gif = hex::encode(Sha256::digest(MINI_GIF));
    digests.insert("attachments/sticker.gif".into(), (sha_gif, MINI_GIF.len() as u64));

    fs::write(dir.join("voice.caf"), MINI_CAF)?;
    let sha_caf = hex::encode(Sha256::digest(MINI_CAF));
    digests.insert("attachments/voice.caf".into(), (sha_caf, MINI_CAF.len() as u64));

    fs::write(dir.join("notes.pdf"), MINI_PDF)?;
    let sha_pdf = hex::encode(Sha256::digest(MINI_PDF));
    digests.insert("attachments/notes.pdf".into(), (sha_pdf, MINI_PDF.len() as u64));

    // Note: attachments/missing-file.heic is intentionally absent from the map.
    // The JSONL will reference it but the file won't exist on disk, exercising
    // vault-push's missing-file warning path.

    Ok(digests)
}
```

- [ ] **Step 2: Thread the digest map through write_all**

In `crates/demo-seed/src/conversations.rs`, change `write_all` signature:

```rust
pub fn write_all(
    staging: &Path,
    _attachments: &Path,
    roster: &Roster,
    cfg: &SeedConfig,
    corpus: &Corpus,
    rng: &mut impl Rng,
    attachment_digests: &HashMap<String, (String, u64)>,   // new
) -> Result<GenStats> {
```

Pass `attachment_digests` through to `write_individual`, `write_unassigned`, and `write_group` calls within the function body.

- [ ] **Step 3: Thread through write_individual and its call sites**

Change `write_individual` signature to accept `attachment_digests` and pass it to `decorate_message`:

```rust
fn write_individual(
    staging: &Path,
    chat_id: &str,
    display: String,
    span_years: f64,
    msg_count: usize,
    cfg: &SeedConfig,
    corpus: &Corpus,
    rng: &mut impl Rng,
    stats: &mut GenStats,
    attachment_digests: &HashMap<String, (String, u64)>,   // new
) -> Result<()> {
```

Update the `decorate_message` call at line 184:

```rust
decorate_message(&mut msg, i, msg_count, chat_id, from_me, cfg, rng, stats,
    &mut origin_guid, attachment_digests);
```

- [ ] **Step 4: Thread through write_unassigned and write_group**

Same pattern — add `attachment_digests` parameter to each function and pass it to `add_jpg_attachment` and `add_attachment` calls.

- [ ] **Step 5: Update add_jpg_attachment to write sha256 and size_bytes**

```rust
fn add_jpg_attachment(
    msg: &mut IrMessage,
    idx: usize,
    stats: &mut GenStats,
    digests: &HashMap<String, (String, u64)>,    // new
) {
    let photo = &JPG_PHOTOS[idx % JPG_PHOTOS.len()];
    let (sha, size) = digests
        .get(photo.path)
        .map(|(s, z)| (s.clone(), *z))
        .unwrap_or_default();
    msg.attachments.push(IrAttachment {
        path: Some(photo.path.into()),
        original_name: Some(photo.original_name.into()),
        mime_type: Some("image/jpeg".into()),
        digest_sha256: if sha.is_empty() { None } else { Some(sha) },
        is_sticker: false,
        transcription: None,
        sticker_effect: None,
        bytes: None,
        size_bytes: if sha.is_empty() { None } else { Some(size) },
    });
    stats.attachment_refs += 1;
}
```

- [ ] **Step 6: Update add_attachment similarly**

```rust
fn add_attachment(
    msg: &mut IrMessage,
    idx: usize,
    stats: &mut GenStats,
    files: &[(&str, &str, bool)],
    digests: &HashMap<String, (String, u64)>,    // new
) {
    let (path, mime, is_sticker) = files[idx % files.len()];
    let (sha, size) = digests
        .get(*path)
        .map(|(s, z)| (s.clone(), *z))
        .unwrap_or_default();
    let transcription = if mime.starts_with("audio/") {
        Some("Hey, just leaving a quick voice note.".into())
    } else {
        None
    };
    msg.attachments.push(IrAttachment {
        path: Some(path.into()),
        original_name: Some(path.rsplit('/').next().unwrap_or(path).into()),
        mime_type: Some(mime.into()),
        digest_sha256: if sha.is_empty() { None } else { Some(sha) },
        is_sticker,
        transcription,
        sticker_effect: None,
        bytes: None,
        size_bytes: if sha.is_empty() { None } else { Some(size) },
    });
    stats.attachment_refs += 1;
}
```

- [ ] **Step 7: Update decorate_message signature**

```rust
fn decorate_message(
    msg: &mut IrMessage,
    i: usize,
    msg_count: usize,
    peer: &str,
    from_me: bool,
    cfg: &SeedConfig,
    rng: &mut impl Rng,
    stats: &mut GenStats,
    origin_guid: &mut Option<String>,
    attachment_digests: &HashMap<String, (String, u64)>,   // new
) {
```

Update internal calls to `add_jpg_attachment` and `add_attachment` to pass `attachment_digests`.

- [ ] **Step 8: Update generate() in lib.rs**

In `crates/demo-seed/src/lib.rs`, change line 39:

```rust
    let attachment_digests = assets::write_attachment_blobs(&attachments)?;
    let roster = personas::build_roster(cfg, &names, &mut rng);
    // ...
    let stats =
        conversations::write_all(&staging, &attachments, &roster, cfg, &corpus,
            &mut rng, &attachment_digests)?;
```

- [ ] **Step 9: Build and run tests**

Run: `cargo build -p demo-seed && cargo test -p demo-seed`
Expected: compiles and all tests pass

- [ ] **Step 10: Regenerate demo and verify JSONL**

```bash
cargo run -p demo-seed -- --out /tmp/demo-digest-test
head -5 /tmp/demo-digest-test/staging/imessage/*1to1*.jsonl | grep -o '"digest_sha256":"[a-f0-9]\{64\}"' | head -3
head -5 /tmp/demo-digest-test/staging/imessage/*1to1*.jsonl | grep -o '"size_bytes":[0-9]\+' | head -3
```

Expected: both fields present with non-null values for messages with attachments.

- [ ] **Step 11: Commit**

```bash
git add crates/demo-seed/src/
git commit -m "feat: hash demo attachments at generation time

demo-seed now computes sha256 and file size for every synthetic blob
and writes both into the generated JSONL. Makes demo data self-contained
— vault-push can verify or trust-export without re-hashing. The
intentionally missing attachments/missing-file.heic is excluded from
the digest map, exercising the missing-file warning path.

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

### Task 8: Integration — spot-check the full pipeline

**Files:** None (verification only)

**Why:** Confirm all three layers work together. Generate demo data, verify the JSONL, push to a local vault.

- [ ] **Step 1: Generate fresh demo data**

```bash
cd ~/repo/message-vault-rs
cargo run -p demo-seed -- --out /tmp/demo-integration-test
```

- [ ] **Step 2: Verify JSONL fields**

```bash
cd /tmp/demo-integration-test/staging/imessage
# Check that attachments have sha256 (64 hex chars) and size_bytes (> 0)
grep -oh '"digest_sha256":"[a-f0-9]\{64\}"' *.jsonl | head -5
grep -oh '"size_bytes":[0-9]\+' *.jsonl | head -5
# The intentionally missing file should have no digest
grep "missing-file" *.jsonl | grep -v '"digest_sha256":"[a-f0-9]\{64\}"' || echo "OK: missing-file has no digest"
```

Expected: digests present for real files, missing-file.heic has no `digest_sha256` or `size_bytes`.

- [ ] **Step 3: Push with default verification**

```bash
cd ~/repo/message-vault-io
cargo run -p vault-push -- \
    --url http://127.0.0.1:8080 \
    --key "$VAULT_KEY" \
    --input /tmp/demo-integration-test/staging/imessage
```

Expected: log lines show verification progress. `attachments/missing-file.heic` produces a warning like `WARN ...: missing attachment attachments/missing-file.heic`. No sha256 mismatch warnings (fresh demo data is self-consistent).

- [ ] **Step 4: Push again with --trust-export**

```bash
cargo run -p vault-push -- \
    --url http://127.0.0.1:8080 \
    --key "$VAULT_KEY" \
    --input /tmp/demo-integration-test/staging/imessage \
    --trust-export
```

Expected: faster (no re-hashing). Same results. The journal from the first run means most work is skipped anyway, but the trust-export fast path is exercised on whatever does get processed.

- [ ] **Step 5: Clean up**

```bash
rm -rf /tmp/demo-integration-test /tmp/demo-digest-test
```
