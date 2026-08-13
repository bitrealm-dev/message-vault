use std::fs::{self, File, OpenOptions};
use std::io::{BufReader, Read, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use sha2::{Digest, Sha256};

#[derive(Debug, Default)]
pub struct AssetStats {
    pub copied: u64,
    pub deduped: u64,
    pub missing: u64,
}

#[derive(Debug, Clone)]
pub struct StoredAsset {
    pub sha256: String,
    pub assets_path: String,
    pub mime_type: Option<String>,
}

/// Encode bytes as lowercase hex.
pub fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// SHA-256 fingerprint of `data` as 64 lowercase hex digits.
///
/// SHA-256 is a short fingerprint of the file contents.
pub fn sha256_hex(data: &[u8]) -> String {
    hex_encode(&Sha256::digest(data))
}

/// Relative path under the assets root: first two hex digits as a folder, then
/// the full fingerprint, then `ext`. `.jpeg` is stored as `.jpg`.
pub fn shard_rel_path(sha256: &str, ext: &str) -> String {
    let ext = if ext == ".jpeg" { ".jpg" } else { ext };
    format!("{}/{}{}", &sha256[..2], sha256, ext)
}

/// Find a stored attachment by SHA-256 (a short fingerprint of the file
/// contents) and confirm the file on disk still matches that fingerprint.
///
/// Upload and import paths that skip sending bytes because "the file is already
/// here" must use this function. A truncated or replaced file is then never
/// treated as the real content.
pub fn lookup_by_sha256(assets_root: &Path, sha256: &str) -> Option<StoredAsset> {
    let stored = lookup_by_sha256_unverified(assets_root, sha256)?;
    let path = assets_root.join(&stored.assets_path);
    if hash_file(&path).ok()? != stored.sha256 {
        return None;
    }
    Some(stored)
}

/// Find the stored path and MIME type for a SHA-256 fingerprint without reading
/// the file.
///
/// Used only when streaming an authenticated download. The response body is the
/// file itself, the URL is the fingerprint, and the client can check what it
/// received. Hashing the whole file first would read every download twice.
pub fn lookup_by_sha256_unverified(assets_root: &Path, sha256: &str) -> Option<StoredAsset> {
    let sha = normalize_sha256(sha256)?;
    let existing = find_existing(assets_root, &sha)?;
    let assets_path = path_relative_to(assets_root, &existing).ok()?;
    let mime_type = mime_for_path_or_sidecar(assets_root, &existing, &sha);
    Some(StoredAsset {
        sha256: sha,
        assets_path,
        mime_type,
    })
}

/// Store `source` under `assets_root` using a caller-claimed SHA-256 fingerprint,
/// after checking that the file bytes match that claim.
///
/// When `consume_source` is true (HTTP upload temps), the source is removed
/// after the verified temporary copy is installed.
///
/// `skip_hash` is kept only so the function signature stays stable. Sources are
/// always hashed before reuse, so a wrong upload cannot be accepted just
/// because a matching file already exists.
///
/// Returns `(stored, already_present)`.
///
/// # Errors
///
/// Returns an error when the claimed fingerprint is invalid, the source is not
/// a regular file, the bytes do not match the claim, or the file cannot be
/// written under `assets_root`.
pub fn store_verified(
    source: &Path,
    claimed_sha256: &str,
    assets_root: &Path,
    export_mime: Option<&str>,
    consume_source: bool,
    _skip_hash: bool,
) -> Result<(StoredAsset, bool)> {
    store_verified_inner(
        source,
        claimed_sha256,
        assets_root,
        export_mime,
        consume_source,
        || {},
        || {},
    )
}

/// Same as [`store_verified`], with hooks so tests can observe copy vs reuse.
fn store_verified_inner(
    source: &Path,
    claimed_sha256: &str,
    assets_root: &Path,
    export_mime: Option<&str>,
    consume_source: bool,
    copy_ready: impl FnOnce(),
    selection_ready: impl FnOnce(),
) -> Result<(StoredAsset, bool)> {
    let claimed = require_sha256(claimed_sha256)?;
    ensure_regular_file(source)?;
    let source_mime = resolve_mime(export_mime, source);
    let (dest, already) = install_blob(
        source,
        assets_root,
        &claimed,
        consume_source,
        copy_ready,
        selection_ready,
    )?;
    let rel = path_relative_to(assets_root, &dest)?;
    let mime_type = if already {
        mime_for_existing_file(export_mime, &dest, source_mime)
    } else {
        source_mime
    };
    if let Some(mime) = mime_type.as_deref() {
        store_mime_metadata(assets_root, &claimed, mime)?;
    }
    Ok((
        StoredAsset {
            sha256: claimed,
            assets_path: rel,
            mime_type,
        },
        already,
    ))
}

/// Copy `source` into place through a temporary file in the same folder, then
/// rename. The second return value is `true` only when a concurrent or earlier
/// valid file already won.
///
/// Order of work matters for both safety and cost:
/// 1. Check an existing destination first. On a hit the source is hashed (a
///    wrong claimed fingerprint must never be accepted just because a valid
///    file exists) and the call returns without a copy, a disk flush, or a
///    rename. This is the common repeat-import and repeat-upload case.
/// 2. Otherwise copy into a temporary file in the destination folder, hashing
///    while writing, and refuse to keep the file unless the written bytes match
///    the claimed fingerprint. The bytes that land on disk are therefore always
///    bytes this call checked, even if the source changed underneath.
fn install_blob(
    source: &Path,
    assets_root: &Path,
    claimed_sha256: &str,
    consume_source: bool,
    copy_ready: impl FnOnce(),
    selection_ready: impl FnOnce(),
) -> Result<(PathBuf, bool)> {
    let shard = assets_root.join(&claimed_sha256[..2]);
    fs::create_dir_all(&shard).with_context(|| format!("failed to create {}", shard.display()))?;

    // New files use a single path with no extension, named only from the
    // fingerprint. `find_existing` still finds older files that kept an
    // extension, so those remain readable and reusable.
    let desired = assets_root.join(shard_rel_path(claimed_sha256, ""));
    let dest = if let Some(existing) = find_existing(assets_root, claimed_sha256) {
        if hash_file(&existing).is_ok_and(|actual| actual == claimed_sha256) {
            verify_source_digest(source, claimed_sha256)?;
            if consume_source {
                let _ = fs::remove_file(source);
            }
            return Ok((existing, true));
        }
        existing
    } else {
        desired
    };
    selection_ready();

    if let Ok(meta) = fs::symlink_metadata(&dest) {
        if meta.file_type().is_symlink() {
            bail!("refusing to install over symlink {}", dest.display());
        }
        if !meta.is_file() {
            bail!(
                "asset destination exists and is not a regular file: {}",
                dest.display()
            );
        }
        if hash_file(&dest).is_ok_and(|actual| actual == claimed_sha256) {
            verify_source_digest(source, claimed_sha256)?;
            if consume_source {
                let _ = fs::remove_file(source);
            }
            return Ok((dest, true));
        }
        let temporary = copy_to_verified_temp(source, &shard, claimed_sha256)?;
        copy_ready();
        temporary
            .persist(&dest)
            .map_err(|err| err.error)
            .with_context(|| format!("replace corrupt asset {}", dest.display()))?;
    } else {
        let temporary = copy_to_verified_temp(source, &shard, claimed_sha256)?;
        copy_ready();
        match temporary.persist_noclobber(&dest) {
            Ok(_) => {}
            Err(err) if err.error.kind() == std::io::ErrorKind::AlreadyExists => {
                // The copy above already checked that the source bytes match
                // the claimed fingerprint.
                if hash_file(&dest).is_ok_and(|actual| actual == claimed_sha256) {
                    if consume_source {
                        let _ = fs::remove_file(source);
                    }
                    return Ok((dest, true));
                }
                err.file
                    .persist(&dest)
                    .map_err(|persist_err| persist_err.error)
                    .with_context(|| format!("replace corrupt asset {}", dest.display()))?;
            }
            Err(err) => {
                return Err(err.error).with_context(|| format!("install {}", dest.display()));
            }
        }
    }
    if consume_source {
        let _ = fs::remove_file(source);
    }
    Ok((dest, false))
}

/// Reject a claimed SHA-256 fingerprint that the source bytes do not produce.
///
/// Used when skipping the copy because a matching file is already stored. Without
/// this check, a wrong claim would be accepted just because that file exists.
fn verify_source_digest(source: &Path, claimed_sha256: &str) -> Result<()> {
    let actual = hash_file(source).with_context(|| format!("read source {}", source.display()))?;
    if actual != claimed_sha256 {
        bail!("sha256 mismatch: claimed {claimed_sha256}, got {actual}");
    }
    Ok(())
}

/// Copy `source` into a flushed temporary file inside `shard`, hashing as it
/// writes, and fail unless the written bytes hash to `claimed_sha256`.
///
/// Hashing the bytes as they are written (rather than trusting an earlier hash
/// of the source path) keeps a source that changes mid-copy from being saved.
fn copy_to_verified_temp(
    source: &Path,
    shard: &Path,
    claimed_sha256: &str,
) -> Result<tempfile::NamedTempFile> {
    let mut temporary = tempfile::NamedTempFile::new_in(shard)
        .with_context(|| format!("create temporary asset in {}", shard.display()))?;
    let mut src =
        open_nofollow_read(source).with_context(|| format!("open source {}", source.display()))?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1024 * 1024];
    loop {
        let n = src
            .read(&mut buf)
            .with_context(|| format!("read source {}", source.display()))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
        temporary
            .write_all(&buf[..n])
            .with_context(|| format!("write temporary asset for {}", source.display()))?;
    }
    temporary.flush()?;
    temporary.as_file().sync_all()?;
    let actual = hex_encode(&hasher.finalize());
    if actual != claimed_sha256 {
        bail!("sha256 mismatch: claimed {claimed_sha256}, got {actual}");
    }
    Ok(temporary)
}

