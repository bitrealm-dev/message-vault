use anyhow::{Context, Result};
use rusqlite::{Connection, OptionalExtension, params};

/// Generate a new import API token (`mv_` + 32 random bytes, url-safe base64).
pub fn generate_api_token() -> String {
    let mut buf = [0u8; 32];
    fill_random(&mut buf);
    format!("mv_{}", base64url_nopad(&buf))
}

fn fill_random(buf: &mut [u8]) {
    if getrandom_fill(buf) {
        return;
    }
    // Last-resort fallback (should be rare): mix time + pid into sha256 stream.
    use sha2::{Digest, Sha256};
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

fn getrandom_fill(buf: &mut [u8]) -> bool {
    use std::fs::File;
    use std::io::Read;
    let mut f = match File::open("/dev/urandom") {
        Ok(f) => f,
        Err(_) => return false,
    };
    f.read_exact(buf).is_ok()
}

fn base64url_nopad(bytes: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::with_capacity((bytes.len() * 4).div_ceil(3));
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = chunk.get(1).copied().unwrap_or(0) as u32;
        let b2 = chunk.get(2).copied().unwrap_or(0) as u32;
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(T[((n >> 18) & 63) as usize] as char);
        out.push(T[((n >> 12) & 63) as usize] as char);
        if chunk.len() > 1 {
            out.push(T[((n >> 6) & 63) as usize] as char);
        }
        if chunk.len() > 2 {
            out.push(T[(n & 63) as usize] as char);
        }
    }
    out
}

/// Look up which account owns this API token (exact match).
pub fn lookup_account_for_token(conn: &Connection, token: &str) -> Result<Option<String>> {
    let found: Option<String> = conn
        .query_row(
            "SELECT account_id FROM account_api_tokens WHERE token = ?1",
            params![token],
            |row| row.get(0),
        )
        .optional()?;
    Ok(found)
}

/// Return existing token or create one for the account.
pub fn ensure_account_api_token(conn: &Connection, account_id: &str) -> Result<String> {
    if let Some(existing) = lookup_token_for_account(conn, account_id)? {
        return Ok(existing);
    }
    let token = generate_api_token();
    let created_at = chrono_like_now();
    conn.execute(
        "INSERT INTO account_api_tokens (account_id, token, created_at) VALUES (?1, ?2, ?3)",
        params![account_id, token, created_at],
    )
    .with_context(|| format!("insert api token for {account_id}"))?;
    Ok(token)
}

pub fn lookup_token_for_account(conn: &Connection, account_id: &str) -> Result<Option<String>> {
    let found: Option<String> = conn
        .query_row(
            "SELECT token FROM account_api_tokens WHERE account_id = ?1",
            params![account_id],
            |row| row.get(0),
        )
        .optional()?;
    Ok(found)
}

/// Replace the account's API token; returns the new plaintext token.
#[allow(dead_code)] // used by web; available for future CLI tooling
pub fn rotate_account_api_token(conn: &Connection, account_id: &str) -> Result<String> {
    let token = generate_api_token();
    let created_at = chrono_like_now();
    conn.execute(
        r#"
        INSERT INTO account_api_tokens (account_id, token, created_at)
        VALUES (?1, ?2, ?3)
        ON CONFLICT(account_id) DO UPDATE SET
            token = excluded.token,
            created_at = excluded.created_at
        "#,
        params![account_id, token, created_at],
    )
    .with_context(|| format!("rotate api token for {account_id}"))?;
    Ok(token)
}

fn chrono_like_now() -> String {
    // RFC3339-ish UTC without chrono dep
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("{secs}")
}
