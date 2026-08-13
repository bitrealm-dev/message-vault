//! Cross-process exclusion between the HTTP server and database replacement.
//!
//! Serve and reset-demo must not run at the same time against the same
//! database. This lock file sits next to the database and is taken exclusively
//! for the life of either operation.

use std::ffi::OsString;
use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use fs2::FileExt;

/// Holds an exclusive lock on `{database}.operation.lock` until dropped.
#[derive(Debug)]
pub(crate) struct VaultOperationLock {
    _file: File,
}

/// Take the lock for the HTTP server. Fails if reset-demo or another server
/// already holds it.
///
/// # Errors
///
/// Returns an error when the lock file cannot be created or is already held.
pub(crate) fn acquire_for_serve(db: &Path) -> Result<VaultOperationLock> {
    acquire(db).with_context(|| {
        format!(
            "cannot start serve for {} while reset-demo or another server is active",
            db.display()
        )
    })
}

/// Take the lock for reset-demo. Fails if the HTTP server already holds it.
///
/// # Errors
///
/// Returns an error when the lock file cannot be created or is already held.
pub(crate) fn acquire_for_reset(db: &Path) -> Result<VaultOperationLock> {
    acquire(db).with_context(|| {
        format!(
            "cannot reset demo while serve is active for {}; stop the server and run reset-demo offline",
            db.display()
        )
    })
}

fn acquire(db: &Path) -> Result<VaultOperationLock> {
    let lock_path = lock_path(db);
    if let Some(parent) = lock_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("create lock directory {}", parent.display()))?;
    }
    let file = OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .open(&lock_path)
        .with_context(|| format!("open vault operation lock {}", lock_path.display()))?;
    file.try_lock_exclusive()
        .with_context(|| format!("acquire vault operation lock {}", lock_path.display()))?;
    Ok(VaultOperationLock { _file: file })
}

fn lock_path(db: &Path) -> PathBuf {
    let mut name: OsString = db.as_os_str().to_owned();
    name.push(".operation.lock");
    PathBuf::from(name)
}
