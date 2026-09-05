//! Decrypt encrypted iOS backup files (Messages DB, Contacts DB, attachments).
//!
//! Every decrypted file lands in the scratch folder the app named, so a
//! helper the app kills mid-run leaves nothing behind once the app removes
//! that folder.

use std::{
    fs::File,
    io::{BufWriter, Write, copy},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use crabapple::{
    Authentication, Backup, backup::models::manifest::manifest_plist::ManifestData,
    error::BackupError,
};
use imessage_database::{tables::table::DEFAULT_PATH_IOS, util::platform::Platform};

use crate::{
    contacts,
    error::{
        ENCRYPTED_BACKUP_PASSWORD_REQUIRED, IOS_BACKUP_PASSWORD_INCORRECT, NOT_AN_IPHONE_BACKUP,
        RuntimeError, UNENCRYPTED_BACKUP_CLEAR_PASSWORD,
    },
    options::ReaderOptions,
};

const MAX_IN_MEMORY_DECRYPT: u64 = 25 * 1024 * 1024;

/// Setup steps an encrypted iOS backup goes through before parse: keys, then
/// the messages database, then the contacts database.
const DECRYPT_STEPS: u64 = 5;

/// Process-unique suffix (PID + timestamp + counter) so concurrent exports
/// never share the same `/tmp` file name. The counter covers same-process
/// collisions when the clock tick is coarse.
static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Unique suffix for a temp file name.
fn unique_suffix() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or_default();
    let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{}-{nanos}-{counter}", std::process::id())
}

/// Restrict a freshly created temp file to the current user (0600) on Unix.
///
/// Decrypted backup contents must not be world-readable in shared `/tmp`.
///
/// # Errors
///
/// Returns an error when permissions cannot be set.
#[cfg(unix)]
fn restrict_permissions(file: &File) -> Result<(), RuntimeError> {
    use std::{fs, os::unix::fs::PermissionsExt};
    file.set_permissions(fs::Permissions::from_mode(0o600))?;
    Ok(())
}

/// No-op off Unix; the Unix version narrows the decrypted backup's file mode.
#[cfg(not(unix))]
fn restrict_permissions(_file: &File) -> Result<(), RuntimeError> {
    Ok(())
}

/// Open the iOS backup using the password from options when encrypted.
///
/// Returns `Ok(None)` for non-iOS platforms or unencrypted iOS backups.
/// Does not prompt on stdin.
///
/// # Errors
///
/// Returns an error when the backup is unencrypted but a password was given,
/// the password is missing or wrong, or the backup cannot be opened.
pub(crate) fn decrypt_backup(options: &ReaderOptions) -> Result<Option<Backup>, RuntimeError> {
    if !matches!(options.platform, Platform::iOS) {
        return Ok(None);
    }

    let manifest_data = ManifestData::from_plist(options.db_path.join("Manifest.plist"))?;

    if !manifest_data.is_encrypted {
        reject_leftover_password(false, options.cleartext_password.as_deref())?;
        return Ok(None);
    }

    let password = password_for_encrypted_backup(options.cleartext_password.as_deref())?;

    options.emit_log("Decrypting iOS backup...");
    options.setup_step(1, DECRYPT_STEPS, "Deriving backup keys");
    let backup = match Backup::open(options.db_path.clone(), &Authentication::Password(password)) {
        Ok(backup) => backup,
        Err(BackupError::PasswordOrKeyIncorrect) => {
            return Err(RuntimeError::InvalidOptions(
                IOS_BACKUP_PASSWORD_INCORRECT.to_string(),
            ));
        }
        Err(other) => return Err(other.into()),
    };

    Ok(Some(backup))
}

/// The password for an encrypted backup, or the error that says one is required.
fn password_for_encrypted_backup(provided: Option<&str>) -> Result<String, RuntimeError> {
    match provided {
        Some(password) => Ok(password.to_string()),
        None => Err(RuntimeError::InvalidOptions(
            ENCRYPTED_BACKUP_PASSWORD_REQUIRED.to_string(),
        )),
    }
}

/// Fail when a password was given for a backup that is not encrypted, so a wrong assumption is not silently ignored.
fn reject_leftover_password(
    is_encrypted: bool,
    provided: Option<&str>,
) -> Result<(), RuntimeError> {
    if !is_encrypted && provided.is_some() {
        return Err(RuntimeError::InvalidOptions(
            UNENCRYPTED_BACKUP_CLEAR_PASSWORD.to_string(),
        ));
    }
    Ok(())
}

