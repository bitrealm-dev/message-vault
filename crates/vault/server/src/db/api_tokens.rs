//! Named CLI API tokens (`mv-api-…`); many per account, import/export scoped.

use anyhow::{Context, Result, bail};
use rusqlite::{Connection, OptionalExtension, params};

use super::session_tokens::hash_api_token;

const API_TOKEN_ALPHANUM: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";

/// Access granted to a named API token.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApiTokenScopes {
    Import,
    Export,
    Both,
}

impl ApiTokenScopes {
    pub fn parse(raw: &str) -> Result<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "import" => Ok(Self::Import),
            "export" => Ok(Self::Export),
            "both" | "import_export" | "import-export" => Ok(Self::Both),
            other => bail!("scopes must be import, export, or both (got {other})"),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Import => "import",
            Self::Export => "export",
            Self::Both => "both",
        }
    }

    pub fn allows_import(self) -> bool {
        matches!(self, Self::Import | Self::Both)
    }

    pub fn allows_export(self) -> bool {
        matches!(self, Self::Export | Self::Both)
    }
}

/// Metadata for one API token (never includes plaintext or hash).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiTokenRow {
    pub id: String,
    pub label: String,
    pub scopes: ApiTokenScopes,
    /// Masked secret for Settings, e.g. `mv-api-Sd..mE`.
    pub token_hint: String,
    pub created_at: String,
    /// Unix-seconds string when the token was last used; `None` if never.
    pub last_accessed_at: Option<String>,
}

const API_TOKEN_PREFIX: &str = "mv-api-";
const LEGACY_APP_PASSWORD_PREFIX: &str = "mv-app-";
const HINT_HEAD: usize = 2;
const HINT_TAIL: usize = 2;

/// Mask a plaintext API token for list display (keeps `mv-api-` or legacy `mv-app-` + ends).
/// Format: `mv-api-xx..yy`.
pub fn mask_api_token(token: &str) -> String {
    let (prefix, secret) = if let Some(s) = token.strip_prefix(API_TOKEN_PREFIX) {
        (API_TOKEN_PREFIX, s)
    } else if let Some(s) = token.strip_prefix(LEGACY_APP_PASSWORD_PREFIX) {
        (LEGACY_APP_PASSWORD_PREFIX, s)
    } else {
        return format!("{API_TOKEN_PREFIX}..");
    };
    if secret.len() < HINT_HEAD + HINT_TAIL {
        return format!("{prefix}..");
    }
    let head = &secret[..HINT_HEAD];
    let tail = &secret[secret.len() - HINT_TAIL..];
    format!("{prefix}{head}..{tail}")
}

/// Account + scopes for a presented API token Bearer value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiTokenAuth {
    pub account_id: String,
    pub scopes: ApiTokenScopes,
}

/// Generate a new API token (`mv-api-` + 32 alphanumeric characters).
pub fn generate_api_token() -> String {
    let mut buf = [0u8; 32];
    fill_random(&mut buf);
    let mut suffix = String::with_capacity(32);
    for b in buf {
        suffix.push(API_TOKEN_ALPHANUM[(b as usize) % API_TOKEN_ALPHANUM.len()] as char);
    }
    format!("mv-api-{suffix}")
}

