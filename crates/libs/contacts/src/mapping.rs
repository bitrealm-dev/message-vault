//! Incorrect EML export name → (normalized handle, handle type).

use crate::name::{collapse_inner_whitespace, normalize_name_key};
use anyhow::{Context, Result};
use message_ir::HandleType;
use phone::sanitize_number;
use std::collections::HashMap;
use std::fs::File;
use std::path::Path;

/// Incorrect EML export name → (normalized handle, handle type).
#[derive(Debug, Default, Clone)]
pub struct NameMapping {
    /// Normalized incorrect name → (normalized handle, handle type).
    incorrect_to_handle: HashMap<String, (String, HandleType)>,
}

impl NameMapping {
    /// Construct an empty mapping.
    pub fn empty() -> Self {
        Self {
            incorrect_to_handle: HashMap::new(),
        }
    }

    /// Load `Handle,HandleType,Incorrect Name` CSV (column order flexible; header required).
    ///
    /// # Errors
    ///
    /// Returns an error when the file cannot be opened or read, or when a
    /// required header (`Handle` / `Incorrect Name`) is missing.
    pub fn load(path: &Path) -> Result<Self> {
        let file =
            File::open(path).with_context(|| format!("open name mapping {}", path.display()))?;
        let mut rdr = csv::ReaderBuilder::new()
            .flexible(true)
            .has_headers(true)
            .from_reader(file);
        let header_l: Vec<String> = rdr
            .headers()
            .with_context(|| format!("read name mapping header in {}", path.display()))?
            .iter()
            .map(csv_header_key)
            .collect();

        let handle_idx = header_l.iter().position(|h| h == "handle" || h == "phone");
        let type_idx = header_l
            .iter()
            .position(|h| h == "handle type" || h == "handletype");
        let incorrect_idx = header_l
            .iter()
            .position(|h| h == "incorrect name" || h == "incorrectname" || h == "incorrect");

        let (Some(handle_idx), Some(incorrect_idx)) = (handle_idx, incorrect_idx) else {
            anyhow::bail!(
                "name mapping CSV {} missing required header Handle,Incorrect Name",
                path.display()
            );
        };

        let mut mapping = Self::empty();
        for (idx, rec) in rdr.records().enumerate() {
            let rec = rec.with_context(|| format!("read name mapping line {}", idx + 2))?;
            let handle_raw = rec.get(handle_idx).map(str::trim).unwrap_or("");
            let incorrect = rec
                .get(incorrect_idx)
                .map(|s| collapse_inner_whitespace(s.trim()))
                .unwrap_or_default();
            if handle_raw.is_empty() || incorrect.is_empty() {
                continue;
            }

            // Infer handle type from column or default to Phone
            let handle_type = type_idx
                .and_then(|i| rec.get(i))
                .map(|s| HandleType::parse(s.trim()))
                .unwrap_or(HandleType::Phone);

            if handle_type == HandleType::Phone && sanitize_number(handle_raw).is_none() {
                continue;
            }
            // Shared guarded policy (never fabricates a `+0…` value); the
            // note is produced server-side, where the handles table stores it.
            let normalized = phone::normalize_typed_handle(handle_raw, handle_type).0;

            let key = normalize_name_key(&incorrect);
            if key.is_empty() {
                continue;
            }
            mapping
                .incorrect_to_handle
                .entry(key)
                .or_insert((normalized, handle_type));
        }
        Ok(mapping)
    }

    /// Load from a path option, returning the path when loaded.
    ///
    /// # Errors
    ///
    /// Returns an error when the file cannot be opened or read, or when a
    /// required header (`Handle` / `Incorrect Name`) is missing.
    pub fn load_optional(path: Option<&Path>) -> Result<(Self, Option<std::path::PathBuf>)> {
        match path {
            Some(path) => Ok((Self::load(path)?, Some(path.to_path_buf()))),
            None => Ok((Self::empty(), None)),
        }
    }

    /// If `eml_name` is an incorrect export name, return (normalized handle, type).
    pub fn handle_for_incorrect_name(&self, eml_name: &str) -> Option<&(String, HandleType)> {
        let key = normalize_name_key(eml_name);
        if key.is_empty() {
            return None;
        }
        self.incorrect_to_handle.get(&key)
    }

    /// Number of incorrect-name entries.
    pub fn len(&self) -> usize {
        self.incorrect_to_handle.len()
    }

    /// Whether the mapping has no entries.
    pub fn is_empty(&self) -> bool {
        self.incorrect_to_handle.is_empty()
    }
}

/// Lowercase a CSV header and turn underscores into spaces for matching.
fn csv_header_key(h: &str) -> String {
    h.trim().to_ascii_lowercase().replace('_', " ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn loads_handle_incorrect_name() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("map.csv");
        let mut f = std::fs::File::create(&path).unwrap();
        write!(
            f,
            "Handle,HandleType,Incorrect Name\n\
+15555550144,phone,Jordan Alias (SKIP)\n\
user@example.com,email,Casey Email\n"
        )
        .unwrap();
        let mapping = NameMapping::load(&path).unwrap();
        assert_eq!(
            mapping.handle_for_incorrect_name("Jordan Alias (SKIP)"),
            Some(&("+15555550144".to_string(), HandleType::Phone))
        );
        assert_eq!(
            mapping.handle_for_incorrect_name("casey email"),
            Some(&("user@example.com".to_string(), HandleType::Email))
        );
    }

    #[test]
    fn defaults_to_phone_type_when_column_missing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("map.csv");
        let mut f = std::fs::File::create(&path).unwrap();
        write!(
            f,
            "Phone,Incorrect Name\n\
+15555550144,Jordan Alias (SKIP)\n"
        )
        .unwrap();
        let mapping = NameMapping::load(&path).unwrap();
        assert_eq!(
            mapping.handle_for_incorrect_name("Jordan Alias (SKIP)"),
            Some(&("+15555550144".to_string(), HandleType::Phone))
        );
    }
}
