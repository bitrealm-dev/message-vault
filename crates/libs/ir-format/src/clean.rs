//! Remove leftover files from a previous export in the same directory.

use anyhow::{Context, Result, bail};
use mail::clean_previous_mail_output;
use std::fs;
use std::path::Path;

/// Sentinel file written into export directories so `clean_previous_ir_output` can
/// distinguish a real export directory from a user directory that was pointed at
/// by mistake.
pub const EXPORT_SENTINEL: &str = ".message-vault-export";

/// Write a sentinel file marking `output_dir` as an export target.
/// Callers should run this after `create_dir_all` on a fresh export.
pub fn write_export_sentinel(output_dir: &Path) -> Result<()> {
    fs::write(output_dir.join(EXPORT_SENTINEL), "")?;
    Ok(())
}

/// Delete previous CSV, JSON, JSON Lines, meta, `smses.xml`, temps, staged
/// attachments, and mail archives.
///
/// Only directories that contain the sentinel file `.message-vault-export`,
/// are empty, or already contain recognizable export files are cleaned. This
/// avoids deleting unrelated user files when the output path points at a
/// non-export directory by mistake.
///
/// # Errors
///
/// Returns an error when the directory cannot be read, a file cannot be
/// removed, or the directory looks like it is not an export folder.
pub fn clean_previous_ir_output(output_dir: &Path) -> Result<()> {
    if !output_dir.is_dir() {
        return Ok(());
    }
    let has_sentinel = output_dir.join(EXPORT_SENTINEL).is_file();
    if !has_sentinel {
        // Check if the directory looks like an export directory (contains files
        // matching known export patterns) or is empty. If neither, refuse to clean.
        let mut has_export_files = false;
        let mut has_other_files = false;
        for entry in
            fs::read_dir(output_dir).with_context(|| format!("read {}", output_dir.display()))?
        {
            let entry = entry?;
            let name = entry.file_name();
            let name = name.to_str().unwrap_or("");
            if is_export_artifact(name) {
                has_export_files = true;
            } else if name != EXPORT_SENTINEL {
                has_other_files = true;
            }
        }
        if !has_export_files && has_other_files {
            bail!(
                "output directory {} exists but does not appear to contain export files. \
                 Refusing to clean unrecognized content. Use an empty directory or one \
                 previously used for exports.",
                output_dir.display()
            );
        }
    }
    for entry in
        fs::read_dir(output_dir).with_context(|| format!("read {}", output_dir.display()))?
    {
        let path = entry?.path();
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if !path.is_file() {
            continue;
        }
        if is_export_artifact(name) {
            fs::remove_file(&path)
                .with_context(|| format!("remove previous {}", path.display()))?;
        }
    }
    // Drop staged attachments from previous runs. Files named by a SHA-256
    // fingerprint of their bytes would otherwise pile up when a new run does
    // not reuse them. Media transforms also reprocess every file under
    // attachments/, so leftover files can fail a later run. Callers copy the
    // attachments they need after this function.
    let attachments = output_dir.join("attachments");
    if attachments.is_dir() {
        fs::remove_dir_all(&attachments)
            .with_context(|| format!("remove previous {}", attachments.display()))?;
    } else if attachments.is_file() {
        fs::remove_file(&attachments)
            .with_context(|| format!("remove previous {}", attachments.display()))?;
    }
    clean_previous_mail_output(output_dir)?;
    // Write the sentinel so future runs know this is a safe export directory.
    let _ = fs::write(output_dir.join(EXPORT_SENTINEL), "");
    Ok(())
}

/// Returns true when `name` matches a known export artifact pattern.
fn is_export_artifact(name: &str) -> bool {
    name.ends_with(".csv")
        || name.ends_with(".csv.tmp")
        || name.ends_with(".meta.json")
        || name.ends_with(".meta.json.tmp")
        || name.ends_with(".json")
        || name.ends_with(".json.tmp")
        || name.ends_with(".jsonl")
        || name.ends_with(".jsonl.tmp")
        || name == "smses.xml"
        || name.ends_with(".xml.tmp")
        || name.ends_with(".xml.sbrbody")
}
