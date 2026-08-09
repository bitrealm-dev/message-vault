//! GUI session Bearer tokens (`mv-user-…`); one per account, rotates on login.

use anyhow::{Context, Result};
use rusqlite::{Connection, OptionalExtension, params};
use sha2::{Digest, Sha256};

const SESSION_TOKEN_ALPHANUM: &[u8] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";

/// Generate a new GUI session token (`mv-user-` + 32 alphanumeric characters).
#[allow(dead_code)] // used by rotate_account_session_token
pub fn generate_session_token() -> String {
    let mut buf = [0u8; 32];
    fill_random(&mut buf);
    let mut suffix = String::with_capacity(32);
    for b in buf {
        suffix.push(SESSION_TOKEN_ALPHANUM[(b as usize) % SESSION_TOKEN_ALPHANUM.len()] as char);
    }
    format!("mv-user-{suffix}")
}

/// SHA-256 hex digest of a plaintext token (stored in DB; used for Bearer lookup).
pub fn hash_api_token(token: &str) -> String {
    Sha256::digest(token.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

#[allow(dead_code)]
fn fill_random(buf: &mut [u8]) {
    if getrandom_fill(buf) {
        return;
    }
    let mut seed = format!(
        "{}:{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0),
        std::process::id()
    )
    .into_bytes();
    let mut offset = 0;
    while offset < buf.len() {
        let digest = Sha256::digest(&seed);
        let n = (buf.len() - offset).min(digest.len());
        buf[offset..offset + n].copy_from_slice(&digest[..n]);
        offset += n;
        seed = digest.to_vec();
    }
}

#[allow(dead_code)]
fn getrandom_fill(buf: &mut [u8]) -> bool {
    use std::fs::File;
    use std::io::Read;
    let mut f = match File::open("/dev/urandom") {
        Ok(f) => f,
        Err(_) => return false,
    };
    f.read_exact(buf).is_ok()
}

/// Look up which account owns this session Bearer (by hash).
pub fn lookup_account_for_token(conn: &Connection, token: &str) -> Result<Option<String>> {
    let token_hash = hash_api_token(token);
    let found: Option<String> = conn
        .query_row(
            "SELECT account_id FROM account_session_tokens WHERE token_hash = ?1",
            params![token_hash],
            |row| row.get(0),
        )
        .optional()?;
    Ok(found)
}

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
#[allow(dead_code)]
pub fn rotate_account_session_token(conn: &Connection, account_id: &str) -> Result<String> {
    let token = generate_session_token();
    let token_hash = hash_api_token(&token);
    let created_at = chrono_like_now();
    conn.execute(
        r#"
        INSERT INTO account_session_tokens (account_id, token_hash, created_at)
        VALUES (?1, ?2, ?3)
        ON CONFLICT(account_id) DO UPDATE SET
            token_hash = excluded.token_hash,
            created_at = excluded.created_at
        "#,
        params![account_id, token_hash, created_at],
    )
    .with_context(|| format!("rotate session token for {account_id}"))?;
    Ok(token)
}

/// Create a fresh session token for an account and return the plaintext.
pub fn insert_account_session_token(conn: &Connection, account_id: &str) -> Result<String> {
    let token = generate_session_token();
    let token_hash = hash_api_token(&token);
    let created_at = chrono_like_now();
    conn.execute(
        "INSERT INTO account_session_tokens (account_id, token_hash, created_at) VALUES (?1, ?2, ?3)",
        params![account_id, token_hash, created_at],
    )
    .with_context(|| format!("insert session token for {account_id}"))?;
    Ok(token)
}

/// Session token for GUI: if a row exists, rotate it; otherwise insert.
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

#[allow(dead_code)]
pub fn delete_account_session_token(conn: &Connection, account_id: &str) -> Result<()> {
    conn.execute(
        "DELETE FROM account_session_tokens WHERE account_id = ?1",
        params![account_id],
    )
    .with_context(|| format!("delete session token for {account_id}"))?;
    Ok(())
}

#[allow(dead_code)]
fn chrono_like_now() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("{secs}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_is_stable_hex() {
        let h = hash_api_token("mv-user-abc");
        assert_eq!(h.len(), 64);
        assert_eq!(h, hash_api_token("mv-user-abc"));
        assert_ne!(h, hash_api_token("mv-user-xyz"));
    }
}
