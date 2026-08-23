//! Named CLI API tokens (`mv-api-…`); many per account, import/export scoped.

use anyhow::{Context, Result, bail};
use rusqlite::{Connection, OptionalExtension, params};

use super::session_tokens::{generate_prefixed_token, hash_api_token, unix_secs_string};

/// Access granted to a named API token.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApiTokenScopes {
    /// Import endpoints only.
    Import,
    /// Export endpoints only.
    Export,
    /// Both import and export endpoints.
    Both,
}

impl ApiTokenScopes {
    /// Parse `import`, `export`, or `both` (including `import_export` spellings).
    ///
    /// # Errors
    ///
    /// Returns an error when `raw` is not one of those values.
    pub fn parse(raw: &str) -> Result<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "import" => Ok(Self::Import),
            "export" => Ok(Self::Export),
            "both" | "import_export" | "import-export" => Ok(Self::Both),
            other => bail!("scopes must be import, export, or both (got {other})"),
        }
    }

    /// Canonical scope string (`import`, `export`, or `both`) used in token labels.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Import => "import",
            Self::Export => "export",
            Self::Both => "both",
        }
    }

    /// True when this token may call import endpoints.
    pub fn allows_import(self) -> bool {
        matches!(self, Self::Import | Self::Both)
    }

    /// True when this token may call export endpoints.
    pub fn allows_export(self) -> bool {
        matches!(self, Self::Export | Self::Both)
    }
}

/// Metadata for one API token (never includes plaintext or hash).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiTokenRow {
    /// Token id (the secret itself is stored hashed, never in this row).
    pub id: String,
    /// User-chosen label shown in Settings.
    pub label: String,
    /// Access granted to the token.
    pub scopes: ApiTokenScopes,
    /// Masked secret for Settings, e.g. `mv-api-Sd..mE`.
    pub token_hint: String,
    /// Creation time as a Unix-seconds string.
    pub created_at: String,
    /// Unix-seconds string when the token was last used; `None` if never.
    pub last_accessed_at: Option<String>,
    /// Unix-seconds expiry; `None` means no expiry.
    pub expires_at: Option<String>,
    /// True when the token is disabled and rejects requests.
    pub disabled: bool,
}

/// Default API token lifetime when the client does not pass `expires_in_days` (365 days).
pub const DEFAULT_API_TOKEN_TTL_SECS: u64 = 365 * 24 * 60 * 60;

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
    /// Account the token belongs to.
    pub account_id: String,
    /// Access granted to the token.
    pub scopes: ApiTokenScopes,
}

/// Generate a new API token (`mv-api-` + 32 alphanumeric characters).
///
/// # Errors
///
/// Returns an error when random bytes cannot be generated.
pub fn generate_api_token() -> Result<String> {
    generate_prefixed_token("mv-api-")
}

