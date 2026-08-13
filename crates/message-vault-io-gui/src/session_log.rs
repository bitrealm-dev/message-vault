//! Timestamped session log file next to `export.ini`.

use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

use chrono::Local;

/// Buffered log file that records one GUI session (or one job, after truncate).
pub struct SessionLog {
    pub name: String,
    pub path: PathBuf,
    writer: BufWriter<File>,
}

/// Create or replace a file at `path` (truncate if it already exists).
fn open_truncated(path: &Path) -> std::io::Result<File> {
    OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(path)
}

impl SessionLog {
    /// Create (or replace) a log file in `dir` (normally the directory that holds `export.ini`).
    ///
    /// If that directory is not writable, the file is created in the system temp directory.
    /// `path` still records the original location even when the temp fallback is used.
    pub fn new(dir: &Path) -> Self {
        let name = Local::now()
            .format("message-vault-io-%Y-%m-%d_%H%M%S.log")
            .to_string();
        let path = dir.join(&name);
        let file = open_truncated(&path).unwrap_or_else(|_| {
            let tmp = std::env::temp_dir().join(&name);
            open_truncated(&tmp).expect("cannot create session log")
        });
        Self {
            name,
            path,
            writer: BufWriter::new(file),
        }
    }

    /// Empty the current log file so the next job starts with a blank log.
    ///
    /// If the original path is not writable, a file with the same name is opened
    /// in the system temp directory. `self.path` is left unchanged.
    pub fn truncate(&mut self) {
        let file = open_truncated(&self.path).unwrap_or_else(|_| {
            let tmp = std::env::temp_dir().join(&self.name);
            open_truncated(&tmp).expect("cannot create session log after truncate")
        });
        self.writer = BufWriter::new(file);
    }

    /// Write one line. Bytes stay in the buffer until [`Self::flush`].
    pub fn append(&mut self, line: &str) {
        let _ = writeln!(self.writer, "{line}");
        // Do not flush on every line. Periodic flush keeps logs after a crash
        // without paying for a disk write on each message.
    }

    /// Write buffered bytes to disk.
    pub fn flush(&mut self) {
        let _ = self.writer.flush();
    }
}
