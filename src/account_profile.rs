use anyhow::{Context, Result, bail};
use rusqlite::{Connection, OptionalExtension, params};

use crate::schema;

#[derive(Debug, Clone)]
#[allow(dead_code)] // phones/emails loaded for handle matching; export uses display_name
pub struct AccountProfile {
    pub display_name: String,
    pub phones: Vec<String>,
    pub emails: Vec<String>,
}

/// Load account identity (preferred name + phones) and optional email handles.
/// Soft-defaults when the row is missing or name/phones are empty (`"Me"`, empty sets).
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

    let mut phone_stmt =
        conn.prepare("SELECT phone FROM account_phones WHERE account_id = ?1 ORDER BY phone")?;
    let phones: Vec<String> = phone_stmt
        .query_map(params![account_id], |row| row.get(0))?
        .collect::<Result<Vec<_>, _>>()?;

    let mut email_stmt =
        conn.prepare("SELECT email FROM account_emails WHERE account_id = ?1 ORDER BY email")?;
    let emails: Vec<String> = email_stmt
        .query_map(params![account_id], |row| row.get(0))?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(AccountProfile {
        display_name,
        phones,
        emails,
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

/// Open the vault DB and resolve `account_ref` to a UUID.
pub fn resolve_account_ref_at(db_path: &std::path::Path, account_ref: &str) -> Result<String> {
    let conn = Connection::open(db_path)
        .with_context(|| format!("open database {}", db_path.display()))?;
    crate::schema::configure_connection(&conn)?;
    resolve_account_ref(&conn, account_ref)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn setup() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        schema::ensure_accounts_schema(&conn).unwrap();
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
        assert!(empty.phones.is_empty());

        conn.execute(
            "UPDATE accounts SET preferred_name = 'MB' WHERE id = ?1",
            params!["00000000-0000-4000-8000-000000000001"],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO account_phones (account_id, phone) VALUES (?1, ?2)",
            params!["00000000-0000-4000-8000-000000000001", "+15555550100"],
        )
        .unwrap();
        let loaded = load_account_profile(&conn, "00000000-0000-4000-8000-000000000001").unwrap();
        assert_eq!(loaded.display_name, "MB");
        assert_eq!(loaded.phones, vec!["+15555550100".to_string()]);
    }
}
