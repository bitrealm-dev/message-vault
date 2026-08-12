//! Thin helpers shared by exporter CLIs and in-process runners.
//!
//! Kept free of `anyhow` so GUI/core stay lightweight; callers map `String`
//! errors at the edge when needed.

use message_csv::DateRange;
use std::path::{Path, PathBuf};

/// Recursively walk `root`, collecting files that match `predicate`.
/// Skips symlinks (both files and directories). Directories are
/// traversed depth-first with no explicit depth limit (callers
/// should use this on trusted local input trees).
pub fn discover_files(
    root: &Path,
    predicate: &dyn Fn(&Path) -> bool,
) -> Result<Vec<PathBuf>, std::io::Error> {
    let mut out = Vec::new();
    discover_files_into(root, predicate, &mut out)?;
    Ok(out)
}

fn discover_files_into(
    dir: &Path,
    predicate: &dyn Fn(&Path) -> bool,
    out: &mut Vec<PathBuf>,
) -> Result<(), std::io::Error> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let ft = entry.file_type()?;
        if ft.is_symlink() {
            continue;
        }
        let path = entry.path();
        if ft.is_dir() {
            discover_files_into(&path, predicate, out)?;
        } else if ft.is_file() && predicate(&path) {
            out.push(path);
        }
    }
    Ok(())
}

/// Result of a successful exporter [`crate`]-style `run`: human-readable log lines.
#[derive(Debug, Default)]
pub struct RunResult {
    pub messages: Vec<String>,
}

/// Export run statistics. Per-exporter extension counters (PDU counts,
/// dedupe counts, etc.) are stored in the `extra` map.
#[derive(Debug, Default, Clone)]
pub struct ExportReport {
    pub conversations: u64,
    pub messages: u64,
    pub sent: u64,
    pub received: u64,
    pub skipped_invalid_date: u64,
    pub skipped_out_of_range: u64,
    pub duplicates_dropped: u64,
    pub attachments_saved: u64,
    /// Human-readable error/warning lines (capped by each exporter).
    pub errors: Vec<String>,
    /// Per-exporter extension counters keyed by name.
    pub extra: std::collections::BTreeMap<String, u64>,
}

impl ExportReport {
    /// Append one or more summary lines to `out`.
    pub fn summary_lines(&self, output: &std::path::Path, out: &mut Vec<String>) {
        out.push(format!(
            "Wrote {} export under {}",
            crate::name_stem(output.to_string_lossy().as_ref()),
            output.display()
        ));
        if self.skipped_invalid_date > 0 {
            out.push(format!(
                "  skipped {} invalid-date rows",
                self.skipped_invalid_date
            ));
        }
        if self.skipped_out_of_range > 0 {
            out.push(format!(
                "  skipped {} out-of-range rows",
                self.skipped_out_of_range
            ));
        }
        if self.duplicates_dropped > 0 {
            out.push(format!(
                "  dropped {} duplicate rows",
                self.duplicates_dropped
            ));
        }
        if self.attachments_saved > 0 {
            out.push(format!("  saved {} attachments", self.attachments_saved));
        }
        for (key, count) in &self.extra {
            out.push(format!("  {key}: {count}"));
        }
        for err in &self.errors {
            out.push(format!("  error: {err}"));
        }
    }
}

/// Print `RunResult` lines with the standard stdout/stderr split:
/// media/obfuscate/warning lines → stderr, summary lines → stdout.
pub fn print_result(result: &RunResult) {
    for line in &result.messages {
        if line.starts_with("Media:")
            || line.starts_with("  media ")
            || line.starts_with("Obfuscated ")
            || line.starts_with("warning:")
        {
            eprintln!("{line}");
        } else {
            println!("{line}");
        }
    }
}

/// Parse optional start/end date strings into a [`DateRange`].
pub fn parse_date_range(
    start_date: Option<&str>,
    end_date: Option<&str>,
) -> Result<DateRange, String> {
    DateRange::parse(start_date, end_date).map_err(|e| format!("invalid date range: {e}"))
}

/// Parse optional start/end dates with an optional timezone name (iMazing path).
pub fn parse_date_range_tz(
    start_date: Option<&str>,
    end_date: Option<&str>,
    timezone: Option<&str>,
) -> Result<DateRange, String> {
    DateRange::parse_optional_tz(start_date, end_date, timezone)
        .map_err(|e| format!("invalid date range: {e}"))
}

/// Filesystem-safe stem from a display name or handle (alnum / `-` / `_` / `+`).
pub fn name_stem(value: &str) -> String {
    let raw: String = value
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '+' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if raw.is_empty() || raw.chars().all(|c| c == '_') {
        "unknown".to_string()
    } else {
        raw
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn name_stem_sanitizes() {
        assert_eq!(name_stem("Alice Bob"), "Alice_Bob");
        assert_eq!(name_stem("+15555550100"), "+15555550100");
        assert_eq!(name_stem("!!!"), "unknown");
        assert_eq!(name_stem(""), "unknown");
    }

    #[test]
    fn parse_date_range_rejects_bad() {
        let err = parse_date_range(Some("not-a-date"), None).unwrap_err();
        assert!(err.starts_with("invalid date range:"));
    }

    #[test]
    fn discover_files_walks_and_filters() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join("sub")).unwrap();
        std::fs::write(root.join("a.xml"), b"<x/>").unwrap();
        std::fs::write(root.join("b.txt"), b"x").unwrap();
        std::fs::write(root.join("sub").join("c.xml"), b"<x/>").unwrap();
        std::fs::write(root.join("sub").join("d.eml"), b"").unwrap();
        let files = discover_files(root, &|p| {
            p.extension().and_then(|e| e.to_str()) == Some("xml")
        })
        .unwrap();
        let mut names: Vec<PathBuf> = files
            .iter()
            .map(|p| p.strip_prefix(root).unwrap().to_path_buf())
            .collect();
        names.sort();
        assert_eq!(
            names,
            vec![PathBuf::from("a.xml"), PathBuf::from("sub").join("c.xml")]
        );
    }

    #[test]
    fn discover_files_missing_root_errors() {
        let err = discover_files(Path::new("/no/such/dir"), &|_| true).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::NotFound);
    }
}
