//! Locate and copy iMazing attachment files next to CSV exports.

use anyhow::{Context, Result};
use chrono::{Local, TimeZone};
use message_csv::AttachmentCell;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

/// Maximum directory depth for attachment discovery. iMazing export trees are
/// only a few levels deep; this bounds any pathological nesting.
const MAX_WALK_DEPTH: usize = 64;

/// One-time index of every file under the input tree.
///
/// Built once per export so attachment lookup does not re-walk the tree for
/// every attachment row.
pub(crate) struct AttachmentIndex {
    /// Lowercase file name -> paths sorted by path (exact-name lookup).
    by_name: HashMap<String, Vec<PathBuf>>,
    /// (lowercase file name, path) pairs sorted by path (suffix-match fallback).
    all: Vec<(String, PathBuf)>,
}

impl AttachmentIndex {
    /// Walk `root` once and index every regular file. When `root` is a file
    /// (single-CSV input), index its parent directory instead so sibling
    /// media is still discoverable.
    pub(crate) fn build(root: &Path) -> Self {
        let mut files = Vec::new();
        let base = if root.is_dir() {
            root
        } else {
            root.parent().unwrap_or(root)
        };
        collect_files(base, 0, &mut files);
        files.sort_by(|a, b| a.1.cmp(&b.1));
        let mut by_name: HashMap<String, Vec<PathBuf>> = HashMap::new();
        for (name, path) in &files {
            by_name.entry(name.clone()).or_default().push(path.clone());
        }
        AttachmentIndex { by_name, all: files }
    }

    /// Find a file whose name matches `csv_name` (exact, `_`/`-` prefixed, or
    /// suffix match), preferring the lexicographically first path.
    fn lookup(&self, csv_name: &str) -> Option<PathBuf> {
        if let Some(paths) = self.by_name.get(&csv_name.to_ascii_lowercase())
            && let Some(p) = paths.first()
        {
            return Some(p.clone());
        }
        for (name, path) in &self.all {
            if attachment_name_matches(name, csv_name) {
                return Some(path.clone());
            }
        }
        None
    }
}

/// Collect every regular file under `dir` (symlinks skipped, depth-bounded).
fn collect_files(dir: &Path, depth: usize, out: &mut Vec<(String, PathBuf)>) {
    if depth > MAX_WALK_DEPTH {
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        // Skip symlinks: following them can loop (stack overflow) and can reach
        // files outside the input tree.
        if file_type.is_symlink() {
            continue;
        }
        let path = entry.path();
        if file_type.is_dir() {
            collect_files(&path, depth + 1, out);
            continue;
        }
        if !file_type.is_file() {
            continue;
        }
        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            out.push((name.to_ascii_lowercase(), path));
        }
    }
}

/// Resolve a CSV attachment name into an [`AttachmentCell`].
///
/// Lookup order (unchanged):
/// 1. Files in the CSV's parent directory
/// 2. Indexed walk under the input tree
///
/// When `copy_attachments` is false, keep the CSV name only. On copy failure or
/// a missing file, fall back to the CSV name so the row still projects.
pub(crate) fn resolve_attachment_cell(
    csv_name: &str,
    attachment_type: &str,
    csv_parent: &Path,
    index: Option<&AttachmentIndex>,
    attachments_dir: &Path,
    copy_attachments: bool,
    message_secs: i64,
    attachments_saved: &mut u64,
) -> AttachmentCell {
    let mime = mime_hint(attachment_type, csv_name);
    let is_sticker = attachment_type.eq_ignore_ascii_case("sticker");
    if !copy_attachments {
        return AttachmentCell {
            path: Some(csv_name.to_string()),
            original_name: Some(csv_name.to_string()),
            mime_type: mime,
            digest_sha256: None,
            is_sticker,
            transcription: None,
            sticker_effect: None,
        };
    }
    match find_and_copy_attachment(
        csv_name,
        csv_parent,
        index,
        attachments_dir,
        message_secs,
        attachments_saved,
    ) {
        Ok(Some(rel_path)) => AttachmentCell {
            path: Some(rel_path),
            original_name: Some(csv_name.to_string()),
            mime_type: mime,
            digest_sha256: None,
            is_sticker,
            transcription: None,
            sticker_effect: None,
        },
        Ok(None) | Err(_) => AttachmentCell {
            path: Some(csv_name.to_string()),
            original_name: Some(csv_name.to_string()),
            mime_type: mime,
            digest_sha256: None,
            is_sticker,
            transcription: None,
            sticker_effect: None,
        },
    }
}

fn attachment_name_matches(disk_name: &str, csv_name: &str) -> bool {
    let disk = disk_name.to_ascii_lowercase();
    let csv = csv_name.to_ascii_lowercase();
    if disk == csv {
        return true;
    }
    disk.ends_with(&csv) || disk.ends_with(&format!("_{csv}")) || disk.ends_with(&format!("-{csv}"))
}

