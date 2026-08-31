//! Read which addresses a backup's device sent from, before any parsing.
//!
//! The Import screen's identity check calls [`backup_identities`] right after
//! the user starts an iMessage import, before the import session is created.
//! It opens the source through the same [`DataSource`] the real run uses, so
//! every method (Mac `chat.db`, iPhone backup folder, jailbreak `sms.db`) and
//! both encryption states go through one code path.

use std::{collections::HashSet, fs::File, path::Path};

use imessage_database::util::{platform::Platform, query_context::QueryContext};
use message_ir_format::ExportTransforms;
use message_vault_io_core::OutputFormat;
use rusqlite::Connection;

use crate::{
    data_source::DataSource,
    options::{AttachmentEmbed, MailOptions},
};

/// `Info.plist` → `Phone Number` from an iOS backup folder.
///
/// Returns `None` when the file is missing or cannot be parsed. `Info.plist`
/// is plaintext even in an encrypted backup.
pub fn ios_backup_phone_number(backup_root: &Path) -> Option<String> {
    let file = File::open(backup_root.join("Info.plist")).ok()?;
    let value = plist::Value::from_reader(file).ok()?;
    let dict = value.as_dictionary()?;
    match dict.get("Phone Number") {
        Some(plist::Value::String(number)) => Some(number.clone()),
        _ => None,
    }
}

/// Addresses the backup's device sent from: the union of
/// `chat.account_login`, `message.destination_caller_id`, and (for iOS
/// backups) `Info.plist` → `Phone Number`, cleaned and deduplicated.
///
/// Each per-column query falls back to an empty list when the table or
/// column is missing, so an unusual schema degrades to fewer signals rather
/// than an error.
///
/// # Errors
///
/// Returns an error when the source cannot be opened: missing database,
/// missing or wrong backup password, not an iPhone backup.
pub fn backup_identities(
    db_path: &Path,
    ios: bool,
    backup_password: Option<&str>,
) -> anyhow::Result<Vec<String>> {
    let options = MailOptions {
        db_path: db_path.to_path_buf(),
        attachment_root: None,
        export_path: std::path::PathBuf::new(),
        query_context: QueryContext::default(),
        use_caller_id: true,
        platform: if ios { Platform::iOS } else { Platform::macOS },
        conversation_filter: None,
        cleartext_password: backup_password.map(str::to_string),
        contacts_path: None,
        attachment_embed: AttachmentEmbed::Disabled,
        transforms: ExportTransforms::default(),
        output_format: OutputFormat::Jsonl,
        log: None,
        cancel: None,
        resume: false,
    };
    let data_source = DataSource::from(&options)?;

    let mut raw = distinct_texts(data_source.db(), "SELECT DISTINCT account_login FROM chat");
    raw.extend(distinct_texts(
        data_source.db(),
        "SELECT DISTINCT destination_caller_id FROM message",
    ));
    if ios {
        raw.extend(ios_backup_phone_number(db_path));
    }

    let mut seen = HashSet::new();
    let mut identities = Vec::new();
    for value in raw {
        let Some(cleaned) = clean_identity(&value) else {
            continue;
        };
        let key = identity_key(&cleaned);
        if key.is_empty() || !seen.insert(key) {
            continue;
        }
        identities.push(cleaned);
    }
    Ok(identities)
}

/// One column's distinct values; empty on any query error (older schemas).
fn distinct_texts(db: &Connection, sql: &str) -> Vec<String> {
    let Ok(mut stmt) = db.prepare(sql) else {
        return Vec::new();
    };
    let Ok(rows) = stmt.query_map([], |row| row.get::<_, Option<String>>(0)) else {
        return Vec::new();
    };
    rows.flatten().flatten().collect()
}

/// Strip the `P:` / `E:` / `tel:` prefix and drop what is then empty.
///
/// Real backups hold `account_login` rows that are the bare prefix `E:` with
/// nothing after it, so the emptiness test must run on the remainder.
fn clean_identity(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    let stripped = trimmed
        .strip_prefix("P:")
        .or_else(|| trimmed.strip_prefix("E:"))
        .or_else(|| trimmed.strip_prefix("tel:"))
        .unwrap_or(trimmed)
        .trim();
    if stripped.is_empty() {
        None
    } else {
        Some(stripped.to_string())
    }
}

/// Deduplication key: emails lowercased, phones as US national digits
/// (matching `toUsNationalDigits` in the web app and the vault's
/// `sanitize_number`).
fn identity_key(value: &str) -> String {
    if value.contains('@') {
        return value.to_ascii_lowercase();
    }
    let mut digits: String = value.chars().filter(char::is_ascii_digit).collect();
    if digits.len() == 11 && digits.starts_with('1') {
        digits.remove(0);
    }
    digits
}

#[cfg(test)]
mod tests {
    use super::{backup_identities, clean_identity, identity_key, ios_backup_phone_number};
    use rusqlite::Connection;

    #[test]
    fn clean_identity_strips_prefixes_and_drops_empties() {
        assert_eq!(
            clean_identity("P:+15550001111"),
            Some("+15550001111".to_string())
        );
        assert_eq!(
            clean_identity("E:owner@example.com"),
            Some("owner@example.com".to_string())
        );
        assert_eq!(
            clean_identity("tel:+15550001111"),
            Some("+15550001111".to_string())
        );
        assert_eq!(clean_identity("E:"), None);
        assert_eq!(clean_identity("  "), None);
    }

    #[test]
    fn identity_key_normalizes_phones_and_emails() {
        assert_eq!(identity_key("+1 (555) 000-1111"), "5550001111");
        assert_eq!(identity_key("5550001111"), "5550001111");
        assert_eq!(identity_key("Owner@Example.com"), "owner@example.com");
    }

    #[test]
    fn info_plist_phone_number_reads_string() {
        let dir = tempfile::tempdir().unwrap();
        let body = r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Phone Number</key>
  <string>+1 (555) 000-1111</string>
</dict>
</plist>
"#;
        std::fs::write(dir.path().join("Info.plist"), body).unwrap();
        assert_eq!(
            ios_backup_phone_number(dir.path()),
            Some("+1 (555) 000-1111".to_string())
        );

        let missing = tempfile::tempdir().unwrap();
        assert_eq!(ios_backup_phone_number(missing.path()), None);
    }

    #[test]
    fn backup_identities_cleans_and_dedupes() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("chat.db");
        let db = Connection::open(&db_path).unwrap();
        db.execute_batch(
            "CREATE TABLE chat (ROWID INTEGER PRIMARY KEY, account_login TEXT);
             CREATE TABLE message (ROWID INTEGER PRIMARY KEY, destination_caller_id TEXT);
             INSERT INTO chat (account_login) VALUES
                 ('P:+15550001111'), ('E:'), ('E:Owner@Example.com');
             INSERT INTO message (destination_caller_id) VALUES
                 ('+15550001111'), ('tel:+15550001111'), ('owner@example.com'), (NULL);",
        )
        .unwrap();
        drop(db);

        let mut identities = backup_identities(&db_path, false, None).unwrap();
        identities.sort();
        assert_eq!(
            identities,
            vec!["+15550001111".to_string(), "Owner@Example.com".to_string()]
        );
    }

    #[test]
    fn backup_identities_survives_missing_columns() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("chat.db");
        let db = Connection::open(&db_path).unwrap();
        db.execute_batch("CREATE TABLE chat (ROWID INTEGER PRIMARY KEY);")
            .unwrap();
        drop(db);

        let identities = backup_identities(&db_path, false, None).unwrap();
        assert!(identities.is_empty());
    }
}
