use anyhow::{Context, Result, bail};
use message_ir::HandleType;
use rusqlite::{Connection, OptionalExtension, params};

use crate::db::schema;

/// Account identity loaded for profile display: the handles migration replaced the
/// load-for-matching path with direct joins through `account_handles`/`handles`,
/// so this struct/loader now exists to pin the soft-default behavior.

#[derive(Debug, Clone)]
pub struct AccountProfile {
    pub display_name: String,
    pub handle_ids: Vec<i64>,
    pub emails: Vec<String>,
    pub phones: Vec<String>,
}

/// Load account identity (preferred name + linked handle ids) and optional email handles.
/// Soft-defaults when the row is missing or name/handles are empty (`"Me"`, empty sets).

pub fn load_account_profile(conn: &Connection, account_id: &str) -> Result<AccountProfile> {
    let preferred_name: Option<Option<String>> = conn
        .query_row(
            "SELECT preferred_name FROM accounts WHERE id = ?1",
            params![account_id],
            |row| row.get(0),
        )
        .optional()?;

    let preferred = preferred_name
        .flatten()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let display_name = preferred.unwrap_or_else(|| "Me".to_string());

    let mut handle_stmt = conn.prepare(
        "SELECT ah.handle_id FROM account_handles ah WHERE ah.account_id = ?1 ORDER BY ah.handle_id",
    )?;
    let handle_ids: Vec<i64> = handle_stmt
        .query_map(params![account_id], |row| row.get(0))?
        .collect::<Result<Vec<_>, _>>()?;

    let mut email_stmt =
        conn.prepare("SELECT email FROM account_emails WHERE account_id = ?1 ORDER BY email")?;
    let emails: Vec<String> = email_stmt
        .query_map(params![account_id], |row| row.get(0))?
        .collect::<Result<Vec<_>, _>>()?;

    let mut phone_stmt = conn.prepare(
        "SELECT h.normalized FROM handles h
         JOIN account_handles ah ON ah.handle_id = h.id
         WHERE ah.account_id = ?1 AND h.handle_type = 'phone'
         ORDER BY h.normalized",
    )?;
    let phones: Vec<String> = phone_stmt
        .query_map(params![account_id], |row| row.get(0))?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(AccountProfile {
        display_name,
        handle_ids,
        emails,
        phones,
    })
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

/// Canonical form of a handle for identity matching, per type, plus a
/// human-readable note when the canonical form is ambiguous (guarded policy).
///
/// Mirrors `import.rs::normalize_handle` (and the address-book normalization in
/// `db/contacts.rs`): phones become E.164 when unambiguous, else digits-as-is
/// with a note; emails lowercase; others verbatim.
fn normalize_handle(raw: &str, handle_type: HandleType) -> (String, Option<String>) {
    match handle_type {
        HandleType::Phone => {
            let guarded = phone::normalize_guarded(raw, phone::PhoneRegion::for_raw(raw));
            if guarded.normalized.is_empty() {
                // No usable digits: fall back to the raw, unflagged.
                (raw.trim().to_string(), None)
            } else {
                (guarded.normalized, guarded.note)
            }
        }
        HandleType::Email => (raw.trim().to_lowercase(), None),
        HandleType::Username | HandleType::Other => (raw.trim().to_string(), None),
    }
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

/// Like [`link_account_handle`], optionally recording a messaging `service`
/// (for example `"whatsapp"`) on the handles row.
pub fn link_account_handle_with_service(
    conn: &Connection,
    account_id: &str,
    raw: &str,
    handle_type: HandleType,
    service: Option<&str>,
) -> Result<i64> {
    let (normalized, note) = normalize_handle(raw, handle_type);
    conn.execute(
        "INSERT OR IGNORE INTO handles (account_id, raw, normalized, normalized_note, handle_type, service)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            account_id,
            raw,
            normalized,
            note,
            handle_type.as_str(),
            service
        ],
    )?;
    let handle_id: i64 = conn.query_row(
        "SELECT id FROM handles WHERE account_id = ?1 AND normalized = ?2 AND handle_type = ?3",
        params![account_id, normalized, handle_type.as_str()],
        |row| row.get(0),
    )?;
    if let Some(svc) = service {
        conn.execute(
            "UPDATE handles SET service = COALESCE(service, ?1) WHERE id = ?2",
            params![svc, handle_id],
        )?;
    }
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
pub fn load_password_hash(conn: &Connection, account_id: &str) -> Result<Option<String>> {
    let hash: Option<String> = conn
        .query_row(
            "SELECT password_hash FROM accounts WHERE id = ?1",
            params![account_id],
            |row| row.get(0),
        )
        .optional()?;
    Ok(hash)
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
    let name: Option<String> = conn
        .query_row(
            "SELECT preferred_name FROM accounts WHERE id = ?1",
            params![account_id],
            |row| row.get(0),
        )
        .optional()?;
    Ok(name)
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

/// Open the vault DB and resolve `account_ref` to a UUID.
pub fn resolve_account_ref_at(db_path: &std::path::Path, account_ref: &str) -> Result<String> {
    let conn = Connection::open(db_path)
        .with_context(|| format!("open database {}", db_path.display()))?;
    crate::db::schema::configure_connection(&conn)?;
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
    fn load_profile_soft_defaults_and_preferred_name() {
        let conn = setup();
        let empty = load_account_profile(&conn, "00000000-0000-4000-8000-000000000001").unwrap();
        assert_eq!(empty.display_name, "Me");
        assert!(empty.handle_ids.is_empty());

        conn.execute(
            "UPDATE accounts SET preferred_name = 'MB' WHERE id = ?1",
            params!["00000000-0000-4000-8000-000000000001"],
        )
        .unwrap();
        let handle_id = link_account_handle(
            &conn,
            "00000000-0000-4000-8000-000000000001",
            "+15555550100",
            HandleType::Phone,
        )
        .unwrap();
        let loaded = load_account_profile(&conn, "00000000-0000-4000-8000-000000000001").unwrap();
        assert_eq!(loaded.display_name, "MB");
        assert_eq!(loaded.handle_ids, vec![handle_id]);
    }

    #[test]
    fn link_account_handle_normalizes_and_dedupes() {
        let conn = setup();
        let account = "00000000-0000-4000-8000-000000000001";
        let a = link_account_handle(&conn, account, "+1 (555) 555-0100", HandleType::Phone).unwrap();
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
        let email = link_account_handle(&conn, account, "ME@EXAMPLE.com", HandleType::Email).unwrap();
        let loaded = load_account_profile(&conn, account).unwrap();
        assert_eq!(loaded.handle_ids.len(), 2);
        assert!(loaded.handle_ids.contains(&email));
    }
}
