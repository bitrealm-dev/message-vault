//! Timestamped session log file next to `export.ini`.

use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

use chrono::Local;

pub struct SessionLog {
    pub name: String,
    pub path: PathBuf,
    writer: BufWriter<File>,
}

impl SessionLog {
    /// Create (or truncate) a log file in `dir` (normally the directory that holds `export.ini`).
    pub fn new(dir: &Path) -> Self {
        let name = Local::now()
            .format("message-vault-io-%Y-%m-%d_%H%M%S.log")
            .to_string();
        let path = dir.join(&name);
        let file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&path)
            .unwrap_or_else(|_| {
                // Fallback: create in a temp location if the target dir isn't writable.
                let tmp = std::env::temp_dir().join(&name);
                OpenOptions::new()
                    .create(true)
                    .write(true)
                    .truncate(true)
                    .open(&tmp)
                    .expect("cannot create session log")
            });
        Self {
            name,
            path,
            writer: BufWriter::new(file),
        }
    }

    pub fn truncate(&mut self) {
        let file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&self.path)
            .unwrap_or_else(|_| {
                let _ = std::env::temp_dir().join(&self.name);
                OpenOptions::new()
                    .create(true)
                    .write(true)
                    .truncate(true)
                    .open(std::env::temp_dir().join(&self.name))
                    .expect("cannot create session log after truncate")
            });
        self.writer = BufWriter::new(file);
    }

    pub fn append(&mut self, line: &str) {
        let _ = writeln!(self.writer, "{line}");
        // Flush periodically so logs survive a crash, but not on every line.
    }

    /// Flush buffered writes to disk.
    pub fn flush(&mut self) {
        let _ = self.writer.flush();
    }
}
