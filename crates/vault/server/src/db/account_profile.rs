use anyhow::{Context, Result, bail};
use message_ir::HandleType;
use rusqlite::{Connection, OptionalExtension, params};

use crate::db::handles::{normalize_handle, upsert_handle_row};
use crate::db::schema;

/// Contact points linked to an account, for profile display.
#[derive(Debug, Clone)]
pub struct AccountProfile {
    pub emails: Vec<String>,
    pub phones: Vec<String>,
}

/// Load the email and phone handles linked to an account. Both default to empty
/// when nothing is linked.
pub fn load_account_profile(conn: &Connection, account_id: &str) -> Result<AccountProfile> {
    let emails = query_account_strings(
        conn,
        "SELECT email FROM account_emails WHERE account_id = ?1 ORDER BY email",
        account_id,
    )?;
    let phones = query_account_strings(
        conn,
        "SELECT h.normalized FROM handles h
         JOIN account_handles ah ON ah.handle_id = h.id
         WHERE ah.account_id = ?1 AND h.handle_type = 'phone'
         ORDER BY h.normalized",
        account_id,
    )?;
    Ok(AccountProfile { emails, phones })
}

fn query_account_strings(conn: &Connection, sql: &str, account_id: &str) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt
        .query_map(params![account_id], |row| row.get(0))?
        .collect::<Result<Vec<String>, _>>()?;
    Ok(rows)
}

/// Ensure `accounts` row exists (stub username = id) for CLI imports.
pub fn ensure_account_row(conn: &Connection, account_id: &str) -> Result<()> {
    conn.execute(
        "INSERT OR IGNORE INTO accounts (id, username, read_only) VALUES (?1, ?1, 0)",
        params![account_id],
    )
    .with_context(|| format!("failed to ensure account row for {account_id}"))?;
    Ok(())
}

/// Ensure a `handles` row exists and link it to the account via `account_handles`.
/// Returns the handle id.
pub fn link_account_handle(
    conn: &Connection,
    account_id: &str,
    raw: &str,
    handle_type: HandleType,
) -> Result<i64> {
    link_account_handle_with_service(conn, account_id, raw, handle_type, None)
}

/// Like [`link_account_handle`], recording a platform `service`
/// (`phone` | `whatsapp`). Missing/`None` defaults to `phone`.
pub fn link_account_handle_with_service(
    conn: &Connection,
    account_id: &str,
    raw: &str,
    handle_type: HandleType,
    service: Option<&str>,
) -> Result<i64> {
    let (handle_id, _) = upsert_handle_row(conn, account_id, raw, handle_type, service)?;
    conn.execute(
        "INSERT OR IGNORE INTO account_handles (account_id, handle_id) VALUES (?1, ?2)",
        params![account_id, handle_id],
    )?;
    Ok(handle_id)
}

fn looks_like_uuid(s: &str) -> bool {
    let s = s.trim();
    if s.len() != 36 {
        return false;
    }
    let b = s.as_bytes();
    if b[8] != b'-' || b[13] != b'-' || b[18] != b'-' || b[23] != b'-' {
        return false;
    }
    s.chars()
        .enumerate()
        .all(|(i, c)| matches!(i, 8 | 13 | 18 | 23) || c.is_ascii_hexdigit())
}

/// Look up an existing account by UUID or username (case-insensitive).
/// Returns `None` when no row matches (does not create stubs).
pub fn lookup_account_ref(conn: &Connection, account_ref: &str) -> Result<Option<String>> {
    let account_ref = account_ref.trim();
    if account_ref.is_empty() {
        return Ok(None);
    }
    schema::ensure_accounts_schema(conn)?;

    let by_id: Option<String> = conn
        .query_row(
            "SELECT id FROM accounts WHERE id = ?1",
            params![account_ref],
            |row| row.get(0),
        )
        .optional()?;
    if by_id.is_some() {
        return Ok(by_id);
    }

    let by_user: Option<String> = conn
        .query_row(
            "SELECT id FROM accounts WHERE username = ?1 COLLATE NOCASE",
            params![account_ref],
            |row| row.get(0),
        )
        .optional()?;
    Ok(by_user)
}