fn find_attachment_on_disk(
    csv_name: &str,
    csv_parent: &Path,
    index: &AttachmentIndex,
) -> Option<PathBuf> {
    if let Ok(entries) = fs::read_dir(csv_parent) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            if let Some(name) = path.file_name().and_then(|n| n.to_str())
                && attachment_name_matches(name, csv_name)
            {
                return Some(path);
            }
        }
    }
    index.lookup(csv_name)
}

fn find_and_copy_attachment(
    csv_name: &str,
    csv_parent: &Path,
    index: Option<&AttachmentIndex>,
    attachments_dir: &Path,
    message_secs: i64,
    attachments_saved: &mut u64,
) -> Result<Option<String>> {
    let Some(src) = index.and_then(|i| find_attachment_on_disk(csv_name, csv_parent, i)) else {
        return Ok(None);
    };
    let digest_hex = stream_sha256(&src)?;
    let digest_prefix = &digest_hex[..16.min(digest_hex.len())];
    let ext = src
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| format!(".{e}"))
        .unwrap_or_default();
    let date_prefix = Local
        .timestamp_opt(message_secs, 0)
        .single()
        .map(|t| t.format("%Y%m%d_%H%M%S").to_string())
        .unwrap_or_else(|| message_secs.to_string());
    let name = format!("{date_prefix}-{digest_prefix}{ext}");
    let dest = attachments_dir.join(&name);
    if !dest.exists() {
        fs::copy(&src, &dest).with_context(|| {
            format!(
                "copy {} to {}",
                src.display(),
                dest.display()
            )
        })?;
        *attachments_saved += 1;
    }
    Ok(Some(format!("attachments/{name}")))
}

/// Stream a file through SHA-256 in 64 KB chunks (no full read into memory).
fn stream_sha256(path: &Path) -> Result<String> {
    let mut file = fs::File::open(path).with_context(|| format!("open {}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = file
            .read(&mut buf)
            .with_context(|| format!("read {}", path.display()))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex::encode(hasher.finalize()))
}

fn mime_hint(attachment_type: &str, filename: &str) -> Option<String> {
    let t = attachment_type.trim().to_ascii_lowercase();
    if !t.is_empty() {
        return Some(match t.as_str() {
            "image" => "image/jpeg".into(),
            "video" => "video/mp4".into(),
            "audio" => "audio/mpeg".into(),
            "gif" => "image/gif".into(),
            "sticker" => "image/webp".into(),
            other => other.to_string(),
        });
    }
    let lower = filename.to_ascii_lowercase();
    if lower.ends_with(".png") {
        Some("image/png".into())
    } else if lower.ends_with(".jpg") || lower.ends_with(".jpeg") {
        Some("image/jpeg".into())
    } else if lower.ends_with(".gif") {
        Some("image/gif".into())
    } else if lower.ends_with(".heic") {
        Some("image/heic".into())
    } else if lower.ends_with(".mp4") || lower.ends_with(".mov") {
        Some("video/mp4".into())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attachment_name_matches_suffix_and_separators() {
        assert!(attachment_name_matches("IMG_1234.jpg", "1234.jpg"));
        assert!(attachment_name_matches("photo_abc.jpg", "abc.jpg"));
        assert!(attachment_name_matches("photo-abc.jpg", "abc.jpg"));
        assert!(!attachment_name_matches("other.jpg", "abc.jpg"));
    }

    #[test]
    fn index_finds_exact_and_suffix_matches() {
        let dir = tempfile::tempdir().unwrap();
        let chat = dir.path().join("chat");
        fs::create_dir_all(&chat).unwrap();
        fs::write(chat.join("ABC123_image000000.jpg"), b"jpeg").unwrap();
        fs::write(chat.join("notes.txt"), b"x").unwrap();
        let index = AttachmentIndex::build(dir.path());
        assert_eq!(
            index.lookup("image000000.jpg"),
            Some(chat.join("ABC123_image000000.jpg"))
        );
        assert_eq!(index.lookup("notes.txt"), Some(chat.join("notes.txt")));
        assert_eq!(index.lookup("missing.jpg"), None);
    }

    #[cfg(unix)]
    #[test]
    fn index_skips_symlink_loops() {
        use std::os::unix::fs::symlink;
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a");
        fs::create_dir_all(&a).unwrap();
        // Self-referential symlink: must be skipped, not followed forever.
        symlink(&a, a.join("loop")).unwrap();
        fs::write(a.join("photo.jpg"), b"jpeg").unwrap();
        let index = AttachmentIndex::build(dir.path());
        assert_eq!(index.lookup("photo.jpg"), Some(a.join("photo.jpg")));
    }
}
