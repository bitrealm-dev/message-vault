//! Timestamped session log file next to `export.ini`.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};

use chrono::Local;

pub struct SessionLog {
    pub name: String,
    pub path: PathBuf,
}

impl SessionLog {
    /// Create (or truncate) a log file in `dir` (normally the directory that holds `export.ini`).
    pub fn new(dir: &Path) -> Self {
        let name = Local::now()
            .format("message-vault-io-%Y-%m-%d_%H%M%S.log")
            .to_string();
        let path = dir.join(&name);
        let _ = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&path);
        Self { name, path }
    }

    pub fn truncate(&self) {
        let _ = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&self.path);
    }

    pub fn append(&self, line: &str) {
        if let Ok(mut file) = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
        {
            let _ = writeln!(file, "{line}");
        }
    }
}
