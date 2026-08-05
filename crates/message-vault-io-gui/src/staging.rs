//! Timestamped directories for guided Vault import staging and Vault export.
//!
//! Import extraction writes JSONL + attachments into `staging-<importer>-…`
//! beside `export.ini`. Vault export writes into `export-<type>-…` under the
//! chosen parent (default: process working directory).

use std::path::{Path, PathBuf};

use chrono::{DateTime, Local};

/// Importer slug used in staging directory names for iPhone / iOS backups.
pub const IPHONE_IOS_IMPORTER: &str = "iphone-ios";

/// Importer slug used in staging directory names for macOS Messages (`chat.db`).
pub const MACOS_IMPORTER: &str = "macos";

/// Exporter type slug used in Vault Export directory names (iMessage covers iOS and macOS).
pub const IMESSAGE_EXPORTER: &str = "imessage";

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

/// Build `export-<type>-YYMMDD-HHMMSS` (same timestamp shape as import staging).
pub fn export_dir_name(exporter_type: &str, now: DateTime<Local>) -> String {
    format!("export-{}-{}", exporter_type, now.format("%y%m%d-%H%M%S"))
}

/// Resolve the export directory under `parent`.
pub fn export_dir_path(parent: &Path, exporter_type: &str, now: DateTime<Local>) -> PathBuf {
    parent.join(export_dir_name(exporter_type, now))
}

/// Parent folder for Vault Export when the UI field is empty: process cwd.
pub fn default_export_parent() -> PathBuf {
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
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
    fn export_dir_name_uses_type_and_local_timestamp() {
        let now = Local
            .with_ymd_and_hms(2026, 8, 3, 18, 5, 9)
            .single()
            .expect("valid local time");
        assert_eq!(
            export_dir_name(IMESSAGE_EXPORTER, now),
            "export-imessage-260803-180509"
        );
    }

    #[test]
    fn export_dir_path_joins_parent() {
        let now = Local
            .with_ymd_and_hms(2026, 1, 2, 3, 4, 5)
            .single()
            .expect("valid local time");
        let path = export_dir_path(Path::new("/tmp/runs"), IMESSAGE_EXPORTER, now);
        assert_eq!(
            path,
            PathBuf::from("/tmp/runs/export-imessage-260102-030405")
        );
    }
}