/// Resolve an account reference to `accounts.id` for import.
///
/// Accepts UUID or username. Unknown usernames error. Unknown UUID-shaped
/// values are returned as-is so CLI import can still stub-create the row.
pub fn resolve_account_ref(conn: &Connection, account_ref: &str) -> Result<String> {
    let account_ref = account_ref.trim();
    if account_ref.is_empty() {
        bail!("account is empty");
    }
    if let Some(id) = lookup_account_ref(conn, account_ref)? {
        return Ok(id);
    }
    if looks_like_uuid(account_ref) {
        return Ok(account_ref.to_string());
    }
    bail!("account not found: {account_ref} (use an existing username or account UUID)");
}

/// Username for an account id, if the row exists.
pub fn username_for_account(conn: &Connection, account_id: &str) -> Result<Option<String>> {
    schema::ensure_accounts_schema(conn)?;
    let name: Option<String> = conn
        .query_row(
            "SELECT username FROM accounts WHERE id = ?1",
            params![account_id],
            |row| row.get(0),
        )
        .optional()?;
    Ok(name)
}

/// Load the argon2 password hash for an account id, if set.
///
/// Outer `Option` is "row missing"; inner is the nullable `password_hash`
/// column (NULL/empty means passwordless login).
pub fn load_password_hash(conn: &Connection, account_id: &str) -> Result<Option<String>> {
    let hash: Option<Option<String>> = conn
        .query_row(
            "SELECT password_hash FROM accounts WHERE id = ?1",
            params![account_id],
            |row| row.get(0),
        )
        .optional()?;
    Ok(hash.flatten())
}

/// Replace the argon2 password hash for an account.
pub fn update_password_hash(
    conn: &Connection,
    account_id: &str,
    password_hash: &str,
) -> Result<()> {
    conn.execute(
        "UPDATE accounts SET password_hash = ?2 WHERE id = ?1",
        params![account_id, password_hash],
    )
    .with_context(|| format!("update password hash for {account_id}"))?;
    Ok(())
}

/// Permanently delete an account. All dependent rows are removed by
/// ON DELETE CASCADE (messages, conversations, contacts, vault_imports,
/// account_handles/emails/api_tokens).
pub fn delete_account(conn: &Connection, account_id: &str) -> Result<()> {
    conn.execute("DELETE FROM accounts WHERE id = ?1", params![account_id])
        .with_context(|| format!("delete account {account_id}"))?;
    Ok(())
}

/// Stable id for the seeded demo account (`reset-demo`).
pub const DEMO_ACCOUNT_ID: &str = "00000000-0000-0000-0000-00000000d001";

pub fn is_demo_account(account_id: &str) -> bool {
    account_id == DEMO_ACCOUNT_ID
}

/// Whether the account row is marked read-only (demo seed sets this).
pub fn account_is_read_only(conn: &Connection, account_id: &str) -> Result<bool> {
    schema::ensure_accounts_schema(conn)?;
    let flag: Option<i64> = conn
        .query_row(
            "SELECT read_only FROM accounts WHERE id = ?1",
            params![account_id],
            |row| row.get(0),
        )
        .optional()?;
    Ok(flag.unwrap_or(0) != 0)
}

#[derive(Debug, Clone, Copy)]
pub struct DeletedMessagesStats {
    pub conversations: u64,
    pub attachments: u64,
}

/// Permanently delete one account's conversations (cascades to messages,
/// attachments, participants, tapbacks), staging rows, and trash markers.
/// Contacts, labels, login details, and import tokens are retained.
pub fn delete_all_messages_for_account(
    conn: &Connection,
    account_id: &str,
) -> Result<DeletedMessagesStats> {
    schema::ensure_vault_schema(conn)?;
    let attachment_count: i64 = conn.query_row(
        r#"
        SELECT COUNT(*)
        FROM attachments a
        JOIN messages m ON m.id = a.message_id
        JOIN conversations c ON c.id = m.conversation_id
        WHERE c.account_id = ?1
        "#,
        params![account_id],
        |row| row.get(0),
    )?;
    let conversations = conn
        .execute(
            "DELETE FROM conversations WHERE account_id = ?1",
            params![account_id],
        )
        .with_context(|| format!("delete conversations for {account_id}"))?;
    conn.execute(
        "DELETE FROM staging_conversations WHERE account_id = ?1",
        params![account_id],
    )
    .with_context(|| format!("delete staging conversations for {account_id}"))?;
    conn.execute(
        "DELETE FROM trashed_conversations WHERE account_id = ?1",
        params![account_id],
    )
    .with_context(|| format!("delete trashed conversations for {account_id}"))?;
    conn.execute(
        "DELETE FROM trashed_handles WHERE account_id = ?1",
        params![account_id],
    )
    .with_context(|| format!("delete trashed handles for {account_id}"))?;
    Ok(DeletedMessagesStats {
        conversations: u64::try_from(conversations).unwrap_or(0),
        attachments: u64::try_from(attachment_count).unwrap_or(0),
    })
}

