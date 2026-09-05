//! What one read of a Messages source needs.

use std::path::{Path, PathBuf};

use imessage_database::{
    tables::table::DEFAULT_PATH_IOS,
    util::{platform::Platform, query_context::QueryContext},
};
use imessage_reader_protocol::{Event, ExportRequest, Progress, Source};

use crate::log::{emit, emit_log};

/// Parsed options for one read.
#[derive(Debug)]
pub(crate) struct ReaderOptions {
    pub db_path: PathBuf,
    pub attachment_root: Option<String>,
    pub query_context: QueryContext,
    pub use_caller_id: bool,
    pub platform: Platform,
    pub cleartext_password: Option<String>,
    pub contacts_path: Option<PathBuf>,
    /// Where decrypted files go. The app owns this folder.
    pub scratch_dir: PathBuf,
}

impl ReaderOptions {
    /// Options for a full export.
    pub fn from_export(request: ExportRequest) -> Self {
        let scratch_dir = request.scratch_dir.unwrap_or_else(std::env::temp_dir);
        Self {
            attachment_root: request.attachment_root,
            contacts_path: request.contacts_path,
            use_caller_id: request.use_caller_id,
            scratch_dir,
            ..Self::from_source(request.source)
        }
    }

    /// Options for opening the source and nothing more.
    pub fn from_source(source: Source) -> Self {
        Self {
            db_path: source.db_path,
            attachment_root: None,
            query_context: QueryContext::default(),
            use_caller_id: true,
            platform: match source.platform {
                imessage_reader_protocol::Platform::MacOs => Platform::macOS,
                imessage_reader_protocol::Platform::Ios => Platform::iOS,
            },
            cleartext_password: source.backup_password,
            contacts_path: None,
            scratch_dir: std::env::temp_dir(),
        }
    }

    /// Messages database path for the selected platform.
    pub fn get_db_path(&self) -> PathBuf {
        match self.platform {
            Platform::iOS => self.db_path.join(DEFAULT_PATH_IOS),
            Platform::macOS => self.db_path.clone(),
        }
    }

    /// The folder decrypted files are written to.
    pub fn scratch_dir(&self) -> &Path {
        &self.scratch_dir
    }

    /// Send one log line to the app.
    pub fn emit_log(&self, line: impl AsRef<str>) {
        emit_log(line);
    }

    /// Announce a numbered setup step (decrypting a backup, caching a table):
    /// a `[step/total] label...` log line for people and a
    /// [`Progress::Setup`] event for the progress bar.
    pub fn setup_step(&self, step: u64, total: u64, label: &str) {
        self.emit_log(format!("  [{step}/{total}] {label}..."));
        emit(&Event::Progress(Progress::Setup {
            label: label.to_string(),
            step,
            total,
        }));
    }
}
