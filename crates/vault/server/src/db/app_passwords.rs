//! Named app passwords for CLI import/export (separate from rotating session tokens).

use anyhow::{Context, Result, bail};
use rusqlite::{Connection, OptionalExtension, params};

use super::api_tokens::hash_api_token;

const APP_PASSWORD_ALPHANUM: &[u8] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";

/// Access granted to an app password.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppPasswordScopes {
    Import,
    Export,
    Both,
}

impl AppPasswordScopes {
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

    pub fn label(self) -> &'static str {
        match self {
            Self::Import => "Import",
            Self::Export => "Export",
            Self::Both => "Import + export",
        }
    }
}

/// Metadata for one app password (never includes plaintext or hash).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppPasswordRow {
    pub id: String,
    pub label: String,
    pub scopes: AppPasswordScopes,
    pub created_at: String,
}

/// Account + scopes for a presented app password Bearer value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppPasswordAuth {
    pub account_id: String,
    pub scopes: AppPasswordScopes,
}

/// Generate a new app password (`mv-app-` + 32 alphanumeric characters).
pub fn generate_app_password() -> String {
    let mut buf = [0u8; 32];
    fill_random(&mut buf);
    let mut suffix = String::with_capacity(32);
    for b in buf {
        suffix.push(APP_PASSWORD_ALPHANUM[(b as usize) % APP_PASSWORD_ALPHANUM.len()] as char);
    }
    format!("mv-app-{suffix}")
}

/// Look up which account owns this app password Bearer value.
pub fn lookup_account_for_app_password(
    conn: &Connection,
    token: &str,
) -> Result<Option<AppPasswordAuth>> {
    let token_hash = hash_api_token(token);
    let found: Option<(String, String)> = conn
        .query_row(
            "SELECT account_id, scopes FROM account_app_passwords WHERE token_hash = ?1",
            params![token_hash],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    match found {
        Some((account_id, scopes_raw)) => Ok(Some(AppPasswordAuth {
            account_id,
            scopes: AppPasswordScopes::parse(&scopes_raw)?,
        })),
        None => Ok(None),
    }
}

/// Create a named app password. Returns `(id, label, scopes, created_at, plaintext_token)`.
pub fn create_app_password(
    conn: &Connection,
    account_id: &str,
    label: &str,
    scopes: AppPasswordScopes,
) -> Result<(String, String, AppPasswordScopes, String, String)> {
    let label = label.trim();
    if label.is_empty() {
        bail!("label is required");
    }
    if label.len() > 120 {
        bail!("label must be at most 120 characters");
    }
    let id = uuid::Uuid::new_v4().to_string();
    let token = generate_app_password();
    let token_hash = hash_api_token(&token);
    let created_at = now_secs();
    let label_owned = label.to_string();
    conn.execute(
        r#"
        INSERT INTO account_app_passwords (id, account_id, label, token_hash, scopes, created_at)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6)
        "#,
        params![
            id,
            account_id,
            label_owned,
            token_hash,
            scopes.as_str(),
            created_at
        ],
    )
    .with_context(|| format!("insert app password for {account_id}"))?;
    Ok((id, label_owned, scopes, created_at, token))
}

/// List app passwords for an account (no secrets).
pub fn list_app_passwords(conn: &Connection, account_id: &str) -> Result<Vec<AppPasswordRow>> {
    let mut stmt = conn.prepare(
        r#"
        SELECT id, label, scopes, created_at
        FROM account_app_passwords
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
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let mut out = Vec::with_capacity(rows.len());
    for (id, label, scopes_raw, created_at) in rows {
        out.push(AppPasswordRow {
            id,
            label,
            scopes: AppPasswordScopes::parse(&scopes_raw)?,
            created_at,
        });
    }
    Ok(out)
}

/// Delete one app password if it belongs to the account.
pub fn delete_app_password(conn: &Connection, account_id: &str, id: &str) -> Result<bool> {
    let n = conn
        .execute(
            "DELETE FROM account_app_passwords WHERE id = ?1 AND account_id = ?2",
            params![id, account_id],
        )
        .with_context(|| format!("delete app password {id} for {account_id}"))?;
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
            create_app_password(&conn, &account_id, " laptop CLI ", AppPasswordScopes::Export)
                .unwrap();
        assert!(token.starts_with("mv-app-"));
        assert_eq!(scopes, AppPasswordScopes::Export);

        let listed = list_app_passwords(&conn, &account_id).unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, id);
        assert_eq!(listed[0].label, "laptop CLI");
        assert_eq!(listed[0].scopes, AppPasswordScopes::Export);

        let auth = lookup_account_for_app_password(&conn, &token).unwrap().unwrap();
        assert_eq!(auth.account_id, account_id);
        assert_eq!(auth.scopes, AppPasswordScopes::Export);
        assert!(lookup_account_for_app_password(&conn, "mv-app-nope")
            .unwrap()
            .is_none());

        assert!(delete_app_password(&conn, &account_id, &id).unwrap());
        assert!(list_app_passwords(&conn, &account_id).unwrap().is_empty());
        assert!(lookup_account_for_app_password(&conn, &token)
            .unwrap()
            .is_none());
    }

    #[test]
    fn empty_label_rejected() {
        let (conn, account_id) = setup();
        assert!(
            create_app_password(&conn, &account_id, "  ", AppPasswordScopes::Both).is_err()
        );
    }
}