/// Look up account id by Hanko user id. Returns None if no account is linked.
pub fn lookup_account_by_hanko(conn: &Connection, hanko_user_id: &str) -> Result<Option<String>> {
    schema::ensure_accounts_schema(conn)?;
    let id: Option<String> = conn
        .query_row(
            "SELECT id FROM accounts WHERE hanko_user_id = ?1",
            params![hanko_user_id],
            |row| row.get(0),
        )
        .optional()?;
    Ok(id)
}

/// Load the preferred_name for an account, if set.
pub fn load_preferred_name(conn: &Connection, account_id: &str) -> Result<Option<String>> {
    let name: Option<Option<String>> = conn
        .query_row(
            "SELECT preferred_name FROM accounts WHERE id = ?1",
            params![account_id],
            |row| row.get(0),
        )
        .optional()?;
    Ok(name
        .flatten()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty()))
}

/// Insert a new account row. All fields except id and username are optional.
pub fn insert_account(
    conn: &Connection,
    id: &str,
    username: &str,
    password_hash: Option<&str>,
    preferred_name: Option<&str>,
    hanko_user_id: Option<&str>,
    read_only: bool,
) -> Result<()> {
    schema::ensure_accounts_schema(conn)?;
    conn.execute(
        "INSERT INTO accounts (id, username, read_only, password_hash, preferred_name, hanko_user_id) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![id, username, read_only as i32, password_hash, preferred_name, hanko_user_id],
    )
    .with_context(|| format!("insert account {username}"))?;
    Ok(())
}

pub fn guest_status(conn: &Connection, account_id: &str) -> Result<Option<String>> {
    schema::ensure_accounts_schema(conn)?;
    let status: Option<Option<String>> = conn
        .query_row(
            "SELECT guest_status FROM accounts WHERE id = ?1",
            params![account_id],
            |row| row.get(0),
        )
        .optional()?;
    Ok(status.flatten().filter(|s| !s.is_empty()))
}

pub fn is_guest_account(conn: &Connection, account_id: &str) -> Result<bool> {
    Ok(guest_status(conn, account_id)?.is_some())
}

pub fn insert_guest_account(
    conn: &Connection,
    id: &str,
    username: &str,
    preferred_name: Option<&str>,
) -> Result<()> {
    schema::ensure_accounts_schema(conn)?;
    conn.execute(
        r#"
        INSERT INTO accounts (
            id, username, read_only, password_hash, preferred_name, guest_status
        ) VALUES (?1, ?2, 0, NULL, ?3, 'ready')
        "#,
        params![id, username, preferred_name],
    )?;
    Ok(())
}

pub fn set_guest_status(conn: &Connection, account_id: &str, status: &str) -> Result<()> {
    conn.execute(
        "UPDATE accounts SET guest_status = ?2 WHERE id = ?1",
        params![account_id, status],
    )?;
    Ok(())
}

/// Ensure a phone handle is linked to the account via `account_handles`.
pub fn upsert_account_phone(conn: &Connection, account_id: &str, phone: &str) -> Result<()> {
    link_account_handle(conn, account_id, phone, HandleType::Phone)?;
    Ok(())
}

/// Upsert an account_emails row.
pub fn upsert_account_email(
    conn: &Connection,
    account_id: &str,
    email: &str,
    is_primary: bool,
) -> Result<()> {
    conn.execute(
        "INSERT OR IGNORE INTO account_emails (account_id, email, is_primary) VALUES (?1, ?2, ?3)",
        params![account_id, email, is_primary as i32],
    )?;
    Ok(())
}

