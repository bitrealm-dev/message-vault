//! Encrypted iOS backup decrypt helpers (ported from imessage-vault-io).

use std::{
    env::temp_dir,
    fs::File,
    io::{BufWriter, IsTerminal, Write, copy, stdin},
    path::{Path, PathBuf},
};

use crabapple::{
    Authentication, Backup, backup::models::manifest::manifest_plist::ManifestData,
    error::BackupError,
};
use imessage_database::{tables::table::DEFAULT_PATH_IOS, util::platform::Platform};
use message_vault_io_core::{LogSink, emit_log};

use crate::{contacts, error::RuntimeError, options::MailOptions};

const MAX_IN_MEMORY_DECRYPT: u64 = 25 * 1024 * 1024;

/// Open the iOS backup, prompting for a password if encrypted and none was provided.
///
/// Returns `Ok(None)` for non-iOS platforms or unencrypted iOS backups.
pub(crate) fn decrypt_backup(options: &MailOptions) -> Result<Option<Backup>, RuntimeError> {
    if !matches!(options.platform, Platform::iOS) {
        return Ok(None);
    }

    let manifest_data = ManifestData::from_plist(options.db_path.join("Manifest.plist"))?;

    if !manifest_data.is_encrypted {
        if options.cleartext_password.is_some() {
            return Err(RuntimeError::InvalidOptions(format!(
                "--cleartext-password was provided, but the iOS backup at {} is not encrypted.",
                options.db_path.display()
            )));
        }
        return Ok(None);
    }

    let password = match options.cleartext_password.as_deref() {
        Some(pw) => pw.to_string(),
        None => prompt_for_password()?,
    };

    options.emit_log("Decrypting iOS backup...");
    options.emit_log("  [1/5] Deriving backup keys...");
    let backup = match Backup::open(options.db_path.clone(), &Authentication::Password(password)) {
        Ok(backup) => backup,
        Err(BackupError::PasswordOrKeyIncorrect) => {
            return Err(RuntimeError::InvalidOptions(
                "The iOS backup password was incorrect.".to_string(),
            ));
        }
        Err(other) => return Err(other.into()),
    };

    Ok(Some(backup))
}

fn prompt_for_password() -> Result<String, RuntimeError> {
    if !stdin().is_terminal() {
        return Err(RuntimeError::InvalidOptions(
            "No terminal available to prompt for the iOS backup password; pass a backup password for non-interactive use.".to_string(),
        ));
    }
    eprintln!("Encrypted iOS backup detected. Enter password (input hidden):");
    rpassword::prompt_password("> ").map_err(|e| {
        RuntimeError::InvalidOptions(format!(
            "Unable to read password interactively ({e}); pass a backup password for non-interactive use."
        ))
    })
}

/// Write the decrypted Messages database from the iOS backup to a temp file.
pub(crate) fn get_decrypted_message_database(
    backup: &Backup,
    log: Option<&LogSink>,
) -> Result<PathBuf, RuntimeError> {
    let (_, file_id) = DEFAULT_PATH_IOS.split_at(3);
    emit_log(log, "  [2/5] Resolving messages database...");
    let file = backup.get_file(file_id)?;
    let mut decrypted_chat_db = backup.decrypt_entry_stream(&file)?;

    let tmp_path = temp_dir().join("crabapple-sms.db");
    let mut file = File::create(&tmp_path)?;

    emit_log(log, "  [3/5] Decrypting messages database...");
    copy(&mut decrypted_chat_db, &mut file)?;
    Ok(tmp_path)
}

/// Write the decrypted Contacts database from the iOS backup to a temp file.
pub(crate) fn get_decrypted_contacts_database(
    backup: &Backup,
    log: Option<&LogSink>,
) -> Result<PathBuf, RuntimeError> {
    let (_, file_id) = contacts::DEFAULT_PATH_IOS.split_at(3);
    emit_log(log, "  [4/5] Resolving contacts database...");
    let file = backup.get_file(file_id)?;
    let mut decrypted_contacts_db = backup.decrypt_entry_stream(&file)?;

    let tmp_path = temp_dir().join("crabapple-contacts.db");
    let mut file = File::create(&tmp_path)?;

    emit_log(log, "  [5/5] Decrypting contacts database...");
    copy(&mut decrypted_contacts_db, &mut file)?;

    Ok(tmp_path)
}

/// Decrypt one iOS backup file into a temporary file.
pub(crate) fn decrypt_file(backup: &Backup, from: &Path) -> Result<PathBuf, RuntimeError> {
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
            let temp_path = temp_dir().join(&file.file_id);
            let mut temp_file = File::create(&temp_path)?;

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
