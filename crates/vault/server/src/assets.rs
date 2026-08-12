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

pub fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn sha256_hex(data: &[u8]) -> String {
    hex_encode(&Sha256::digest(data))
}

pub fn shard_rel_path(sha256: &str, ext: &str) -> String {
    let ext = if ext == ".jpeg" { ".jpg" } else { ext };
    format!("{}/{}{}", &sha256[..2], sha256, ext)
}

/// Look up an already-stored blob by lowercase hex SHA-256.
pub fn lookup_by_sha256(assets_root: &Path, sha256: &str) -> Option<StoredAsset> {
    let sha = normalize_sha256(sha256)?;
    let existing = find_existing(assets_root, &sha)?;
    let assets_path = path_relative_to(assets_root, &existing).ok()?;
    Some(StoredAsset {
        sha256: sha,
        assets_path,
        mime_type: resolve_mime(None, &existing),
    })
}

/// Store `source` under `assets_root` using a caller-claimed SHA-256 (verified).
///
/// When `consume_source` is true (HTTP upload temps), prefers `rename` into place.
/// When false (export/import sources), always copies so the original file remains.
///
/// When `skip_hash` is true, the claimed sha256 is trusted without hashing.
/// Callers should pass `false`; the flag remains only for API stability.
///
/// Returns `(stored, already_present)`.
pub fn store_verified(
    source: &Path,
    claimed_sha256: &str,
    assets_root: &Path,
    export_mime: Option<&str>,
    consume_source: bool,
    skip_hash: bool,
) -> Result<(StoredAsset, bool)> {
    let claimed = require_sha256(claimed_sha256)?;
    ensure_regular_file(source)?;

    // Prefer existence check before hashing — duplicate PUTs skip a full SHA pass.
    if let Some(existing) = lookup_by_sha256(assets_root, &claimed) {
        return Ok((
            StoredAsset {
                mime_type: resolve_mime(export_mime, source).or(existing.mime_type),
                ..existing
            },
            true,
        ));
    }

    if skip_hash {
        // Trust the claimed sha256 — caller verified the assembled file size
        // matches the declared size. For large files this avoids an expensive
        // full-file SHA-256 pass on the server.
    } else {
        let actual =
            hash_file(source).with_context(|| format!("failed to hash {}", source.display()))?;
        if actual != claimed {
            anyhow::bail!("sha256 mismatch: claimed {claimed}, got {actual}");
        }
    }

    let ext = normalize_ext(source.extension().and_then(|e| e.to_str()));
    let rel = shard_rel_path(&claimed, &ext);
    let dest = assets_root.join(&rel);
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    let already = install_blob(source, &dest, consume_source)?;
    Ok((
        StoredAsset {
            sha256: claimed,
            assets_path: rel,
            mime_type: resolve_mime(export_mime, source),
        },
        already,
    ))
}

/// Install `source` at `dest` without following symlinks. Returns `true` when
/// `dest` already existed as a regular file. Uses create-new semantics so a
/// concurrent install cannot silently overwrite.
fn install_blob(source: &Path, dest: &Path, consume_source: bool) -> Result<bool> {
    if let Ok(meta) = fs::symlink_metadata(dest) {
        if meta.file_type().is_symlink() {
            bail!("refusing to install over symlink {}", dest.display());
        }
        if meta.is_file() {
            return Ok(true);
        }
        bail!(
            "asset destination exists and is not a regular file: {}",
            dest.display()
        );
    }

    if consume_source {
        match fs::rename(source, dest) {
            Ok(()) => return Ok(false),
            Err(err) => {
                if let Ok(meta) = fs::symlink_metadata(dest) {
                    if meta.file_type().is_symlink() {
                        bail!("refusing to install over symlink {}", dest.display());
                    }
                    if meta.is_file() {
                        let _ = fs::remove_file(source);
                        return Ok(true);
                    }
                }
                // Cross-device rename: fall through to create_new copy.
                let _ = err;
            }
        }
    }

    ensure_regular_file(source)?;
    let mut src =
        open_nofollow_read(source).with_context(|| format!("open source {}", source.display()))?;
    let mut dest_file = match OpenOptions::new().write(true).create_new(true).open(dest) {
        Ok(f) => f,
        Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => {
            if let Ok(meta) = fs::symlink_metadata(dest) {
                if meta.file_type().is_symlink() {
                    bail!("refusing to install over symlink {}", dest.display());
                }
                if meta.is_file() {
                    if consume_source {
                        let _ = fs::remove_file(source);
                    }
                    return Ok(true);
                }
            }
            return Err(err).with_context(|| format!("create {}", dest.display()));
        }
        Err(err) => return Err(err).with_context(|| format!("create {}", dest.display())),
    };
    std::io::copy(&mut src, &mut dest_file)
        .with_context(|| format!("failed to copy {} → {}", source.display(), dest.display()))?;
    dest_file.flush()?;
    if consume_source {
        let _ = fs::remove_file(source);
    }
    Ok(false)
}