/// Unlink a handle from the account profile (`account_handles`).
///
/// For emails, also removes the matching `account_emails` row. The underlying
/// `handles` row is left in place so conversation history stays intact.
pub fn unlink_account_handle(
    conn: &Connection,
    account_id: &str,
    raw: &str,
    handle_type: HandleType,
) -> Result<bool> {
    let (normalized, _) = normalize_handle(raw, handle_type);
    let handle_id: Option<i64> = conn
        .query_row(
            "SELECT id FROM handles
             WHERE account_id = ?1 AND normalized = ?2 AND handle_type = ?3
             ORDER BY CASE service WHEN 'phone' THEN 0 WHEN 'whatsapp' THEN 1 ELSE 2 END
             LIMIT 1",
            params![account_id, normalized, handle_type.as_str()],
            |row| row.get(0),
        )
        .optional()?;
    let Some(handle_id) = handle_id else {
        if matches!(handle_type, HandleType::Email) {
            let n = conn.execute(
                "DELETE FROM account_emails WHERE account_id = ?1 AND email = ?2",
                params![account_id, normalized],
            )?;
            return Ok(n > 0);
        }
        return Ok(false);
    };

    let removed = conn.execute(
        "DELETE FROM account_handles WHERE account_id = ?1 AND handle_id = ?2",
        params![account_id, handle_id],
    )?;
    if matches!(handle_type, HandleType::Email) {
        conn.execute(
            "DELETE FROM account_emails WHERE account_id = ?1 AND email = ?2",
            params![account_id, normalized],
        )?;
    }
    Ok(removed > 0)
}

