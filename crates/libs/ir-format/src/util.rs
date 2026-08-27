//! Small shared helpers for reading and writing conversation documents.

use anyhow::{Context, Result};
use message_ir::{HandleType, IrAttachment};
use std::fs;
use std::path::Path;

/// Guess the handle type of a raw handle string when no type is known.
///
/// Rules (mirrored by the CSV `handle_type` cell and the EML/mbox reader):
/// - empty → [`HandleType::Other`]
/// - contains `@` → [`HandleType::Email`]
/// - digit-heavy string (digits, `+`, `-`, spaces, parentheses, dots, `#`,
///   `*`) → [`HandleType::Phone`]
/// - anything else → [`HandleType::Other`]
pub(crate) fn infer_handle_type(handle: &str) -> HandleType {
    let h = handle.trim();
    if h.is_empty() {
        return HandleType::Other;
    }
    if h.contains('@') {
        return HandleType::Email;
    }
    let has_digit = h.bytes().any(|b| b.is_ascii_digit());
    let all_phone_chars = h.bytes().all(|b| {
        b.is_ascii_digit() || matches!(b, b'+' | b'-' | b' ' | b'(' | b')' | b'.' | b'#' | b'*')
    });
    if has_digit && all_phone_chars {
        return HandleType::Phone;
    }
    HandleType::Other
}

/// Stem packaging suffix used for WhatsApp exports (`__whatsapp`).
pub(crate) fn packaging_suffix_from_stem(stem: &str) -> Option<String> {
    if stem.ends_with("__whatsapp") {
        Some("__whatsapp".into())
    } else {
        None
    }
}

/// Load attachment bytes from in-memory data or from `output_dir` + relative path.
///
/// Missing paths yield an empty buffer. IO failures return an error.
pub(crate) fn load_attachment_bytes_strict(
    att: &IrAttachment,
    output_dir: &Path,
) -> Result<Vec<u8>> {
    if let Some(b) = &att.bytes {
        return Ok(b.clone());
    }
    match read_attachment_file(att, output_dir) {
        Ok(Some(bytes)) => Ok(bytes),
        Ok(None) => Ok(Vec::new()),
        Err(err) => Err(err),
    }
}

/// Shared message prefix for unsafe-attachment-path errors. The ir-format path
/// check and the server's `safe_rel_path` both format their bail from this
/// const, and the server's import tests match it — keep the exact text stable.
pub const UNSAFE_ATTACHMENT_PATH_PREFIX: &str = "unsafe attachment path";

/// Read attachment bytes from disk when the relative path exists.
///
/// Missing paths yield `Ok(None)`. IO failures return an error (strict) — callers
/// that want lenient behavior map with `.ok().flatten()`.
///
/// The relative path is validated to prevent directory traversal (no `..` or
/// absolute paths). This defends against malicious CSV/JSON input crafted to
/// read arbitrary files during EML/MBOX/XML embedding.
pub(crate) fn read_attachment_file(
    att: &IrAttachment,
    output_dir: &Path,
) -> Result<Option<Vec<u8>>> {
    let Some(rel) = att.path.as_deref() else {
        return Ok(None);
    };
    // Reject paths that could escape the export folder.
    let rel_path = Path::new(rel);
    if rel_path.is_absolute() {
        anyhow::bail!("attachment path must be relative: {rel}");
    }
    for comp in rel_path.components() {
        if matches!(comp, std::path::Component::ParentDir) {
            anyhow::bail!("{UNSAFE_ATTACHMENT_PATH_PREFIX} (contains ..): {rel}");
        }
    }
    let path = output_dir.join(rel);
    if !path.is_file() {
        return Ok(None);
    }
    let bytes = fs::read(&path).with_context(|| format!("read {}", path.display()))?;
    Ok(Some(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use message_ir::IrAttachment;
    use std::fs;
    use std::io::Write;

    fn att_with_path(rel: &str) -> IrAttachment {
        IrAttachment {
            path: Some(rel.into()),
            original_name: None,
            mime_type: None,
            digest_sha256: None,
            is_sticker: false,
            transcription: None,
            sticker_effect: None,
            size_bytes: None,
            missing_reason: None,
            bytes: None,
        }
    }

    #[test]
    fn strict_prefers_in_memory_bytes() {
        let mut att = att_with_path("attachments/missing.bin");
        att.bytes = Some(vec![1, 2, 3]);
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(
            load_attachment_bytes_strict(&att, dir.path()).unwrap(),
            vec![1, 2, 3]
        );
    }

    #[test]
    fn strict_reads_relative_path() {
        let dir = tempfile::tempdir().unwrap();
        let rel = "attachments/a.bin";
        let path = dir.path().join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let mut f = fs::File::create(&path).unwrap();
        f.write_all(b"hello").unwrap();
        let att = att_with_path(rel);
        assert_eq!(
            load_attachment_bytes_strict(&att, dir.path()).unwrap(),
            b"hello"
        );
    }

    #[test]
    fn read_file_returns_none_for_missing() {
        let att = att_with_path("attachments/missing.bin");
        let dir = tempfile::tempdir().unwrap();
        assert!(read_attachment_file(&att, dir.path()).unwrap().is_none());
    }

    #[test]
    fn infer_handle_type_covers_email_phone_and_other() {
        use message_ir::HandleType;
        assert_eq!(infer_handle_type("alice@example.com"), HandleType::Email);
        assert_eq!(infer_handle_type("+15555550101"), HandleType::Phone);
        assert_eq!(infer_handle_type("1 (555) 555-0101"), HandleType::Phone);
        assert_eq!(infer_handle_type("alice"), HandleType::Other);
        assert_eq!(infer_handle_type(""), HandleType::Other);
    }
}