/// Write the decrypted Messages database from the iOS backup to a temp file.
///
/// # Errors
///
/// Returns an error when the file is missing from the backup or cannot be written.
pub(crate) fn get_decrypted_message_database(
    backup: &Backup,
    options: &ReaderOptions,
) -> Result<PathBuf, RuntimeError> {
    let (_, file_id) = DEFAULT_PATH_IOS.split_at(3);
    options.setup_step(2, DECRYPT_STEPS, "Resolving messages database");
    let file = match backup.get_file(file_id) {
        Ok(file) => file,
        Err(BackupError::FileNotFoundInBackup(_)) => {
            return Err(RuntimeError::InvalidOptions(
                NOT_AN_IPHONE_BACKUP.to_string(),
            ));
        }
        Err(other) => return Err(other.into()),
    };
    let mut decrypted_chat_db = backup.decrypt_entry_stream(&file)?;

    let tmp_path = options
        .scratch_dir()
        .join(format!("crabapple-sms-{}.db", unique_suffix()));
    let mut file = File::create(&tmp_path)?;
    restrict_permissions(&file)?;

    options.setup_step(3, DECRYPT_STEPS, "Decrypting messages database");
    copy(&mut decrypted_chat_db, &mut file)?;
    Ok(tmp_path)
}

/// Write the decrypted Contacts database from the iOS backup to a temp file.
///
/// # Errors
///
/// Returns an error when the file is missing from the backup or cannot be written.
pub(crate) fn get_decrypted_contacts_database(
    backup: &Backup,
    options: &ReaderOptions,
) -> Result<PathBuf, RuntimeError> {
    let (_, file_id) = contacts::DEFAULT_PATH_IOS.split_at(3);
    options.setup_step(4, DECRYPT_STEPS, "Resolving contacts database");
    let file = backup.get_file(file_id)?;
    let mut decrypted_contacts_db = backup.decrypt_entry_stream(&file)?;

    let tmp_path = options
        .scratch_dir()
        .join(format!("crabapple-contacts-{}.db", unique_suffix()));
    let mut file = File::create(&tmp_path)?;
    restrict_permissions(&file)?;

    options.setup_step(5, DECRYPT_STEPS, "Decrypting contacts database");
    copy(&mut decrypted_contacts_db, &mut file)?;

    Ok(tmp_path)
}

/// Decrypt one iOS backup file into the scratch folder.
///
/// # Errors
///
/// Returns an error when the path has no file name, the file is missing from
/// the backup, or the temp file cannot be written.
pub(crate) fn decrypt_file(
    backup: &Backup,
    from: &Path,
    scratch_dir: &Path,
) -> Result<PathBuf, RuntimeError> {
    match backup.get_file(
        from.file_name()
            .ok_or_else(|| RuntimeError::FileNameError {
                path: from.to_path_buf(),
                reason: "path has no file name component",
            })?
            .to_str()
            .ok_or_else(|| RuntimeError::FileNameError {
                path: from.to_path_buf(),
                reason: "file name is not valid UTF-8",
            })?,
    ) {
        Ok(file) => {
            // file_id may contain '/' (e.g. "2f/2fcab…") — replace path
            // separators to keep the temp filename flat, and strip any leading
            // path components from a malicious manifest.
            let safe_id = file.file_id.rsplit('/').next().unwrap_or(&file.file_id);
            let temp_path = scratch_dir.join(format!("{safe_id}-{}.attachment", unique_suffix()));
            let mut temp_file = File::create(&temp_path)?;
            restrict_permissions(&temp_file)?;

            let file_size = file.metadata.size;
            if file_size > MAX_IN_MEMORY_DECRYPT {
                let mut decryption_stream = backup.decrypt_entry_stream(&file)?;
                let mut writer = BufWriter::new(temp_file);
                copy(&mut decryption_stream, &mut writer)?;
                writer.flush()?;
            } else {
                let decrypted_bytes = backup.decrypt_entry(&file)?;
                temp_file.write_all(&decrypted_bytes)?;
            }

            Ok(temp_path)
        }
        Err(why) => Err(why.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::{password_for_encrypted_backup, reject_leftover_password};
    use crate::error::{ENCRYPTED_BACKUP_PASSWORD_REQUIRED, UNENCRYPTED_BACKUP_CLEAR_PASSWORD};

    #[test]
    fn missing_password_does_not_prompt() {
        let err = password_for_encrypted_backup(None).unwrap_err();
        assert_eq!(err.to_string(), ENCRYPTED_BACKUP_PASSWORD_REQUIRED);
        assert!(!err.to_string().contains("Invalid options"));
        assert!(password_for_encrypted_backup(Some("secret")).is_ok());
    }

    #[test]
    fn leftover_password_on_unencrypted_uses_locked_copy() {
        let err = reject_leftover_password(false, Some("secret")).unwrap_err();
        assert_eq!(err.to_string(), UNENCRYPTED_BACKUP_CLEAR_PASSWORD);
        assert!(reject_leftover_password(false, None).is_ok());
        assert!(reject_leftover_password(true, Some("secret")).is_ok());
    }
}
