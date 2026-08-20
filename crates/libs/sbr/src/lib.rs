//! SMS Backup & Restore (SyncTech) XML codec.
//!
//! Writers produce a single backup file (`smses.xml`) with root
//! `<smses count="N">`. See the [SMS Backup & Restore XML output](https://vault.bitrealm.dev/developer/formats/sms-backup-restore-xml/).

mod read;

pub use read::{
    AttachmentBlob, ConversationKind, ParseStats, Record, SourceFields, infer_owner_phones,
    parse_file,
};

use anyhow::{Context, Result};
use base64::Engine;
use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

/// One `<sms>` or `<mms>` element ready to serialize.
#[derive(Debug, Clone)]
pub enum SbrMessage {
    Sms {
        attrs: BTreeMap<String, String>,
    },
    Mms {
        attrs: BTreeMap<String, String>,
        parts: Vec<BTreeMap<String, String>>,
        addrs: Vec<BTreeMap<String, String>>,
    },
}

impl SbrMessage {
    pub fn sms(attrs: BTreeMap<String, String>) -> Self {
        Self::Sms { attrs }
    }

    pub fn mms(
        attrs: BTreeMap<String, String>,
        parts: Vec<BTreeMap<String, String>>,
        addrs: Vec<BTreeMap<String, String>>,
    ) -> Self {
        Self::Mms {
            attrs,
            parts,
            addrs,
        }
    }
}

/// Streaming writer for a SyncTech-style `smses.xml` backup.
///
/// Message bodies are buffered in a sidecar temp file; [`finish`](Self::finish)
/// writes the final document with the correct `count`.
pub struct SbrBackupWriter {
    path: PathBuf,
    body_path: PathBuf,
    body: BufWriter<File>,
    count: u64,
}

impl SbrBackupWriter {
    /// Create a new backup at `path` (typically `…/smses.xml`).
    pub fn create(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
        }
        let body_path = path.with_extension("xml.sbrbody");
        if body_path.exists() {
            fs::remove_file(&body_path)
                .with_context(|| format!("remove stale {}", body_path.display()))?;
        }
        let body = BufWriter::new(
            File::create(&body_path).with_context(|| format!("create {}", body_path.display()))?,
        );
        Ok(Self {
            path: path.to_path_buf(),
            body_path,
            body,
            count: 0,
        })
    }

    pub fn count(&self) -> u64 {
        self.count
    }

    pub fn write_message(&mut self, msg: &SbrMessage) -> Result<()> {
        match msg {
            SbrMessage::Sms { attrs } => {
                write_empty_element(&mut self.body, "sms", attrs)?;
            }
            SbrMessage::Mms {
                attrs,
                parts,
                addrs,
            } => {
                write_mms(&mut self.body, attrs, parts, addrs)?;
            }
        }
        self.count += 1;
        Ok(())
    }

    /// Finalize `count`, close `</smses>`, and replace `path`.
    pub fn finish(mut self) -> Result<PathBuf> {
        self.body.flush().context("flush sbr body")?;
        drop(self.body);

        let body_bytes = fs::read(&self.body_path)
            .with_context(|| format!("read {}", self.body_path.display()))?;

        let mut tmp = self.path.clone();
        tmp.set_extension("xml.tmp");
        {
            let mut out = BufWriter::new(
                File::create(&tmp).with_context(|| format!("create {}", tmp.display()))?,
            );
            writeln!(
                out,
                r#"<?xml version='1.0' encoding='UTF-8' standalone='yes' ?>"#
            )?;
            writeln!(out, r#"<smses count="{}">"#, self.count)?;
            out.write_all(&body_bytes)?;
            if !body_bytes.is_empty() && !body_bytes.ends_with(b"\n") {
                writeln!(out)?;
            }
            writeln!(out, "</smses>")?;
            out.flush()?;
        }
        fs::rename(&tmp, &self.path)
            .with_context(|| format!("rename {} → {}", tmp.display(), self.path.display()))?;
        let _ = fs::remove_file(&self.body_path);
        Ok(self.path)
    }
}

/// Escape a value for use inside a double-quoted XML attribute.
fn escape_attr(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            c => out.push(c),
        }
    }
    out
}

