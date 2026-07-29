use std::fs::{self, File};
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
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
/// Returns `(stored, already_present)`.
pub fn store_verified(
    source: &Path,
    claimed_sha256: &str,
    assets_root: &Path,
    export_mime: Option<&str>,
) -> Result<(StoredAsset, bool)> {
    let claimed = normalize_sha256(claimed_sha256)
        .ok_or_else(|| anyhow::anyhow!("invalid sha256 (expected 64 lowercase hex digits)"))?;
    if !source.is_file() {
        anyhow::bail!("asset source is not a file: {}", source.display());
    }
    let actual = hash_file(source)
        .with_context(|| format!("failed to hash {}", source.display()))?;
    if actual != claimed {
        anyhow::bail!("sha256 mismatch: claimed {claimed}, got {actual}");
    }

    if let Some(existing) = lookup_by_sha256(assets_root, &claimed) {
        return Ok((
            StoredAsset {
                mime_type: resolve_mime(export_mime, source).or(existing.mime_type),
                ..existing
            },
            true,
        ));
    }

    let ext = normalize_ext(source.extension().and_then(|e| e.to_str()));
    let rel = format!("{}/{}{}", &claimed[..2], claimed, ext);
    let dest = assets_root.join(&rel);
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    fs::copy(source, &dest).with_context(|| {
        format!(
            "failed to copy {} → {}",
            source.display(),
            dest.display()
        )
    })?;
    Ok((
        StoredAsset {
            sha256: claimed,
            assets_path: rel,
            mime_type: resolve_mime(export_mime, source),
        },
        false,
    ))
}

/// Hash `source` and store under `assets_root/<sha[0:2]>/<sha><ext>`.
/// If the blob already exists, skip the copy and count as deduped.
pub fn hash_and_store(
    source: &Path,
    assets_root: &Path,
    export_mime: Option<&str>,
    stats: &mut AssetStats,
) -> Result<Option<StoredAsset>> {
    if !source.is_file() {
        stats.missing += 1;
        return Ok(None);
    }

    let sha = hash_file(source)
        .with_context(|| format!("failed to hash {}", source.display()))?;
    let (stored, already) = store_verified(source, &sha, assets_root, export_mime)?;
    if already {
        stats.deduped += 1;
    } else {
        stats.copied += 1;
    }
    Ok(Some(stored))
}

fn normalize_sha256(sha: &str) -> Option<String> {
    let s = sha.trim().to_ascii_lowercase();
    if s.len() != 64 || !s.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    Some(s)
}

fn hash_file(path: &Path) -> Result<String> {
    let file = File::open(path)?;
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
    Ok(hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect())
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
                && p.is_file()
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