/// Hash `source` and store it under `assets_root/<sha[0:2]>/<sha><ext>`.
/// If the file already exists, skip the copy and count it as reused.
///
/// # Errors
///
/// Returns an error when the source cannot be hashed or stored.
pub fn hash_and_store(
    source: &Path,
    assets_root: &Path,
    export_mime: Option<&str>,
    stats: &mut AssetStats,
) -> Result<Option<StoredAsset>> {
    if !is_regular_file(source) {
        stats.missing += 1;
        return Ok(None);
    }

    let sha = hash_file(source).with_context(|| format!("failed to hash {}", source.display()))?;
    let (stored, already) = store_verified(source, &sha, assets_root, export_mime, false, false)?;
    if already {
        stats.deduped += 1;
    } else {
        stats.copied += 1;
    }
    Ok(Some(stored))
}

/// Delete abandoned multipart upload folders under `{assets}/.incoming` older
/// than `max_age_secs`.
///
/// Finished uploads already remove their session folders. Abandoned ones can
/// sit forever without this sweep, which runs from upload start.
///
/// # Errors
///
/// Returns an error when `{assets}/.incoming` cannot be read.
pub fn gc_stale_incoming(assets_root: &Path, max_age_secs: u64) -> Result<u64> {
    let incoming = assets_root.join(".incoming");
    if !incoming.is_dir() {
        return Ok(0);
    }
    let now = std::time::SystemTime::now();
    let mut removed = 0u64;
    for sha_entry in
        fs::read_dir(&incoming).with_context(|| format!("read {}", incoming.display()))?
    {
        let sha_entry = sha_entry?;
        let sha_path = sha_entry.path();
        if !sha_path.is_dir() {
            continue;
        }
        for session_entry in fs::read_dir(&sha_path)? {
            let session_entry = session_entry?;
            let session_path = session_entry.path();
            if !session_path.is_dir() {
                continue;
            }
            let Ok(meta) = session_entry.metadata() else {
                continue;
            };
            let Ok(modified) = meta.modified() else {
                continue;
            };
            let Ok(age) = now.duration_since(modified) else {
                continue;
            };
            if age.as_secs() >= max_age_secs {
                let _ = fs::remove_dir_all(&session_path);
                removed += 1;
            }
        }
        // Remove empty fingerprint folders left after the last session is gone.
        if fs::read_dir(&sha_path)?.next().is_none() {
            let _ = fs::remove_dir(&sha_path);
        }
    }
    Ok(removed)
}

