//! Small shared helpers for IR readers / projectors.

use message_ir::IrAttachment;
use anyhow::{Context, Result};
use std::fs;
use std::path::Path;

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

/// Read attachment bytes from disk when the relative path exists.
///
/// Missing paths yield `Ok(None)`. IO failures return an error (strict) — callers
/// that want lenient behavior map with `.ok().flatten()`.
pub(crate) fn read_attachment_file(
    att: &IrAttachment,
    output_dir: &Path,
) -> Result<Option<Vec<u8>>> {
    let Some(rel) = att.path.as_deref() else {
        return Ok(None);
    };
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
}
