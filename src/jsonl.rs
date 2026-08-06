use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

use anyhow::{Context, Result};

use crate::models::{self, ExportRecord};

/// Read a message-ir JSONL conversation file into import records.
pub fn read_records(path: &Path) -> Result<Vec<ExportRecord>> {
    let file = File::open(path).with_context(|| format!("failed to open {}", path.display()))?;
    let reader = BufReader::new(file);
    let mut lines = Vec::new();

    for (line_no, line) in reader.lines().enumerate() {
        let line = line.with_context(|| {
            format!("failed to read line {} of {}", line_no + 1, path.display())
        })?;
        lines.push(line);
    }

    models::parse_ir_lines(lines)
        .with_context(|| format!("failed to parse message-ir JSONL in {}", path.display()))
}