/// Accept a 64-character lowercase hex SHA-256 fingerprint, or return `None`.
pub(crate) fn normalize_sha256(sha: &str) -> Option<String> {
    let s = sha.trim().to_ascii_lowercase();
    if s.len() != 64 || !s.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    Some(s)
}

/// Same as [`normalize_sha256`], but as an error when the value is invalid.
///
/// # Errors
///
/// Returns an error when `sha` is not 64 lowercase hex digits.
pub(crate) fn require_sha256(sha: &str) -> Result<String> {
    match normalize_sha256(sha) {
        Some(normalized) => Ok(normalized),
        None => Err(anyhow::anyhow!(
            "invalid sha256 (expected 64 lowercase hex digits)"
        )),
    }
}

/// SHA-256 fingerprint of the file at `path`, as 64 lowercase hex digits.
///
/// # Errors
///
/// Returns an error when the file cannot be opened or read.
pub(crate) fn hash_file(path: &Path) -> Result<String> {
    let file = open_nofollow_read(path)?;
    let mut reader = BufReader::new(file);
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 1024 * 1024];
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex_encode(&hasher.finalize()))
}

/// Find a stored file whose name (without extension) is `sha`.
///
/// When more than one match exists, the lexicographically first path is used so
/// the choice is stable across calls.
fn find_existing(assets_root: &Path, sha: &str) -> Option<PathBuf> {
    let shard = assets_root.join(&sha[..2]);
    if !shard.is_dir() {
        return None;
    }
    let entries = fs::read_dir(&shard).ok()?;
    let mut matches = Vec::new();
    for entry in entries {
        let path = entry.ok()?.path();
        if file_stem_equals(&path, sha) && is_regular_file(&path) {
            matches.push(path);
        }
    }
    matches.sort();
    matches.into_iter().next()
}

