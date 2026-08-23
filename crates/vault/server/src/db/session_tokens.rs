//! GUI session Bearer tokens (`mv-user-…`); one per account, rotates on login.

use anyhow::{Context, Result, bail};
use rand::TryRng;
use rusqlite::{Connection, OptionalExtension, params};

const TOKEN_ALPHANUM: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";

/// Default GUI session lifetime (30 days).
pub const SESSION_TTL_SECS: u64 = 30 * 24 * 60 * 60;

/// Generate a new GUI session token (`mv-user-` + 32 alphanumeric characters).
///
/// # Errors
///
/// Returns an error when random bytes cannot be generated.
#[allow(dead_code)] // used by rotate_account_session_token
pub fn generate_session_token() -> Result<String> {
    generate_prefixed_token("mv-user-")
}

pub(crate) fn generate_prefixed_token(prefix: &str) -> Result<String> {
    let mut buf = [0u8; 32];
    fill_random(&mut buf)?;
    let mut suffix = String::with_capacity(32);
    for b in buf {
        suffix.push(TOKEN_ALPHANUM[(b as usize) % TOKEN_ALPHANUM.len()] as char);
    }
    Ok(format!("{prefix}{suffix}"))
}

/// SHA-256 hex fingerprint of a plaintext token (stored in DB; used for Bearer lookup).
pub fn hash_api_token(token: &str) -> String {
    crate::assets::sha256_hex(token.as_bytes())
}

fn fill_random(buf: &mut [u8]) -> Result<()> {
    rand::rngs::SysRng
        .try_fill_bytes(buf)
        .map_err(|e| anyhow::anyhow!("secure random unavailable: {e}"))?;
    if buf.iter().all(|&b| b == 0) {
        bail!("secure random returned an empty entropy buffer");
    }
    Ok(())
}

fn session_expiry_unix(now_secs: u64) -> String {
    format!("{}", now_secs.saturating_add(SESSION_TTL_SECS))
}