/// Look up which account owns this API token Bearer value.
/// On a successful match, updates `last_accessed_at`. Expired or disabled
/// tokens are rejected.
///
/// # Errors
///
/// Returns an error when the lookup or last-accessed update fails.
pub fn lookup_account_for_api_token(
    conn: &Connection,
    token: &str,
) -> Result<Option<ApiTokenAuth>> {
    let token_hash = hash_api_token(token);
    let found: Option<(String, String, Option<String>, i64)> = conn
        .query_row(
            "SELECT account_id, scopes, expires_at, disabled
             FROM account_api_tokens WHERE token_hash = ?1",
            params![token_hash],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .optional()?;
    match found {
        Some((account_id, scopes_raw, expires_at, disabled)) => {
            if disabled != 0 {
                return Ok(None);
            }
            if let Some(exp) = expires_at.as_deref() {
                let exp_secs = exp.parse::<u64>().unwrap_or(0);
                let now = unix_secs_string().parse::<u64>().unwrap_or(0);
                let expired = exp_secs == 0 || exp_secs <= now;
                if expired {
                    return Ok(None);
                }
            }
            conn.execute(
                "UPDATE account_api_tokens SET last_accessed_at = ?1 WHERE token_hash = ?2",
                params![unix_secs_string(), token_hash],
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

/// Create a named API token. Returns `(id, label, scopes, created_at, expires_at, plaintext_token)`.
///
/// # Errors
///
/// Returns an error when the label is invalid or the insert fails.
pub fn create_api_token(
    conn: &Connection,
    account_id: &str,
    label: &str,
    scopes: ApiTokenScopes,
    expires_in_days: Option<u64>,
) -> Result<(
    String,
    String,
    ApiTokenScopes,
    String,
    Option<String>,
    String,
)> {
    let label = validate_api_token_label(label)?;
    let id = uuid::Uuid::new_v4().to_string();
    let token = generate_api_token()?;
    let token_hash = hash_api_token(&token);
    let token_hint = mask_api_token(&token);
    let created_at = unix_secs_string();
    let expires_at = api_token_expiry(expires_in_days, &created_at);
    let label_owned = label.to_string();
    conn.execute(
        r#"
        INSERT INTO account_api_tokens
            (id, account_id, label, token_hash, scopes, token_hint, created_at, expires_at, disabled)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 0)
        "#,
        params![
            id,
            account_id,
            label_owned,
            token_hash,
            scopes.as_str(),
            token_hint,
            created_at,
            expires_at
        ],
    )
    .with_context(|| format!("insert API token for {account_id}"))?;
    Ok((id, label_owned, scopes, created_at, expires_at, token))
}

/// List API tokens for an account (no secrets).
///
/// # Errors
///
/// Returns an error when the query fails or a stored scope value is invalid.
pub fn list_api_tokens(conn: &Connection, account_id: &str) -> Result<Vec<ApiTokenRow>> {
    let mut stmt = conn.prepare(
        r#"
        SELECT id, label, scopes, token_hint, created_at, last_accessed_at, expires_at, disabled
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
                row.get::<_, Option<String>>(6)?,
                row.get::<_, i64>(7)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let mut out = Vec::with_capacity(rows.len());
    for (id, label, scopes_raw, token_hint, created_at, last_accessed_at, expires_at, disabled) in
        rows
    {
        out.push(ApiTokenRow {
            id,
            label,
            scopes: ApiTokenScopes::parse(&scopes_raw)?,
            token_hint,
            created_at,
            last_accessed_at,
            expires_at,
            disabled: disabled != 0,
        });
    }
    Ok(out)
}

/// Delete one API token if it belongs to the account.
///
/// # Errors
///
/// Returns an error when the delete statement fails.
pub fn delete_api_token(conn: &Connection, account_id: &str, id: &str) -> Result<bool> {
    let n = conn
        .execute(
            "DELETE FROM account_api_tokens WHERE id = ?1 AND account_id = ?2",
            params![id, account_id],
        )
        .with_context(|| format!("delete API token {id} for {account_id}"))?;
    Ok(n > 0)
}

/// Delete every named API token belonging to an account.
///
/// # Errors
///
/// Returns an error when the delete statement fails.
pub fn delete_all_api_tokens(conn: &Connection, account_id: &str) -> Result<u64> {
    let deleted = conn
        .execute(
            "DELETE FROM account_api_tokens WHERE account_id = ?1",
            params![account_id],
        )
        .with_context(|| format!("delete all API tokens for {account_id}"))?;
    Ok(deleted as u64)
}

/// Rename an API token label if it belongs to the account.
///
/// # Errors
///
/// Returns an error when the label is invalid or the update fails.
pub fn update_api_token_label(
    conn: &Connection,
    account_id: &str,
    id: &str,
    label: &str,
) -> Result<bool> {
    let label = validate_api_token_label(label)?;
    let n = conn
        .execute(
            "UPDATE account_api_tokens SET label = ?1 WHERE id = ?2 AND account_id = ?3",
            params![label, id, account_id],
        )
        .with_context(|| format!("rename API token {id} for {account_id}"))?;
    Ok(n > 0)
}

fn api_token_expiry(expires_in_days: Option<u64>, created_at: &str) -> Option<String> {
    let now = created_at.parse::<u64>().unwrap_or(0);
    match expires_in_days {
        Some(0) => None, // caller asked for no expiry
        Some(days) => Some(format!(
            "{}",
            now.saturating_add(days.saturating_mul(86_400))
        )),
        None => Some(format!(
            "{}",
            now.saturating_add(DEFAULT_API_TOKEN_TTL_SECS)
        )),
    }
}

fn validate_api_token_label(label: &str) -> Result<&str> {
    let label = label.trim();
    if label.is_empty() {
        bail!("label is required");
    }
    if label.len() > 120 {
        bail!("label must be at most 120 characters");
    }
    Ok(label)
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
        let (id, _label, scopes, _created_at, _expires_at, token) = create_api_token(
            &conn,
            &account_id,
            " laptop CLI ",
            ApiTokenScopes::Export,
            None,
        )
        .unwrap();
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
        assert!(create_api_token(&conn, &account_id, "  ", ApiTokenScopes::Both, None).is_err());
    }

    #[test]
    fn rename_label() {
        let (conn, account_id) = setup();
        let (id, _, _, _, _, _) =
            create_api_token(&conn, &account_id, "old name", ApiTokenScopes::Both, None).unwrap();
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