fn file_stem_equals(path: &Path, expected: &str) -> bool {
    match path.file_stem().and_then(|s| s.to_str()) {
        Some(stem) => stem == expected,
        None => false,
    }
}

fn mime_metadata_path(assets_root: &Path, sha: &str) -> PathBuf {
    assets_root.join(&sha[..2]).join(format!(".{sha}.mime"))
}

fn mime_for_path_or_sidecar(assets_root: &Path, path: &Path, sha: &str) -> Option<String> {
    match resolve_mime(None, path) {
        Some(mime) => Some(mime),
        None => read_mime_metadata(assets_root, sha),
    }
}

fn mime_for_existing_file(
    export_mime: Option<&str>,
    dest: &Path,
    source_mime: Option<String>,
) -> Option<String> {
    if let Some(mime) = export_mime
        && !mime.is_empty()
    {
        return Some(mime.to_owned());
    }
    resolve_mime(None, dest).or(source_mime)
}

fn read_mime_metadata(assets_root: &Path, sha: &str) -> Option<String> {
    let file = open_nofollow_read(&mime_metadata_path(assets_root, sha)).ok()?;
    let mut mime = String::new();
    file.take(1024).read_to_string(&mut mime).ok()?;
    let mime = mime.trim();
    if mime.is_empty() {
        None
    } else {
        Some(mime.to_owned())
    }
}

fn store_mime_metadata(assets_root: &Path, sha: &str, mime: &str) -> Result<()> {
    let mime = mime.trim();
    if mime.is_empty() {
        return Ok(());
    }
    let path = mime_metadata_path(assets_root, sha);
    if read_mime_metadata(assets_root, sha).is_some() {
        return Ok(());
    }
    let Some(parent) = path.parent() else {
        return Err(anyhow::anyhow!("asset MIME metadata has no parent"));
    };
    let mut temporary = tempfile::NamedTempFile::new_in(parent)
        .with_context(|| format!("create MIME metadata in {}", parent.display()))?;
    temporary.write_all(mime.as_bytes())?;
    temporary.flush()?;
    temporary.as_file().sync_all()?;
    match temporary.persist_noclobber(&path) {
        Ok(_) => Ok(()),
        Err(err) if err.error.kind() == std::io::ErrorKind::AlreadyExists => Ok(()),
        Err(err) => Err(err.error).with_context(|| format!("install {}", path.display())),
    }
}

fn path_relative_to(root: &Path, path: &Path) -> Result<String> {
    let relative = path.strip_prefix(root).with_context(|| {
        format!(
            "asset path {} is not under {}",
            path.display(),
            root.display()
        )
    })?;
    Ok(relative.to_string_lossy().replace('\\', "/"))
}

