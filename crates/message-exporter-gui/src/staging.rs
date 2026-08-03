//! Staging directories for the guided Vault import workflow.
//!
//! Extraction writes JSONL + attachments into a timestamped folder beside
//! `export.ini`. Vault upload then reads from that same folder.

use std::fs;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Local};

/// Importer slug used in staging directory names for iPhone / iOS backups.
pub const IPHONE_IOS_IMPORTER: &str = "iphone-ios";

/// Build `staging-<importer>-YYMMDD-HHMMSS`.
pub fn staging_dir_name(importer: &str, now: DateTime<Local>) -> String {
    format!("staging-{}-{}", importer, now.format("%y%m%d-%H%M%S"))
}

/// Resolve the staging directory next to `export.ini`.
pub fn staging_dir_path(export_ini_path: &Path, importer: &str, now: DateTime<Local>) -> PathBuf {
    let parent = export_ini_path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."));
    parent.join(staging_dir_name(importer, now))
}

/// Remove a staging directory after a successful import when the user opted in.
///
/// Failures and cancellations always retain staging data. When
/// `delete_after_success` is false (the default), the directory is kept even
/// after a successful upload.
pub fn maybe_cleanup_staging(
    path: &Path,
    delete_after_success: bool,
    succeeded: bool,
) -> Result<bool, String> {
    if !succeeded || !delete_after_success {
        return Ok(false);
    }
    if !path.exists() {
        return Ok(false);
    }
    fs::remove_dir_all(path).map_err(|error| {
        format!(
            "Could not delete staging directory {}: {error}",
            path.display()
        )
    })?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn staging_dir_name_uses_importer_and_local_timestamp() {
        let now = Local
            .with_ymd_and_hms(2026, 8, 3, 18, 5, 9)
            .single()
            .expect("valid local time");
        assert_eq!(
            staging_dir_name(IPHONE_IOS_IMPORTER, now),
            "staging-iphone-ios-260803-180509"
        );
    }

    #[test]
    fn staging_dir_path_uses_export_ini_parent() {
        let now = Local
            .with_ymd_and_hms(2026, 1, 2, 3, 4, 5)
            .single()
            .expect("valid local time");
        let path = staging_dir_path(Path::new("/tmp/settings/export.ini"), "iphone-ios", now);
        assert_eq!(
            path,
            PathBuf::from("/tmp/settings/staging-iphone-ios-260102-030405")
        );
    }

    #[test]
    fn cleanup_keeps_staging_by_default() {
        let dir = tempfile_dir("keep-default");
        assert!(!maybe_cleanup_staging(&dir, false, true).unwrap());
        assert!(dir.is_dir());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn cleanup_deletes_only_after_successful_opt_in() {
        let dir = tempfile_dir("delete-success");
        assert!(maybe_cleanup_staging(&dir, true, true).unwrap());
        assert!(!dir.exists());
    }

    #[test]
    fn cleanup_retains_staging_after_failure_even_when_opted_in() {
        let dir = tempfile_dir("retain-failure");
        assert!(!maybe_cleanup_staging(&dir, true, false).unwrap());
        assert!(dir.is_dir());
        let _ = fs::remove_dir_all(&dir);
    }

    fn tempfile_dir(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "message-exporter-gui-staging-{}-{}-{}",
            label,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }
}