/// Hash `source` and store under `assets_root/<sha[0:2]>/<sha><ext>`.
/// If the blob already exists, skip the copy and count as deduped.
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

/// Delete orphaned multipart staging under `{assets}/.incoming` older than `max_age_secs`.
///
/// Completed uploads remove their session dirs; abandoned ones can linger forever
/// without this sweep (called opportunistically from upload start).
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
        // Drop empty sha shard dirs.
        if fs::read_dir(&sha_path)?.next().is_none() {
            let _ = fs::remove_dir(&sha_path);
        }
    }
    Ok(removed)
}

pub(crate) fn normalize_sha256(sha: &str) -> Option<String> {
    let s = sha.trim().to_ascii_lowercase();
    if s.len() != 64 || !s.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    Some(s)
}

pub(crate) fn require_sha256(sha: &str) -> Result<String> {
    normalize_sha256(sha)
        .ok_or_else(|| anyhow::anyhow!("invalid sha256 (expected 64 lowercase hex digits)"))
}

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

fn normalize_ext(ext: Option<&str>) -> String {
    let Some(ext) = ext else {
        return String::new();
    };
    let ext = ext.to_ascii_lowercase();
    let ext = if ext == "jpeg" { "jpg" } else { &ext };
    format!(".{ext}")
}

fn find_existing(assets_root: &Path, sha: &str) -> Option<PathBuf> {
    let shard = assets_root.join(&sha[..2]);
    if !shard.is_dir() {
        return None;
    }
    let mut matches: Vec<_> = fs::read_dir(&shard)
        .ok()?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.file_stem()
                .and_then(|s| s.to_str())
                .is_some_and(|stem| stem == sha)
                && is_regular_file(p)
        })
        .collect();
    matches.sort();
    matches.into_iter().next()
}

fn path_relative_to(root: &Path, path: &Path) -> Result<String> {
    Ok(path
        .strip_prefix(root)
        .with_context(|| {
            format!(
                "asset path {} is not under {}",
                path.display(),
                root.display()
            )
        })?
        .to_string_lossy()
        .replace('\\', "/"))
}

fn resolve_mime(export_mime: Option<&str>, source: &Path) -> Option<String> {
    if let Some(mime) = export_mime.filter(|m| !m.is_empty()) {
        return Some(mime.to_string());
    }
    guess_mime(source.extension().and_then(|e| e.to_str()))
}

fn guess_mime(ext: Option<&str>) -> Option<String> {
    let ext = ext?.to_ascii_lowercase();
    let mime = match ext.as_str() {
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "gif" => "image/gif",
        "heic" | "heif" => "image/heic",
        "webp" => "image/webp",
        "mp4" | "m4v" => "video/mp4",
        "mov" => "video/quicktime",
        "mp3" => "audio/mpeg",
        "m4a" | "aac" => "audio/mp4",
        "caf" => "audio/x-caf",
        "pdf" => "application/pdf",
        "vcf" => "text/vcard",
        _ => return None,
    };
    Some(mime.to_string())
}

fn is_regular_file(path: &Path) -> bool {
    fs::symlink_metadata(path)
        .map(|m| m.is_file() && !m.file_type().is_symlink())
        .unwrap_or(false)
}

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
    use tempfile::tempdir;

    #[test]
    fn store_verified_skips_hash_when_already_present() {
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

        // Second store with a throwaway source file: existence short-circuit must win
        // without requiring the new bytes to match (lookup is by claimed SHA).
        let mut other = tempfile::NamedTempFile::new().unwrap();
        other.write_all(b"different-bytes").unwrap();
        other.flush().unwrap();
        let (second, present_again) =
            store_verified(other.path(), &sha, root, Some("text/plain"), false, false).unwrap();
        assert!(present_again);
        assert_eq!(second.sha256, sha);
        assert_eq!(second.assets_path, first.assets_path);
        assert!(lookup_by_sha256(root, &sha).is_some());
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