fn resolve_mime(export_mime: Option<&str>, source: &Path) -> Option<String> {
    if let Some(mime) = export_mime
        && !mime.is_empty()
    {
        return Some(mime.to_string());
    }
    let ext = source.extension().and_then(|e| e.to_str());
    guess_mime(ext)
}

/// Map a file extension to a MIME type.
///
/// Stored files are named only from their SHA-256 fingerprint, so they have no
/// extension. This mapping is the only chance to record what a file is: the
/// result is stored next to the file, returned to download callers, and written
/// to `attachments.mime_type`, which is what derived-media processing classifies
/// on. Extensions common in phone backups (voice notes, camera video, scans)
/// therefore need to be listed here.
fn guess_mime(ext: Option<&str>) -> Option<String> {
    let ext = ext?.to_ascii_lowercase();
    let mime = match ext.as_str() {
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "gif" => "image/gif",
        "heic" | "heif" => "image/heic",
        "webp" => "image/webp",
        "bmp" => "image/bmp",
        "tif" | "tiff" => "image/tiff",
        "mp4" | "m4v" => "video/mp4",
        "mov" => "video/quicktime",
        "3gp" | "3gpp" | "3g2" => "video/3gpp",
        "webm" => "video/webm",
        "mkv" => "video/x-matroska",
        "mpeg" | "mpg" => "video/mpeg",
        "avi" => "video/x-msvideo",
        "mp3" => "audio/mpeg",
        "m4a" | "aac" => "audio/mp4",
        "caf" => "audio/x-caf",
        "amr" => "audio/amr",
        "wav" => "audio/wav",
        "ogg" | "oga" => "audio/ogg",
        "pdf" => "application/pdf",
        "vcf" => "text/vcard",
        _ => return None,
    };
    Some(mime.to_string())
}

fn is_regular_file(path: &Path) -> bool {
    match fs::symlink_metadata(path) {
        Ok(meta) => meta.is_file() && !meta.file_type().is_symlink(),
        Err(_) => false,
    }
}

/// Fail unless `path` is a regular file, not a symlink.
fn ensure_regular_file(path: &Path) -> Result<()> {
    let meta = fs::symlink_metadata(path).with_context(|| format!("stat {}", path.display()))?;
    if meta.file_type().is_symlink() {
        bail!("refusing to follow symlink: {}", path.display());
    }
    if !meta.is_file() {
        bail!("asset source is not a file: {}", path.display());
    }
    Ok(())
}