/// Standard base64 for MMS `data` attributes.
pub fn encode_part_data(bytes: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

fn write_attrs(w: &mut impl Write, attrs: &BTreeMap<String, String>) -> Result<()> {
    for (k, v) in attrs {
        write!(w, r#" {}="{}""#, k, escape_attr(v))?;
    }
    Ok(())
}

fn write_empty_element(
    w: &mut impl Write,
    name: &str,
    attrs: &BTreeMap<String, String>,
) -> Result<()> {
    write!(w, "  <{name}")?;
    write_attrs(w, attrs)?;
    writeln!(w, " />")?;
    Ok(())
}

fn write_mms(
    w: &mut impl Write,
    attrs: &BTreeMap<String, String>,
    parts: &[BTreeMap<String, String>],
    addrs: &[BTreeMap<String, String>],
) -> Result<()> {
    write!(w, "  <mms")?;
    write_attrs(w, attrs)?;
    writeln!(w, ">")?;
    writeln!(w, "    <parts>")?;
    for part in parts {
        write!(w, "      <part")?;
        write_attrs(w, part)?;
        writeln!(w, " />")?;
    }
    writeln!(w, "    </parts>")?;
    writeln!(w, "    <addrs>")?;
    for addr in addrs {
        write!(w, "      <addr")?;
        write_attrs(w, addr)?;
        writeln!(w, " />")?;
    }
    writeln!(w, "    </addrs>")?;
    writeln!(w, "  </mms>")?;
    Ok(())
}

/// Default filename for a full-backup projection.
const DEFAULT_BACKUP_FILENAME: &str = "smses.xml";

pub fn default_backup_path(output_dir: &Path) -> PathBuf {
    output_dir.join(DEFAULT_BACKUP_FILENAME)
}

/// Insert `key` only when it is not already present.
pub fn ensure_attr(attrs: &mut BTreeMap<String, String>, key: &str, value: impl Into<String>) {
    attrs.entry(key.to_string()).or_insert_with(|| value.into());
}

/// Overwrite an attribute (IR authoritative fields like date/body).
pub fn set_attr(attrs: &mut BTreeMap<String, String>, key: &str, value: impl Into<String>) {
    attrs.insert(key.to_string(), value.into());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_sms_and_mms_with_count() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("smses.xml");
        let mut w = SbrBackupWriter::create(&path).unwrap();

        let mut sms = BTreeMap::new();
        sms.insert("protocol".into(), "0".into());
        sms.insert("address".into(), "+15555550101".into());
        sms.insert("date".into(), "1400773261000".into());
        sms.insert("type".into(), "1".into());
        sms.insert("body".into(), r#"hello & "world""#.into());
        sms.insert("read".into(), "1".into());
        w.write_message(&SbrMessage::sms(sms)).unwrap();

        let mut mms_attrs = BTreeMap::new();
        mms_attrs.insert("date".into(), "1400773400000".into());
        mms_attrs.insert("msg_box".into(), "1".into());
        mms_attrs.insert("address".into(), "+15555550101".into());
        let mut part = BTreeMap::new();
        part.insert("seq".into(), "0".into());
        part.insert("ct".into(), "text/plain".into());
        part.insert("text".into(), "mms hi".into());
        let mut part2 = BTreeMap::new();
        part2.insert("seq".into(), "1".into());
        part2.insert("ct".into(), "image/jpeg".into());
        part2.insert("name".into(), "pic.jpg".into());
        part2.insert("data".into(), encode_part_data(b"xxxx"));
        let mut addr = BTreeMap::new();
        addr.insert("address".into(), "+15555550101".into());
        addr.insert("type".into(), "137".into());
        w.write_message(&SbrMessage::mms(mms_attrs, vec![part, part2], vec![addr]))
            .unwrap();

        let out = w.finish().unwrap();
        let text = fs::read_to_string(&out).unwrap();
        assert!(text.contains(r#"<smses count="2">"#));
        assert!(text.contains("hello &amp; &quot;world&quot;"));
        assert!(text.contains("<mms "));
        assert!(text.contains(r#"ct="image/jpeg""#));
        assert!(text.contains("</smses>"));
        assert!(!path.with_extension("xml.sbrbody").exists());
    }

    #[test]
    fn empty_backup_still_valid() {
        let tmp = tempfile::tempdir().unwrap();
        let path = default_backup_path(tmp.path());
        let w = SbrBackupWriter::create(&path).unwrap();
        w.finish().unwrap();
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.contains(r#"count="0""#));
        assert!(text.contains("</smses>"));
    }
}