fn now_unix_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Look up which account owns this session Bearer (by hash). Expired rows are removed.
///
/// # Errors
///
/// Returns an error when the lookup or delete fails.
pub fn lookup_account_for_token(conn: &Connection, token: &str) -> Result<Option<String>> {
    let token_hash = hash_api_token(token);
    let found: Option<(String, String)> = conn
        .query_row(
            "SELECT account_id, expires_at FROM account_session_tokens WHERE token_hash = ?1",
            params![token_hash],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    let Some((account_id, expires_at)) = found else {
        return Ok(None);
    };
    let expires = expires_at.parse::<u64>().unwrap_or(0);
    let now = now_unix_secs();
    // Legacy rows migrated with expires_at='0' are treated as expired; rotate via login.
    if expires == 0 || expires <= now {
        let _ = conn.execute(
            "DELETE FROM account_session_tokens WHERE token_hash = ?1",
            params![token_hash],
        );
        return Ok(None);
    }
    Ok(Some(account_id))
}

/// True when the account already has a session token row.
///
/// # Errors
///
/// Returns an error when the count query fails.
#[allow(dead_code)]
pub fn account_has_session_token(conn: &Connection, account_id: &str) -> Result<bool> {
    let n: i64 = conn.query_row(
        "SELECT COUNT(*) FROM account_session_tokens WHERE account_id = ?1",
        params![account_id],
        |row| row.get(0),
    )?;
    Ok(n > 0)
}

/// Create or replace the account's session token hash; returns plaintext once.
///
/// # Errors
///
/// Returns an error when a token cannot be generated or the write fails.
#[allow(dead_code)]
pub fn rotate_account_session_token(conn: &Connection, account_id: &str) -> Result<String> {
    let token = generate_session_token()?;
    let token_hash = hash_api_token(&token);
    let created_at = unix_secs_string();
    let expires_at = session_expiry_unix(now_unix_secs());
    conn.execute(
        r#"
        INSERT INTO account_session_tokens (account_id, token_hash, created_at, expires_at)
        VALUES (?1, ?2, ?3, ?4)
        ON CONFLICT(account_id) DO UPDATE SET
            token_hash = excluded.token_hash,
            created_at = excluded.created_at,
            expires_at = excluded.expires_at
        "#,
        params![account_id, token_hash, created_at, expires_at],
    )
    .with_context(|| format!("rotate session token for {account_id}"))?;
    Ok(token)
}

/// Create a fresh session token for an account and return the plaintext.
///
/// # Errors
///
/// Returns an error when a token cannot be generated or the insert fails.
pub fn insert_account_session_token(conn: &Connection, account_id: &str) -> Result<String> {
    insert_account_session_token_with_ttl(conn, account_id, SESSION_TTL_SECS)
}

/// Create a fresh session token with a custom lifetime and return the plaintext.
///
/// # Errors
///
/// Returns an error when a token cannot be generated or the insert fails.
pub fn insert_account_session_token_with_ttl(
    conn: &Connection,
    account_id: &str,
    ttl_secs: u64,
) -> Result<String> {
    let token = generate_session_token()?;
    let token_hash = hash_api_token(&token);
    let created_at = unix_secs_string();
    let expires_at = format!("{}", now_unix_secs().saturating_add(ttl_secs));
    conn.execute(
        "INSERT INTO account_session_tokens (account_id, token_hash, created_at, expires_at)
         VALUES (?1, ?2, ?3, ?4)",
        params![account_id, token_hash, created_at, expires_at],
    )
    .with_context(|| format!("insert session token for {account_id}"))?;
    Ok(token)
}

/// Session token for GUI: if a row exists, rotate it; otherwise insert.
///
/// # Errors
///
/// Returns an error when the lookup or token write fails.
pub fn get_or_create_session_token(conn: &Connection, account_id: &str) -> Result<String> {
    let existing: Option<String> = conn
        .query_row(
            "SELECT token_hash FROM account_session_tokens WHERE account_id = ?1",
            params![account_id],
            |row| row.get(0),
        )
        .optional()?;
    match existing {
        Some(_) => rotate_account_session_token(conn, account_id),
        None => insert_account_session_token(conn, account_id),
    }
}

/// Revoke the presented session token (logout). Returns whether a row was deleted.
///
/// # Errors
///
/// Returns an error when the delete fails.
pub fn revoke_session_token(conn: &Connection, token: &str) -> Result<bool> {
    let token_hash = hash_api_token(token);
    let n = conn.execute(
        "DELETE FROM account_session_tokens WHERE token_hash = ?1",
        params![token_hash],
    )?;
    Ok(n > 0)
}

pub(crate) fn unix_secs_string() -> String {
    format!("{}", now_unix_secs())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::schema;

    #[test]
    fn hash_is_stable_hex() {
        let h = hash_api_token("mv-user-abc");
        assert_eq!(h.len(), 64);
        assert_eq!(h, hash_api_token("mv-user-abc"));
        assert_ne!(h, hash_api_token("mv-user-xyz"));
    }

    #[test]
    fn generate_prefixed_token_uses_os_entropy() {
        let a = generate_prefixed_token("mv-user-").unwrap();
        let b = generate_prefixed_token("mv-user-").unwrap();
        assert!(a.starts_with("mv-user-"));
        assert_eq!(a.len(), "mv-user-".len() + 32);
        assert_ne!(a, b);
    }

    #[test]
    fn lookup_rejects_expired_session() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        schema::ensure_accounts_schema(&conn).unwrap();
        conn.execute(
            "INSERT INTO accounts (id, username) VALUES ('a1', 'alice')",
            [],
        )
        .unwrap();
        let token = insert_account_session_token(&conn, "a1").unwrap();
        // Force expiry into the past.
        conn.execute("UPDATE account_session_tokens SET expires_at = '1'", [])
            .unwrap();
        assert!(lookup_account_for_token(&conn, &token).unwrap().is_none());
    }

    #[test]
    fn insert_session_with_ttl_sets_expires_at() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        schema::ensure_accounts_schema(&conn).unwrap();
        conn.execute(
            "INSERT INTO accounts (id, username) VALUES ('a1', 'alice')",
            [],
        )
        .unwrap();
        let before = now_unix_secs();
        let token = insert_account_session_token_with_ttl(&conn, "a1", 120).unwrap();
        assert!(token.starts_with("mv-user-"));
        let expires: String = conn
            .query_row(
                "SELECT expires_at FROM account_session_tokens WHERE account_id = 'a1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        let exp: u64 = expires.parse().unwrap();
        assert!(exp >= before + 120);
        assert!(exp <= before + 130);
    }

    #[test]
    fn revoke_session_token_removes_row() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        schema::ensure_accounts_schema(&conn).unwrap();
        conn.execute(
            "INSERT INTO accounts (id, username) VALUES ('a1', 'alice')",
            [],
        )
        .unwrap();
        let token = insert_account_session_token(&conn, "a1").unwrap();
        assert!(lookup_account_for_token(&conn, &token).unwrap().is_some());
        assert!(revoke_session_token(&conn, &token).unwrap());
        assert!(lookup_account_for_token(&conn, &token).unwrap().is_none());
    }
}