/// Open `path` for reading without following a symlink.
fn open_nofollow_read(path: &Path) -> Result<File> {
    ensure_regular_file(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        return OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW)
            .open(path)
            .with_context(|| format!("open {}", path.display()));
    }
    #[cfg(not(unix))]
    {
        File::open(path).with_context(|| format!("open {}", path.display()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::process::Command;
    use std::sync::{Arc, Barrier};
    use std::time::{Duration, Instant};
    use tempfile::tempdir;

    fn files_named_with_sha(root: &Path, sha: &str) -> Vec<std::fs::DirEntry> {
        let shard = root.join(&sha[..2]);
        let mut installed = Vec::new();
        for entry in fs::read_dir(shard).unwrap() {
            let entry = entry.unwrap();
            let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
                continue;
            };
            if name.starts_with(sha) {
                installed.push(entry);
            }
        }
        installed
    }

    #[test]
    fn store_verified_replaces_corrupt_destination() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        let source = root.join("source.bin");
        fs::write(&source, b"valid-asset").unwrap();
        let sha = hash_file(&source).unwrap();
        let destination = root.join(shard_rel_path(&sha, ".bin"));
        fs::create_dir_all(destination.parent().unwrap()).unwrap();
        fs::write(&destination, b"corrupt").unwrap();

        let (stored, already_present) =
            store_verified(&source, &sha, root, None, false, false).unwrap();

        assert!(!already_present);
        assert_eq!(
            fs::read(root.join(stored.assets_path)).unwrap(),
            b"valid-asset"
        );
    }

    #[test]
    fn store_verified_concurrent_installers_leave_valid_destination() {
        let dir = tempdir().unwrap();
        let root = Arc::new(dir.path().to_path_buf());
        let source_a = root.join("source-a.bin");
        let source_b = root.join("source-b.dat");
        fs::write(&source_a, b"shared-asset").unwrap();
        fs::write(&source_b, b"shared-asset").unwrap();
        let sha = hash_file(&source_a).unwrap();
        let barrier = Arc::new(Barrier::new(2));

        let desired_path = root.join(shard_rel_path(&sha, ""));
        let installers: Vec<_> = [source_a, source_b]
            .into_iter()
            .enumerate()
            .map(|(index, source)| {
                let root = Arc::clone(&root);
                let sha = sha.clone();
                let barrier = Arc::clone(&barrier);
                let desired_path = desired_path.clone();
                std::thread::spawn(move || {
                    store_verified_inner(
                        &source,
                        &sha,
                        &root,
                        None,
                        false,
                        || {},
                        || {
                            barrier.wait();
                            if index == 1 {
                                let deadline = Instant::now() + Duration::from_secs(5);
                                while !desired_path.is_file() {
                                    assert!(
                                        Instant::now() < deadline,
                                        "timed out waiting for winning installer"
                                    );
                                    std::thread::sleep(Duration::from_millis(1));
                                }
                            }
                        },
                    )
                })
            })
            .collect();

        let mut results = Vec::new();
        for installer in installers {
            results.push(installer.join().unwrap().unwrap());
        }
        let newly_stored = results.iter().filter(|(_, present)| !present).count();
        assert_eq!(newly_stored, 1);
        assert_eq!(results[0].0.assets_path, results[1].0.assets_path);
        let installed = files_named_with_sha(root.as_path(), &sha);
        assert_eq!(installed.len(), 1);
        assert_eq!(
            fs::read(root.join(&results[0].0.assets_path)).unwrap(),
            b"shared-asset"
        );
    }

    #[test]
    fn store_verified_processes_share_one_mixed_extension_path() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        let source_a = root.join("process-a.bin");
        let source_b = root.join("process-b.dat");
        fs::write(&source_a, b"cross-process-asset").unwrap();
        fs::write(&source_b, b"cross-process-asset").unwrap();
        let sha = hash_file(&source_a).unwrap();
        let test_binary = std::env::current_exe().unwrap();

        let children: Vec<_> = [("a", source_a), ("b", source_b)]
            .into_iter()
            .map(|(worker, source)| {
                Command::new(&test_binary)
                    .args([
                        "--ignored",
                        "--exact",
                        "assets::tests::filesystem_install_worker",
                        "--nocapture",
                    ])
                    .env("ASSET_TEST_ROOT", root)
                    .env("ASSET_TEST_SOURCE", source)
                    .env("ASSET_TEST_SHA", &sha)
                    .env("ASSET_TEST_WORKER", worker)
                    .spawn()
                    .unwrap()
            })
            .collect();

        for mut child in children {
            assert!(child.wait().unwrap().success());
        }

        let result_a = fs::read_to_string(root.join("result-a")).unwrap();
        let result_b = fs::read_to_string(root.join("result-b")).unwrap();
        assert_eq!(result_a, result_b);
        assert!(Path::new(&result_a).extension().is_none());
        let installed = files_named_with_sha(root, &sha);
        assert_eq!(installed.len(), 1);
        assert_eq!(
            fs::read(root.join(result_a)).unwrap(),
            b"cross-process-asset"
        );
    }

    #[test]
    fn lookup_by_sha256_keeps_legacy_extension_paths_compatible() {
        let dir = tempdir().unwrap();
        let sha = sha256_hex(b"legacy-jpeg");
        let legacy = dir.path().join(shard_rel_path(&sha, ".jpg"));
        fs::create_dir_all(legacy.parent().unwrap()).unwrap();
        fs::write(&legacy, b"legacy-jpeg").unwrap();

        let stored = lookup_by_sha256(dir.path(), &sha).unwrap();

        assert_eq!(stored.assets_path, shard_rel_path(&sha, ".jpg"));
        assert_eq!(stored.mime_type.as_deref(), Some("image/jpeg"));
    }

    #[test]
    fn guess_mime_covers_phone_media_extensions() {
        for (ext, expected) in [
            ("amr", "audio/amr"),
            ("wav", "audio/wav"),
            ("ogg", "audio/ogg"),
            ("3gp", "video/3gpp"),
            ("3gpp", "video/3gpp"),
            ("webm", "video/webm"),
            ("mkv", "video/x-matroska"),
            ("avi", "video/x-msvideo"),
            ("mpg", "video/mpeg"),
            ("tiff", "image/tiff"),
            ("tif", "image/tiff"),
            ("bmp", "image/bmp"),
        ] {
            assert_eq!(
                guess_mime(Some(ext)).as_deref(),
                Some(expected),
                "unexpected MIME for .{ext}"
            );
        }
    }

    #[test]
    fn store_verified_records_mime_for_extensionless_media_blobs() {
        let dir = tempdir().unwrap();
        for (name, expected) in [
            ("voice.amr", "audio/amr"),
            ("memo.wav", "audio/wav"),
            ("clip.3gp", "video/3gpp"),
            ("scan.tiff", "image/tiff"),
        ] {
            let source = dir.path().join(name);
            fs::write(&source, name.as_bytes()).unwrap();
            let sha = sha256_hex(name.as_bytes());

            let (stored, _) =
                store_verified(&source, &sha, dir.path(), None, false, false).unwrap();

            assert!(Path::new(&stored.assets_path).extension().is_none());
            assert_eq!(stored.mime_type.as_deref(), Some(expected));
            // The fingerprint-only path has no extension, so serving relies on
            // the MIME file written next to the stored attachment.
            assert_eq!(
                lookup_by_sha256(dir.path(), &sha)
                    .unwrap()
                    .mime_type
                    .as_deref(),
                Some(expected)
            );
            assert_eq!(
                lookup_by_sha256_unverified(dir.path(), &sha)
                    .unwrap()
                    .mime_type
                    .as_deref(),
                Some(expected)
            );
        }
    }

    #[test]
    fn lookup_by_sha256_preserves_mime_for_extensionless_assets() {
        let dir = tempdir().unwrap();
        let source = dir.path().join("source.jpg");
        fs::write(&source, b"new-jpeg").unwrap();
        let sha = sha256_hex(b"new-jpeg");

        let (stored, _) = store_verified(&source, &sha, dir.path(), None, false, false).unwrap();
        let looked_up = lookup_by_sha256(dir.path(), &sha).unwrap();

        assert!(Path::new(&stored.assets_path).extension().is_none());
        assert_eq!(looked_up.mime_type.as_deref(), Some("image/jpeg"));
    }

    #[test]
    #[ignore = "helper launched by store_verified_processes_share_one_mixed_extension_path"]
    fn filesystem_install_worker() {
        let root = PathBuf::from(std::env::var_os("ASSET_TEST_ROOT").unwrap());
        let source = PathBuf::from(std::env::var_os("ASSET_TEST_SOURCE").unwrap());
        let sha = std::env::var("ASSET_TEST_SHA").unwrap();
        let worker = std::env::var("ASSET_TEST_WORKER").unwrap();

        let (stored, _) = store_verified_inner(
            &source,
            &sha,
            &root,
            None,
            false,
            || {},
            || {
                fs::write(root.join(format!("ready-{worker}")), b"ready").unwrap();
                let deadline = Instant::now() + Duration::from_secs(5);
                while !(root.join("ready-a").is_file() && root.join("ready-b").is_file()) {
                    assert!(
                        Instant::now() < deadline,
                        "timed out waiting for peer installer"
                    );
                    std::thread::sleep(Duration::from_millis(5));
                }
            },
        )
        .unwrap();
        fs::write(root.join(format!("result-{worker}")), stored.assets_path).unwrap();
    }

    #[test]
    fn store_verified_skips_temp_copy_on_valid_dedup() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        let source = root.join("dedup.bin");
        fs::write(&source, b"dedup-asset").unwrap();
        let sha = sha256_hex(b"dedup-asset");

        let (first, present) = store_verified(&source, &sha, root, None, false, false).unwrap();
        assert!(!present);

        let copied = std::cell::Cell::new(false);
        let (second, present) =
            store_verified_inner(&source, &sha, root, None, false, || copied.set(true), || {})
                .unwrap();

        assert!(present);
        assert_eq!(second.assets_path, first.assets_path);
        assert!(
            !copied.get(),
            "storing over a valid destination must not copy the source into a temporary blob"
        );
    }

    #[test]
    fn unverified_lookup_reads_no_content_while_verified_lookup_rejects_corruption() {
        let dir = tempdir().unwrap();
        let sha = sha256_hex(b"expected-bytes");
        let stored_path = dir.path().join(shard_rel_path(&sha, ""));
        fs::create_dir_all(stored_path.parent().unwrap()).unwrap();
        fs::write(&stored_path, b"corrupt-bytes").unwrap();

        let unverified = lookup_by_sha256_unverified(dir.path(), &sha)
            .expect("path lookup must not depend on file contents");
        assert_eq!(unverified.assets_path, shard_rel_path(&sha, ""));
        assert!(
            lookup_by_sha256(dir.path(), &sha).is_none(),
            "a file whose bytes do not match its fingerprint must not be reported as present"
        );
    }

    #[test]
    fn store_verified_hashes_source_before_deduplication() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        let mut src = tempfile::NamedTempFile::new().unwrap();
        src.write_all(b"hello-asset").unwrap();
        src.flush().unwrap();

        let sha = hash_file(src.path()).unwrap();
        let (first, present) =
            store_verified(src.path(), &sha, root, Some("text/plain"), false, false).unwrap();
        assert!(!present);
        assert_eq!(first.sha256, sha);
        assert!(src.path().is_file(), "non-consuming store must keep source");

        // A duplicate claim with different bytes must fail even when the valid
        // destination already exists.
        let mut other = tempfile::NamedTempFile::new().unwrap();
        other.write_all(b"different-bytes").unwrap();
        other.flush().unwrap();
        let err =
            store_verified(other.path(), &sha, root, Some("text/plain"), false, false).unwrap_err();
        assert!(err.to_string().contains("sha256 mismatch"));
        assert_eq!(
            fs::read(root.join(first.assets_path)).unwrap(),
            b"hello-asset"
        );
        assert!(lookup_by_sha256(root, &sha).is_some());
    }

    #[test]
    fn store_verified_persists_the_bytes_that_were_hashed() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        let source = root.join("mutable.bin");
        fs::write(&source, b"verified-bytes").unwrap();
        let sha = sha256_hex(b"verified-bytes");

        let (stored, present) = store_verified_inner(
            &source,
            &sha,
            root,
            None,
            false,
            || fs::write(&source, b"mutated-after-copy").unwrap(),
            || {},
        )
        .unwrap();

        assert!(!present);
        assert_eq!(
            fs::read(root.join(stored.assets_path)).unwrap(),
            b"verified-bytes"
        );
        assert_eq!(fs::read(source).unwrap(), b"mutated-after-copy");
    }

    #[test]
    fn store_verified_renames_same_filesystem_temp() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        let incoming = root.join(".incoming");
        fs::create_dir_all(&incoming).unwrap();
        let tmp = incoming.join("upload.part");
        fs::write(&tmp, b"rename-me").unwrap();
        let sha = hash_file(&tmp).unwrap();

        let (stored, present) = store_verified(
            &tmp,
            &sha,
            root,
            Some("application/octet-stream"),
            true,
            false,
        )
        .unwrap();
        assert!(!present);
        assert!(!tmp.exists(), "rename should consume the temp file");
        assert!(root.join(&stored.assets_path).is_file());
        assert_eq!(
            fs::read(root.join(&stored.assets_path)).unwrap(),
            b"rename-me"
        );
    }

    #[test]
    fn store_verified_rejects_symlink_source() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        let real = dir.path().join("real.bin");
        fs::write(&real, b"payload").unwrap();
        let link = dir.path().join("link.bin");
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&real, &link).unwrap();
            let sha = hash_file(&real).unwrap();
            let err = store_verified(&link, &sha, root, None, false, false).unwrap_err();
            assert!(
                err.to_string().contains("symlink"),
                "unexpected error: {err}"
            );
        }
    }

    #[test]
    fn gc_stale_incoming_removes_old_sessions() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        let session = root.join(".incoming").join("ab").join("deadbeef");
        fs::create_dir_all(&session).unwrap();
        fs::write(session.join("manifest.json"), b"{}").unwrap();
        let removed = gc_stale_incoming(root, 0).unwrap();
        assert_eq!(removed, 1);
        assert!(!session.exists());
    }
}