/// Look up which account owns this API token Bearer value.
/// On a successful match, updates `last_accessed_at`.
pub fn lookup_account_for_api_token(
    conn: &Connection,
    token: &str,
) -> Result<Option<ApiTokenAuth>> {
    let token_hash = hash_api_token(token);
    let found: Option<(String, String)> = conn
        .query_row(
            "SELECT account_id, scopes FROM account_api_tokens WHERE token_hash = ?1",
            params![token_hash],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    match found {
        Some((account_id, scopes_raw)) => {
            conn.execute(
                "UPDATE account_api_tokens SET last_accessed_at = ?1 WHERE token_hash = ?2",
                params![now_secs(), token_hash],
            )
            .with_context(|| "update API token last_accessed_at")?;
            Ok(Some(ApiTokenAuth {
                account_id,
                scopes: ApiTokenScopes::parse(&scopes_raw)?,
            }))
        }
        None => Ok(None),
    }
}

/// Create a named API token. Returns `(id, label, scopes, created_at, plaintext_token)`.
pub fn create_api_token(
    conn: &Connection,
    account_id: &str,
    label: &str,
    scopes: ApiTokenScopes,
) -> Result<(String, String, ApiTokenScopes, String, String)> {
    let label = label.trim();
    if label.is_empty() {
        bail!("label is required");
    }
    if label.len() > 120 {
        bail!("label must be at most 120 characters");
    }
    let id = uuid::Uuid::new_v4().to_string();
    let token = generate_api_token();
    let token_hash = hash_api_token(&token);
    let token_hint = mask_api_token(&token);
    let created_at = now_secs();
    let label_owned = label.to_string();
    conn.execute(
        r#"
        INSERT INTO account_api_tokens
            (id, account_id, label, token_hash, scopes, token_hint, created_at)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
        "#,
        params![
            id,
            account_id,
            label_owned,
            token_hash,
            scopes.as_str(),
            token_hint,
            created_at
        ],
    )
    .with_context(|| format!("insert API token for {account_id}"))?;
    Ok((id, label_owned, scopes, created_at, token))
}

/// List API tokens for an account (no secrets).
pub fn list_api_tokens(conn: &Connection, account_id: &str) -> Result<Vec<ApiTokenRow>> {
    let mut stmt = conn.prepare(
        r#"
        SELECT id, label, scopes, token_hint, created_at, last_accessed_at
        FROM account_api_tokens
        WHERE account_id = ?1
        ORDER BY created_at DESC, label COLLATE NOCASE
        "#,
    )?;
    let rows = stmt
        .query_map(params![account_id], |row| {
            let scopes_raw: String = row.get(2)?;
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                scopes_raw,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, Option<String>>(5)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let mut out = Vec::with_capacity(rows.len());
    for (id, label, scopes_raw, token_hint, created_at, last_accessed_at) in rows {
        out.push(ApiTokenRow {
            id,
            label,
            scopes: ApiTokenScopes::parse(&scopes_raw)?,
            token_hint,
            created_at,
            last_accessed_at,
        });
    }
    Ok(out)
}

/// Delete one API token if it belongs to the account.
pub fn delete_api_token(conn: &Connection, account_id: &str, id: &str) -> Result<bool> {
    let n = conn
        .execute(
            "DELETE FROM account_api_tokens WHERE id = ?1 AND account_id = ?2",
            params![id, account_id],
        )
        .with_context(|| format!("delete API token {id} for {account_id}"))?;
    Ok(n > 0)
}

/// Rename an API token label if it belongs to the account.
pub fn update_api_token_label(
    conn: &Connection,
    account_id: &str,
    id: &str,
    label: &str,
) -> Result<bool> {
    let label = label.trim();
    if label.is_empty() {
        bail!("label is required");
    }
    if label.len() > 120 {
        bail!("label must be at most 120 characters");
    }
    let n = conn
        .execute(
            "UPDATE account_api_tokens SET label = ?1 WHERE id = ?2 AND account_id = ?3",
            params![label, id, account_id],
        )
        .with_context(|| format!("rename API token {id} for {account_id}"))?;
    Ok(n > 0)
}

fn now_secs() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("{secs}")
}

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
        use sha2::{Digest, Sha256};
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::schema;

    fn setup() -> (Connection, String) {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        schema::ensure_accounts_schema(&conn).unwrap();
        let account_id = "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa";
        conn.execute(
            "INSERT INTO accounts (id, username) VALUES (?1, 'alice')",
            params![account_id],
        )
        .unwrap();
        (conn, account_id.to_string())
    }

    #[test]
    fn create_list_lookup_delete() {
        let (conn, account_id) = setup();
        let (id, _label, scopes, _created_at, token) =
            create_api_token(&conn, &account_id, " laptop CLI ", ApiTokenScopes::Export).unwrap();
        assert!(token.starts_with("mv-api-"));
        assert_eq!(scopes, ApiTokenScopes::Export);
        assert_eq!(
            mask_api_token("mv-api-Sd1abcdefghijklmnopqrsmtuvwxyZmE"),
            "mv-api-Sd..mE"
        );
        assert_eq!(
            mask_api_token("mv-app-Sd1abcdefghijklmnopqrsmtuvwxyZmE"),
            "mv-app-Sd..mE"
        );

        let listed = list_api_tokens(&conn, &account_id).unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, id);
        assert_eq!(listed[0].label, "laptop CLI");
        assert_eq!(listed[0].scopes, ApiTokenScopes::Export);
        assert_eq!(listed[0].token_hint, mask_api_token(&token));
        assert!(listed[0].last_accessed_at.is_none());

        let auth = lookup_account_for_api_token(&conn, &token)
            .unwrap()
            .unwrap();
        assert_eq!(auth.account_id, account_id);
        assert_eq!(auth.scopes, ApiTokenScopes::Export);

        let listed_after = list_api_tokens(&conn, &account_id).unwrap();
        assert!(listed_after[0].last_accessed_at.is_some());

        assert!(
            lookup_account_for_api_token(&conn, "mv-api-nope")
                .unwrap()
                .is_none()
        );

        assert!(delete_api_token(&conn, &account_id, &id).unwrap());
        assert!(list_api_tokens(&conn, &account_id).unwrap().is_empty());
        assert!(
            lookup_account_for_api_token(&conn, &token)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn empty_label_rejected() {
        let (conn, account_id) = setup();
        assert!(create_api_token(&conn, &account_id, "  ", ApiTokenScopes::Both).is_err());
    }

    #[test]
    fn rename_label() {
        let (conn, account_id) = setup();
        let (id, _, _, _, _) =
            create_api_token(&conn, &account_id, "old name", ApiTokenScopes::Both).unwrap();
        assert!(update_api_token_label(&conn, &account_id, &id, " new name ").unwrap());
        let listed = list_api_tokens(&conn, &account_id).unwrap();
        assert_eq!(listed[0].label, "new name");
        assert!(update_api_token_label(&conn, &account_id, &id, "  ").is_err());
        assert!(
            !update_api_token_label(&conn, "bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb", &id, "stolen")
                .unwrap()
        );
        assert_eq!(
            list_api_tokens(&conn, &account_id).unwrap()[0].label,
            "new name"
        );
    }
}
