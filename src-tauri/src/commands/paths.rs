//! Path helpers the Settings and Import screens use.

use serde::Serialize;
use std::path::{Component, Path, PathBuf};

/// Folder under the user home directory that holds import staging folders.
const IMPORT_STAGING_PARENT: &str = "message-vault";

/// The signed-in user's home folder, plus which OS this process is running on.
#[derive(Debug, Clone, Serialize)]
pub struct HomeDirInfo {
    /// Home folder as an absolute path the UI can join onto.
    pub path: String,
    /// Operating system name as Rust reports it, for example `linux`, `macos`,
    /// or `windows`.
    pub os: String,
}

/// Ask this process for the current user's home directory.
///
/// The WebView cannot see the real home folder on its own. Settings uses the
/// result as a starting point for file paths.
///
/// # Errors
///
/// Returns an error if the operating system does not report a home directory.
#[tauri::command]
pub fn home_dir() -> Result<HomeDirInfo, String> {
    let path = dirs::home_dir()
        .ok_or_else(|| "Could not determine the user home directory".to_string())?;
    Ok(HomeDirInfo {
        path: path.display().to_string(),
        os: std::env::consts::OS.to_string(),
    })
}

/// Open a file or folder with the operating system's default handler.
///
/// Only paths under `{home}/message-vault/` are allowed. That is where import
/// staging folders and `vault-push.log` live.
///
/// # Errors
///
/// Returns an error when the path is empty, outside the allowed folder, or the
/// OS cannot open it.
#[tauri::command]
pub fn open_path(path: String) -> Result<(), String> {
    let home = dirs::home_dir()
        .ok_or_else(|| "Could not determine the user home directory".to_string())?;
    let resolved = resolve_openable_path(&path, &home)?;
    open::that_detached(&resolved).map_err(|error| format!("Could not open path: {error}"))
}

/// Collapse `.` and `..` without requiring the path to exist on disk.
fn normalize_lexically(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            Component::Normal(part) => out.push(part),
            Component::RootDir | Component::Prefix(_) => out.push(component.as_os_str()),
        }
    }
    out
}

/// Resolve `raw` to an absolute path that must stay under `{home}/message-vault/`.
///
/// When the path already exists, both sides are canonicalized so symlinks cannot
/// escape the staging tree. When it does not exist yet (for example a staging
/// folder that extract is about to create), lexical normalization is used.
pub(crate) fn resolve_openable_path(raw: &str, home: &Path) -> Result<PathBuf, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err("Path is empty".to_string());
    }

    let candidate = PathBuf::from(trimmed);
    if !candidate.is_absolute() {
        return Err("Path must be absolute".to_string());
    }

    let staging_root = home.join(IMPORT_STAGING_PARENT);

    if candidate.exists() {
        let canonical = candidate
            .canonicalize()
            .map_err(|error| format!("Could not resolve path: {error}"))?;
        let root = if staging_root.exists() {
            staging_root
                .canonicalize()
                .map_err(|error| format!("Could not resolve staging root: {error}"))?
        } else {
            normalize_lexically(&staging_root)
        };
        if !canonical.starts_with(&root) {
            return Err("Path is outside the import staging folder".to_string());
        }
        return Ok(canonical);
    }

    let normalized = normalize_lexically(&candidate);
    let root = normalize_lexically(&staging_root);
    if !normalized.starts_with(&root) {
        return Err("Path is outside the import staging folder".to_string());
    }
    Ok(normalized)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn rejects_empty_path() {
        let home = PathBuf::from("/home/sam");
        let err = resolve_openable_path("  ", &home).unwrap_err();
        assert!(err.contains("empty"));
    }

    #[test]
    fn rejects_relative_path() {
        let home = PathBuf::from("/home/sam");
        let err = resolve_openable_path("message-vault/staging", &home).unwrap_err();
        assert!(err.contains("absolute"));
    }

    #[test]
    fn accepts_path_under_staging_when_missing() {
        let home = PathBuf::from("/home/sam");
        let path = "/home/sam/message-vault/staging-iphone-ios-260824-180509";
        let resolved = resolve_openable_path(path, &home).unwrap();
        assert_eq!(resolved, PathBuf::from(path));
    }

    #[test]
    fn accepts_log_file_under_staging_when_missing() {
        let home = PathBuf::from("/home/sam");
        let path = "/home/sam/message-vault/staging-x/vault-push.log";
        let resolved = resolve_openable_path(path, &home).unwrap();
        assert_eq!(resolved, PathBuf::from(path));
    }

    #[test]
    fn rejects_path_outside_staging() {
        let home = PathBuf::from("/home/sam");
        let err = resolve_openable_path("/home/sam/Documents/notes.txt", &home).unwrap_err();
        assert!(err.contains("outside"));
    }

    #[test]
    fn rejects_parent_traversal_escape() {
        let home = PathBuf::from("/home/sam");
        let err =
            resolve_openable_path("/home/sam/message-vault/../.ssh/id_rsa", &home).unwrap_err();
        assert!(err.contains("outside"));
    }

    #[test]
    fn accepts_existing_file_under_staging() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path();
        let staging = home.join(IMPORT_STAGING_PARENT).join("staging-test");
        fs::create_dir_all(&staging).unwrap();
        let log = staging.join("vault-push.log");
        fs::write(&log, "ok\n").unwrap();

        let resolved = resolve_openable_path(log.to_str().unwrap(), home).unwrap();
        assert_eq!(resolved, log.canonicalize().unwrap());
    }

    #[test]
    fn rejects_existing_file_outside_staging() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path();
        fs::create_dir_all(home.join(IMPORT_STAGING_PARENT)).unwrap();
        let outside = home.join("secrets.txt");
        fs::write(&outside, "secret\n").unwrap();

        let err = resolve_openable_path(outside.to_str().unwrap(), home).unwrap_err();
        assert!(err.contains("outside"));
    }
}