/// Open the vault DB and resolve `account_ref` to a UUID.
pub fn resolve_account_ref_at(db_path: &std::path::Path, account_ref: &str) -> Result<String> {
    let conn = schema::open_configured(db_path)
        .with_context(|| format!("open database {}", db_path.display()))?;
    resolve_account_ref(&conn, account_ref)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn setup() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        schema::ensure_vault_schema(&conn).unwrap();
        conn.execute(
            "INSERT INTO accounts (id, username, read_only) VALUES (?1, ?2, 0)",
            params!["00000000-0000-4000-8000-000000000001", "Alice"],
        )
        .unwrap();
        conn
    }

    #[test]
    fn resolve_by_username_case_insensitive() {
        let conn = setup();
        assert_eq!(
            resolve_account_ref(&conn, "alice").unwrap(),
            "00000000-0000-4000-8000-000000000001"
        );
        assert_eq!(
            resolve_account_ref(&conn, "ALICE").unwrap(),
            "00000000-0000-4000-8000-000000000001"
        );
    }

    #[test]
    fn resolve_by_uuid() {
        let conn = setup();
        assert_eq!(
            resolve_account_ref(&conn, "00000000-0000-4000-8000-000000000001").unwrap(),
            "00000000-0000-4000-8000-000000000001"
        );
    }

    #[test]
    fn unknown_username_errors() {
        let conn = setup();
        let err = resolve_account_ref(&conn, "nobody")
            .unwrap_err()
            .to_string();
        assert!(err.contains("not found"), "{err}");
    }

    #[test]
    fn unknown_uuid_passthrough() {
        let conn = setup();
        let id = "11111111-1111-4111-8111-111111111111";
        assert_eq!(resolve_account_ref(&conn, id).unwrap(), id);
    }

    #[test]
    fn username_for_account_works() {
        let conn = setup();
        assert_eq!(
            username_for_account(&conn, "00000000-0000-4000-8000-000000000001")
                .unwrap()
                .as_deref(),
            Some("Alice")
        );
    }

    #[test]
    fn load_password_hash_returns_none_when_null() {
        // Demo (and any passwordless account) stores password_hash as SQL NULL.
        // Reading that column must not fail with "Invalid column type Null".
        let conn = setup();
        let hash = load_password_hash(&conn, "00000000-0000-4000-8000-000000000001").unwrap();
        assert_eq!(hash, None);
    }

    #[test]
    fn load_password_hash_returns_set_value() {
        let conn = setup();
        update_password_hash(
            &conn,
            "00000000-0000-4000-8000-000000000001",
            "$argon2id$example",
        )
        .unwrap();
        let hash = load_password_hash(&conn, "00000000-0000-4000-8000-000000000001").unwrap();
        assert_eq!(hash.as_deref(), Some("$argon2id$example"));
    }

    #[test]
    fn load_profile_returns_linked_handles_and_preferred_name() {
        let conn = setup();
        let empty = load_account_profile(&conn, "00000000-0000-4000-8000-000000000001").unwrap();
        assert!(empty.phones.is_empty());
        assert!(empty.emails.is_empty());
        assert_eq!(
            load_preferred_name(&conn, "00000000-0000-4000-8000-000000000001").unwrap(),
            None
        );

        conn.execute(
            "UPDATE accounts SET preferred_name = 'MB' WHERE id = ?1",
            params!["00000000-0000-4000-8000-000000000001"],
        )
        .unwrap();
        link_account_handle(
            &conn,
            "00000000-0000-4000-8000-000000000001",
            "+15555550100",
            HandleType::Phone,
        )
        .unwrap();
        let loaded = load_account_profile(&conn, "00000000-0000-4000-8000-000000000001").unwrap();
        assert_eq!(loaded.phones, vec!["+15555550100".to_string()]);
        assert_eq!(
            load_preferred_name(&conn, "00000000-0000-4000-8000-000000000001").unwrap(),
            Some("MB".to_string())
        );
    }

    #[test]
    fn link_account_handle_normalizes_and_dedupes() {
        let conn = setup();
        let account = "00000000-0000-4000-8000-000000000001";
        let a =
            link_account_handle(&conn, account, "+1 (555) 555-0100", HandleType::Phone).unwrap();
        // Same normalized value with a different raw form reuses the handle row.
        let b = link_account_handle(&conn, account, "+15555550100", HandleType::Phone).unwrap();
        assert_eq!(a, b);
        let normalized: String = conn
            .query_row(
                "SELECT normalized FROM handles WHERE id = ?1",
                params![a],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(normalized, "+15555550100");
        let linked: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM account_handles WHERE account_id = ?1",
                params![account],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(linked, 1);
        // Email handles are lowercased and stored separately by type.
        let email =
            link_account_handle(&conn, account, "ME@EXAMPLE.com", HandleType::Email).unwrap();
        let linked_ids: Vec<i64> = conn
            .prepare("SELECT handle_id FROM account_handles WHERE account_id = ?1")
            .unwrap()
            .query_map(params![account], |row| row.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(linked_ids.len(), 2);
        assert!(linked_ids.contains(&email));
    }

    #[test]
    fn guest_helpers_work() {
        let conn = setup();
        let guest_id = "22222222-2222-4222-8222-222222222222";
        insert_guest_account(&conn, guest_id, "guest-abc", Some("Guest")).unwrap();
        assert_eq!(
            guest_status(&conn, guest_id).unwrap().as_deref(),
            Some("ready")
        );
        assert!(is_guest_account(&conn, guest_id).unwrap());
        set_guest_status(&conn, guest_id, "assigned").unwrap();
        assert_eq!(
            guest_status(&conn, guest_id).unwrap().as_deref(),
            Some("assigned")
        );
        assert!(!is_guest_account(&conn, "00000000-0000-4000-8000-000000000001").unwrap());
    }

    #[test]
    fn delete_all_messages_keeps_account_and_contacts() {
        let conn = setup();
        let account = "00000000-0000-4000-8000-000000000001";
        let handle_id =
            link_account_handle(&conn, account, "+15555550100", HandleType::Phone).unwrap();
        conn.execute(
            "INSERT INTO contacts (account_id, preferred_name) VALUES (?1, 'Pat')",
            params![account],
        )
        .unwrap();
        let contact_id: i64 = conn
            .query_row(
                "SELECT id FROM contacts WHERE account_id = ?1",
                params![account],
                |r| r.get(0),
            )
            .unwrap();
        conn.execute(
            "INSERT INTO conversations (
                id, account_id, chat_handle_id, conversation_type, source_file
             ) VALUES (1, ?1, ?2, 'individual', 'c.jsonl')",
            params![account, handle_id],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO messages (
                conversation_id, account_id, source, timestamp, is_from_me, sort_order, body
             ) VALUES (1, ?1, 'imessage', '2020-01-01T00:00:00Z', 1, 0, 'hi')",
            params![account],
        )
        .unwrap();
        let msg_id: i64 = conn
            .query_row(
                "SELECT id FROM messages WHERE account_id = ?1",
                params![account],
                |r| r.get(0),
            )
            .unwrap();
        conn.execute(
            "INSERT INTO attachments (message_id, path, original_name, mime_type)
             VALUES (?1, 'a.jpg', 'a.jpg', 'image/jpeg')",
            params![msg_id],
        )
        .unwrap();

        let stats = delete_all_messages_for_account(&conn, account).unwrap();
        assert_eq!(stats.conversations, 1);
        assert_eq!(stats.attachments, 1);
        let remaining_msgs: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM messages WHERE account_id = ?1",
                params![account],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(remaining_msgs, 0);
        let contacts: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM contacts WHERE id = ?1",
                params![contact_id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(contacts, 1);
        assert!(username_for_account(&conn, account).unwrap().is_some());
    }
}
