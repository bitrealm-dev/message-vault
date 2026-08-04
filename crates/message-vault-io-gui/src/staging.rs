//! Staging directories for the guided Vault import workflow.
//!
//! Extraction writes JSONL + attachments into a timestamped folder beside
//! `export.ini`. Vault upload then reads from that same folder.

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
}
