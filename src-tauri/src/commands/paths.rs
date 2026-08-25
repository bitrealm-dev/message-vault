//! Path helpers the Settings and Import screens use.

use serde::Serialize;
use std::path::{Component, Path, PathBuf};

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
/// Only paths under `staging_root` are allowed. That is the Import Staging
/// Directory from Settings (default `{home}/message-vault`), where staging
/// folders and `vault-push.log` live.
///
/// # Errors
///
/// Returns an error when the path is empty, the staging root is empty, the path
/// is outside the allowed folder, missing on disk, or the OS cannot open it.
#[tauri::command]
pub fn open_path(path: String, staging_root: String) -> Result<(), String> {
    let resolved = resolve_openable_path(&path, &staging_root)?;
    missing_path_error(&resolved)?;
    open::that_detached(&resolved).map_err(|error| format!("Could not open path: {error}"))
}

/// Error when a resolved staging path is not on disk yet.
///
/// The OS opener often reports success for a missing path (for example
/// `xdg-open` exiting 0), so the UI must fail here to show an inline alert.
pub(crate) fn missing_path_error(resolved: &Path) -> Result<(), String> {
    if resolved.exists() {
        return Ok(());
    }
    let name = resolved
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("path");
    if name == "vault-push.log" {
        return Err(
            "vault-push.log is not written until upload starts. Try again after Upload to vault begins."
                .to_string(),
        );
    }
    Err(format!("Nothing exists at {name} yet"))
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

/// Resolve `raw` to an absolute path that must stay under `staging_root`.
///
/// When the path already exists, both sides are canonicalized so symlinks cannot
/// escape the staging tree. When it does not exist yet (for example a staging
/// folder that extract is about to create), lexical normalization is used.
pub(crate) fn resolve_openable_path(raw: &str, staging_root: &str) -> Result<PathBuf, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err("Path is empty".to_string());
    }

    let root_trimmed = staging_root.trim();
    if root_trimmed.is_empty() {
        return Err("Import staging directory is empty".to_string());
    }

    let candidate = PathBuf::from(trimmed);
    if !candidate.is_absolute() {
        return Err("Path must be absolute".to_string());
    }

    let staging_root = PathBuf::from(root_trimmed);
    if !staging_root.is_absolute() {
        return Err("Import staging directory must be absolute".to_string());
    }

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
        reject_filesystem_root(&root)?;
        if !canonical.starts_with(&root) {
            return Err("Path is outside the import staging folder".to_string());
        }
        return Ok(canonical);
    }

    let normalized = normalize_lexically(&candidate);
    let root = normalize_lexically(&staging_root);
    reject_filesystem_root(&root)?;
    if !normalized.starts_with(&root) {
        return Err("Path is outside the import staging folder".to_string());
    }
    Ok(normalized)
}

/// `/` (and a Windows drive root) would make `starts_with` true for every absolute path.
fn reject_filesystem_root(root: &Path) -> Result<(), String> {
    if root.parent().is_none() {
        return Err("Import staging directory cannot be the filesystem root".to_string());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn rejects_empty_path() {
        let root = "/home/sam/message-vault";
        let err = resolve_openable_path("  ", root).unwrap_err();
        assert!(err.contains("empty"));
    }

    #[test]
    fn rejects_empty_staging_root() {
        let err = resolve_openable_path("/home/sam/message-vault/staging", "  ").unwrap_err();
        assert!(err.contains("staging directory is empty"));
    }

    #[test]
    fn rejects_relative_path() {
        let root = "/home/sam/message-vault";
        let err = resolve_openable_path("message-vault/staging", root).unwrap_err();
        assert!(err.contains("absolute"));
    }

    #[test]
    fn rejects_relative_staging_root() {
        let err = resolve_openable_path("/tmp/staging", "message-vault").unwrap_err();
        assert!(err.contains("must be absolute"));
    }

    #[test]
    fn rejects_filesystem_root_staging_root() {
        let err = resolve_openable_path("/etc/passwd", "/").unwrap_err();
        assert!(err.contains("filesystem root"));
    }

    #[test]
    fn accepts_path_under_staging_when_missing() {
        let root = "/home/sam/message-vault";
        let path = "/home/sam/message-vault/staging-iphone-ios-260824-180509";
        let resolved = resolve_openable_path(path, root).unwrap();
        assert_eq!(resolved, PathBuf::from(path));
    }

    #[test]
    fn accepts_path_under_custom_staging_root() {
        let root = "/data/imports";
        let path = "/data/imports/staging-iphone-ios-260824-180509";
        let resolved = resolve_openable_path(path, root).unwrap();
        assert_eq!(resolved, PathBuf::from(path));
    }

    #[test]
    fn accepts_log_file_under_staging_when_missing() {
        let root = "/home/sam/message-vault";
        let path = "/home/sam/message-vault/staging-x/vault-push.log";
        let resolved = resolve_openable_path(path, root).unwrap();
        assert_eq!(resolved, PathBuf::from(path));
    }

    #[test]
    fn rejects_path_outside_staging() {
        let root = "/home/sam/message-vault";
        let err = resolve_openable_path("/home/sam/Documents/notes.txt", root).unwrap_err();
        assert!(err.contains("outside"));
    }

    #[test]
    fn rejects_parent_traversal_escape() {
        let root = "/home/sam/message-vault";
        let err =
            resolve_openable_path("/home/sam/message-vault/../.ssh/id_rsa", root).unwrap_err();
        assert!(err.contains("outside"));
    }

    #[test]
    fn accepts_existing_file_under_staging() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("message-vault");
        let staging = root.join("staging-test");
        fs::create_dir_all(&staging).unwrap();
        let log = staging.join("vault-push.log");
        fs::write(&log, "ok\n").unwrap();

        let resolved =
            resolve_openable_path(log.to_str().unwrap(), root.to_str().unwrap()).unwrap();
        assert_eq!(resolved, log.canonicalize().unwrap());
    }

    #[test]
    fn rejects_existing_file_outside_staging() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("message-vault");
        fs::create_dir_all(&root).unwrap();
        let outside = temp.path().join("secrets.txt");
        fs::write(&outside, "secret\n").unwrap();

        let err =
            resolve_openable_path(outside.to_str().unwrap(), root.to_str().unwrap()).unwrap_err();
        assert!(err.contains("outside"));
    }

    #[test]
    fn missing_log_explains_upload_timing() {
        let log = PathBuf::from("/home/sam/message-vault/staging-x/vault-push.log");
        let err = missing_path_error(&log).unwrap_err();
        assert!(err.contains("upload starts"));
    }

    #[test]
    fn missing_folder_uses_generic_message() {
        let staging = PathBuf::from("/home/sam/message-vault/staging-x");
        let err = missing_path_error(&staging).unwrap_err();
        assert!(err.contains("Nothing exists"));
        assert!(err.contains("staging-x"));
    }

    #[test]
    fn existing_path_passes_missing_check() {
        let temp = tempfile::tempdir().unwrap();
        let file = temp.path().join("vault-push.log");
        fs::write(&file, "ok\n").unwrap();
        missing_path_error(&file).unwrap();
    }
}
