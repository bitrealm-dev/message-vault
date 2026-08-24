//! Account rows, profile fields, guest status, and message deletion.

use anyhow::{Context, Result, bail};
use message_ir::HandleType;
use sqlx::AnyConnection;

use crate::db::dialect;
use crate::db::engine::DbEngine;
use crate::db::handles::{normalize_handle, upsert_handle_row};
use crate::db::schema;

/// Contact points linked to an account, for profile display.
#[derive(Debug, Clone)]
pub struct AccountProfile {
    /// Email addresses linked to the account.
    pub emails: Vec<String>,
    /// Phone handles linked to the account.
    pub phones: Vec<String>,
}

/// Load the email and phone handles linked to an account. Both default to empty
/// when nothing is linked.
pub async fn load_account_profile(
    conn: &mut AnyConnection,
    account_id: &str,
) -> Result<AccountProfile> {
    let emails = query_account_strings(
        conn,
        "SELECT email FROM account_emails WHERE account_id = $1 ORDER BY email",
        account_id,
    )
    .await?;
    let phones = query_account_strings(
        conn,
        "SELECT h.normalized FROM handles h
         JOIN account_handles ah ON ah.handle_id = h.id
         WHERE ah.account_id = $1 AND h.handle_type = 'phone'
         ORDER BY h.normalized",
        account_id,
    )
    .await?;
    Ok(AccountProfile { emails, phones })
}

async fn query_account_strings(
    conn: &mut AnyConnection,
    sql: &str,
    account_id: &str,
) -> Result<Vec<String>> {
    Ok(sqlx::query_scalar::<_, String>(sql)
        .bind(account_id)
        .fetch_all(&mut *conn)
        .await?)
}

/// Ensure `accounts` row exists (stub username = id) for CLI imports.
pub async fn ensure_account_row(conn: &mut AnyConnection, account_id: &str) -> Result<()> {
    sqlx::query(
        "INSERT INTO accounts (id, username, read_only) VALUES ($1, $1, 0)
         ON CONFLICT DO NOTHING",
    )
    .bind(account_id)
    .execute(&mut *conn)
    .await
    .with_context(|| format!("failed to ensure account row for {account_id}"))?;
    Ok(())
}

/// Ensure a `handles` row exists and link it to the account via `account_handles`.
/// Returns the handle id.
pub async fn link_account_handle(
    conn: &mut AnyConnection,
    account_id: &str,
    raw: &str,
    handle_type: HandleType,
) -> Result<i64> {
    link_account_handle_with_service(conn, account_id, raw, handle_type, None).await
}

/// Like [`link_account_handle`], recording a platform `service`
/// (`phone` | `whatsapp`). Missing/`None` defaults to `phone`.
pub async fn link_account_handle_with_service(
    conn: &mut AnyConnection,
    account_id: &str,
    raw: &str,
    handle_type: HandleType,
    service: Option<&str>,
) -> Result<i64> {
    let (handle_id, _) = upsert_handle_row(conn, account_id, raw, handle_type, service).await?;
    sqlx::query(
        "INSERT INTO account_handles (account_id, handle_id) VALUES ($1, $2)
         ON CONFLICT DO NOTHING",
    )
    .bind(account_id)
    .bind(handle_id)
    .execute(&mut *conn)
    .await?;
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
pub async fn lookup_account_ref(
    conn: &mut AnyConnection,
    account_ref: &str,
) -> Result<Option<String>> {
    let account_ref = account_ref.trim();
    if account_ref.is_empty() {
        return Ok(None);
    }
    schema::ensure_accounts_schema(conn).await?;

    let by_id: Option<String> = sqlx::query_scalar("SELECT id FROM accounts WHERE id = $1")
        .bind(account_ref)
        .fetch_optional(&mut *conn)
        .await?;
    if by_id.is_some() {
        return Ok(by_id);
    }

    // `COLLATE NOCASE` is SQLite-only; Postgres lowercases both sides (the
    // CI index from the schema is on `lower(username)`).
    let by_user: Option<String> = if dialect::engine_of(conn) == DbEngine::Postgres {
        sqlx::query_scalar("SELECT id FROM accounts WHERE lower(username) = lower($1)")
            .bind(account_ref)
            .fetch_optional(&mut *conn)
            .await?
    } else {
        sqlx::query_scalar("SELECT id FROM accounts WHERE username = $1 COLLATE NOCASE")
            .bind(account_ref)
            .fetch_optional(&mut *conn)
            .await?
    };
    Ok(by_user)
}

/// Resolve an account reference to `accounts.id` for import.
///
/// Accepts UUID or username. Unknown usernames error. Unknown UUID-shaped
/// values are returned as-is so CLI import can still stub-create the row.
pub async fn resolve_account_ref(conn: &mut AnyConnection, account_ref: &str) -> Result<String> {
    let account_ref = account_ref.trim();
    if account_ref.is_empty() {
        bail!("account is empty");
    }
    if let Some(id) = lookup_account_ref(conn, account_ref).await? {
        return Ok(id);
    }
    if looks_like_uuid(account_ref) {
        return Ok(account_ref.to_string());
    }
    bail!("account not found: {account_ref} (use an existing username or account UUID)");
}

/// Username for an account id, if the row exists.
pub async fn username_for_account(
    conn: &mut AnyConnection,
    account_id: &str,
) -> Result<Option<String>> {
    schema::ensure_accounts_schema(conn).await?;
    let name: Option<String> = sqlx::query_scalar("SELECT username FROM accounts WHERE id = $1")
        .bind(account_id)
        .fetch_optional(&mut *conn)
        .await?;
    Ok(name)
}

/// Load the argon2 password hash for an account id, if set.
///
/// Outer `Option` is "row missing"; inner is the nullable `password_hash`
/// column (NULL/empty means passwordless login).
pub async fn load_password_hash(
    conn: &mut AnyConnection,
    account_id: &str,
) -> Result<Option<String>> {
    let hash: Option<Option<String>> =
        sqlx::query_scalar("SELECT password_hash FROM accounts WHERE id = $1")
            .bind(account_id)
            .fetch_optional(&mut *conn)
            .await?;
    Ok(hash.flatten())
}

/// Replace the argon2 password hash for an account.
pub async fn update_password_hash(
    conn: &mut AnyConnection,
    account_id: &str,
    password_hash: &str,
) -> Result<()> {
    sqlx::query("UPDATE accounts SET password_hash = $1 WHERE id = $2")
        .bind(password_hash)
        .bind(account_id)
        .execute(&mut *conn)
        .await
        .with_context(|| format!("update password hash for {account_id}"))?;
    Ok(())
}

/// Permanently delete an account. All dependent rows are removed by
/// ON DELETE CASCADE (messages, conversations, contacts, vault_imports,
/// account_handles/emails/api_tokens).
pub async fn delete_account(conn: &mut AnyConnection, account_id: &str) -> Result<()> {
    sqlx::query("DELETE FROM accounts WHERE id = $1")
        .bind(account_id)
        .execute(&mut *conn)
        .await
        .with_context(|| format!("delete account {account_id}"))?;
    Ok(())
}

/// Stable id for the seeded demo account (`reset-demo`).
pub const DEMO_ACCOUNT_ID: &str = "00000000-0000-0000-0000-00000000d001";

/// True when `account_id` is the seeded demo account.
pub fn is_demo_account(account_id: &str) -> bool {
    account_id == DEMO_ACCOUNT_ID
}

/// Whether the account row is marked read-only (demo seed sets this).
pub async fn account_is_read_only(conn: &mut AnyConnection, account_id: &str) -> Result<bool> {
    schema::ensure_accounts_schema(conn).await?;
    let flag: Option<i64> = sqlx::query_scalar("SELECT read_only FROM accounts WHERE id = $1")
        .bind(account_id)
        .fetch_optional(&mut *conn)
        .await?;
    Ok(flag.unwrap_or(0) != 0)
}

/// Counts from deleting one account's messages.
#[derive(Debug, Clone, Copy)]
pub struct DeletedMessagesStats {
    /// Conversations deleted (cascade removes their messages).
    pub conversations: u64,
    /// Attachment rows deleted (files on disk are removed by the caller).
    pub attachments: u64,
}

/// Permanently delete one account's conversations (cascades to messages,
/// attachments, participants, tapbacks), staging rows, and trash markers.
/// Contacts, groups, login details, and import tokens are retained.
pub async fn delete_all_messages_for_account(
    conn: &mut AnyConnection,
    account_id: &str,
) -> Result<DeletedMessagesStats> {
    schema::ensure_vault_schema(conn).await?;
    let attachment_count: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(*)
        FROM attachments a
        JOIN messages m ON m.id = a.message_id
        JOIN conversations c ON c.id = m.conversation_id
        WHERE c.account_id = $1
        "#,
    )
    .bind(account_id)
    .fetch_one(&mut *conn)
    .await?;
    let conversations = sqlx::query("DELETE FROM conversations WHERE account_id = $1")
        .bind(account_id)
        .execute(&mut *conn)
        .await
        .with_context(|| format!("delete conversations for {account_id}"))?
        .rows_affected();
    sqlx::query("DELETE FROM staging_conversations WHERE account_id = $1")
        .bind(account_id)
        .execute(&mut *conn)
        .await
        .with_context(|| format!("delete staging conversations for {account_id}"))?;
    sqlx::query("DELETE FROM trashed_conversations WHERE account_id = $1")
        .bind(account_id)
        .execute(&mut *conn)
        .await
        .with_context(|| format!("delete trashed conversations for {account_id}"))?;
    sqlx::query("DELETE FROM trashed_handles WHERE account_id = $1")
        .bind(account_id)
        .execute(&mut *conn)
        .await
        .with_context(|| format!("delete trashed handles for {account_id}"))?;
    Ok(DeletedMessagesStats {
        conversations,
        attachments: u64::try_from(attachment_count).unwrap_or(0),
    })
}

/// Look up account id by Hanko user id. Returns None if no account is linked.
pub async fn lookup_account_by_hanko(
    conn: &mut AnyConnection,
    hanko_user_id: &str,
) -> Result<Option<String>> {
    schema::ensure_accounts_schema(conn).await?;
    let id: Option<String> = sqlx::query_scalar("SELECT id FROM accounts WHERE hanko_user_id = $1")
        .bind(hanko_user_id)
        .fetch_optional(&mut *conn)
        .await?;
    Ok(id)
}

/// Load the preferred_name for an account, if set.
pub async fn load_preferred_name(
    conn: &mut AnyConnection,
    account_id: &str,
) -> Result<Option<String>> {
    let name: Option<Option<String>> =
        sqlx::query_scalar("SELECT preferred_name FROM accounts WHERE id = $1")
            .bind(account_id)
            .fetch_optional(&mut *conn)
            .await?;
    Ok(name
        .flatten()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty()))
}

/// Insert a new account row. All fields except id and username are optional.
pub async fn insert_account(
    conn: &mut AnyConnection,
    id: &str,
    username: &str,
    password_hash: Option<&str>,
    preferred_name: Option<&str>,
    hanko_user_id: Option<&str>,
    read_only: bool,
) -> Result<()> {
    schema::ensure_accounts_schema(conn).await?;
    sqlx::query(
        "INSERT INTO accounts (id, username, read_only, password_hash, preferred_name, hanko_user_id) VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(id)
    .bind(username)
    .bind(read_only as i32)
    .bind(password_hash)
    .bind(preferred_name)
    .bind(hanko_user_id)
    .execute(&mut *conn)
    .await
    .with_context(|| format!("insert account {username}"))?;
    Ok(())
}

/// The account's `guest_status` value (`ready` or `assigned`), or `None` when
/// the account is not a guest.
pub async fn guest_status(conn: &mut AnyConnection, account_id: &str) -> Result<Option<String>> {
    schema::ensure_accounts_schema(conn).await?;
    let status: Option<Option<String>> =
        sqlx::query_scalar("SELECT guest_status FROM accounts WHERE id = $1")
            .bind(account_id)
            .fetch_optional(&mut *conn)
            .await?;
    Ok(status.flatten().filter(|s| !s.is_empty()))
}

/// True when the account has any guest status set.
pub async fn is_guest_account(conn: &mut AnyConnection, account_id: &str) -> Result<bool> {
    Ok(guest_status(conn, account_id).await?.is_some())
}

/// Insert a new guest account with status `ready`, no password, and
/// `read_only = 0`.
pub async fn insert_guest_account(
    conn: &mut AnyConnection,
    id: &str,
    username: &str,
    preferred_name: Option<&str>,
) -> Result<()> {
    schema::ensure_accounts_schema(conn).await?;
    sqlx::query(
        r#"
        INSERT INTO accounts (
            id, username, read_only, password_hash, preferred_name, guest_status
        ) VALUES ($1, $2, 0, NULL, $3, 'ready')
        "#,
    )
    .bind(id)
    .bind(username)
    .bind(preferred_name)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// Overwrite an account's `guest_status` value.
pub async fn set_guest_status(
    conn: &mut AnyConnection,
    account_id: &str,
    status: &str,
) -> Result<()> {
    sqlx::query("UPDATE accounts SET guest_status = $1 WHERE id = $2")
        .bind(status)
        .bind(account_id)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

/// Ensure a phone handle is linked to the account via `account_handles`.
pub async fn upsert_account_phone(
    conn: &mut AnyConnection,
    account_id: &str,
    phone: &str,
) -> Result<()> {
    link_account_handle(conn, account_id, phone, HandleType::Phone).await?;
    Ok(())
}

/// Upsert an account_emails row.
pub async fn upsert_account_email(
    conn: &mut AnyConnection,
    account_id: &str,
    email: &str,
    is_primary: bool,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO account_emails (account_id, email, is_primary) VALUES ($1, $2, $3)
         ON CONFLICT DO NOTHING",
    )
    .bind(account_id)
    .bind(email)
    .bind(is_primary as i32)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// Unlink a handle from the account profile (`account_handles`).
///
/// For emails, also removes the matching `account_emails` row. The underlying
/// `handles` row is left in place so conversation history stays intact.
pub async fn unlink_account_handle(
    conn: &mut AnyConnection,
    account_id: &str,
    raw: &str,
    handle_type: HandleType,
) -> Result<bool> {
    let (normalized, _) = normalize_handle(raw, handle_type);
    let handle_id: Option<i64> = sqlx::query_scalar(
        "SELECT id FROM handles
         WHERE account_id = $1 AND normalized = $2 AND handle_type = $3
         ORDER BY CASE service WHEN 'phone' THEN 0 WHEN 'whatsapp' THEN 1 ELSE 2 END
         LIMIT 1",
    )
    .bind(account_id)
    .bind(normalized.as_str())
    .bind(handle_type.as_str())
    .fetch_optional(&mut *conn)
    .await?;
    let Some(handle_id) = handle_id else {
        if matches!(handle_type, HandleType::Email) {
            let n = sqlx::query("DELETE FROM account_emails WHERE account_id = $1 AND email = $2")
                .bind(account_id)
                .bind(normalized.as_str())
                .execute(&mut *conn)
                .await?
                .rows_affected();
            return Ok(n > 0);
        }
        return Ok(false);
    };

    let removed =
        sqlx::query("DELETE FROM account_handles WHERE account_id = $1 AND handle_id = $2")
            .bind(account_id)
            .bind(handle_id)
            .execute(&mut *conn)
            .await?
            .rows_affected();
    if matches!(handle_type, HandleType::Email) {
        sqlx::query("DELETE FROM account_emails WHERE account_id = $1 AND email = $2")
            .bind(account_id)
            .bind(normalized.as_str())
            .execute(&mut *conn)
            .await?;
    }
    Ok(removed > 0)
}

/// Open the vault DB and resolve `account_ref` to a UUID.
pub async fn resolve_account_ref_at(
    db_path: &std::path::Path,
    account_ref: &str,
) -> Result<String> {
    let pool = crate::db::engine::open_pool_for_path(db_path)
        .await
        .with_context(|| format!("open database {}", db_path.display()))?;
    let mut conn = pool.acquire().await?;
    resolve_account_ref(&mut conn, account_ref).await
}

#[cfg(test)]
mod tests {
    use super::*;

    const ACCOUNT_ID: &str = "00000000-0000-4000-8000-000000000001";

    async fn setup() -> (sqlx::AnyPool, tempfile::TempDir) {
        let (pool, dir) = crate::db::engine::test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        schema::ensure_vault_schema(&mut conn).await.unwrap();
        sqlx::query("INSERT INTO accounts (id, username, read_only) VALUES ($1, $2, 0)")
            .bind(ACCOUNT_ID)
            .bind("Alice")
            .execute(&mut *conn)
            .await
            .unwrap();
        (pool, dir)
    }

    #[tokio::test]
    async fn resolve_by_username_case_insensitive() {
        let (pool, _dir) = setup().await;
        let mut conn = pool.acquire().await.unwrap();
        assert_eq!(
            resolve_account_ref(&mut conn, "alice").await.unwrap(),
            ACCOUNT_ID
        );
        assert_eq!(
            resolve_account_ref(&mut conn, "ALICE").await.unwrap(),
            ACCOUNT_ID
        );
    }

    #[tokio::test]
    async fn resolve_by_uuid() {
        let (pool, _dir) = setup().await;
        let mut conn = pool.acquire().await.unwrap();
        assert_eq!(
            resolve_account_ref(&mut conn, ACCOUNT_ID).await.unwrap(),
            ACCOUNT_ID
        );
    }

    #[tokio::test]
    async fn unknown_username_errors() {
        let (pool, _dir) = setup().await;
        let mut conn = pool.acquire().await.unwrap();
        let err = resolve_account_ref(&mut conn, "nobody")
            .await
            .unwrap_err()
            .to_string();
        assert!(err.contains("not found"), "{err}");
    }

    #[tokio::test]
    async fn unknown_uuid_passthrough() {
        let (pool, _dir) = setup().await;
        let mut conn = pool.acquire().await.unwrap();
        let id = "11111111-1111-4111-8111-111111111111";
        assert_eq!(resolve_account_ref(&mut conn, id).await.unwrap(), id);
    }

    #[tokio::test]
    async fn username_for_account_works() {
        let (pool, _dir) = setup().await;
        let mut conn = pool.acquire().await.unwrap();
        assert_eq!(
            username_for_account(&mut conn, ACCOUNT_ID)
                .await
                .unwrap()
                .as_deref(),
            Some("Alice")
        );
    }

    #[tokio::test]
    async fn load_password_hash_returns_none_when_null() {
        // Demo (and any passwordless account) stores password_hash as SQL NULL.
        // Reading that column must not fail with "Invalid column type Null".
        let (pool, _dir) = setup().await;
        let mut conn = pool.acquire().await.unwrap();
        let hash = load_password_hash(&mut conn, ACCOUNT_ID).await.unwrap();
        assert_eq!(hash, None);
    }

    #[tokio::test]
    async fn load_password_hash_returns_set_value() {
        let (pool, _dir) = setup().await;
        let mut conn = pool.acquire().await.unwrap();
        update_password_hash(&mut conn, ACCOUNT_ID, "$argon2id$example")
            .await
            .unwrap();
        let hash = load_password_hash(&mut conn, ACCOUNT_ID).await.unwrap();
        assert_eq!(hash.as_deref(), Some("$argon2id$example"));
    }

    #[tokio::test]
    async fn load_profile_returns_linked_handles_and_preferred_name() {
        let (pool, _dir) = setup().await;
        let mut conn = pool.acquire().await.unwrap();
        let empty = load_account_profile(&mut conn, ACCOUNT_ID).await.unwrap();
        assert!(empty.phones.is_empty());
        assert!(empty.emails.is_empty());
        assert_eq!(
            load_preferred_name(&mut conn, ACCOUNT_ID).await.unwrap(),
            None
        );

        sqlx::query("UPDATE accounts SET preferred_name = 'MB' WHERE id = $1")
            .bind(ACCOUNT_ID)
            .execute(&mut *conn)
            .await
            .unwrap();
        link_account_handle(&mut conn, ACCOUNT_ID, "+15555550100", HandleType::Phone)
            .await
            .unwrap();
        let loaded = load_account_profile(&mut conn, ACCOUNT_ID).await.unwrap();
        assert_eq!(loaded.phones, vec!["+15555550100".to_string()]);
        assert_eq!(
            load_preferred_name(&mut conn, ACCOUNT_ID).await.unwrap(),
            Some("MB".to_string())
        );
    }

    #[tokio::test]
    async fn link_account_handle_normalizes_and_dedupes() {
        let (pool, _dir) = setup().await;
        let mut conn = pool.acquire().await.unwrap();
        let a = link_account_handle(
            &mut conn,
            ACCOUNT_ID,
            "+1 (555) 555-0100",
            HandleType::Phone,
        )
        .await
        .unwrap();
        // Same normalized value with a different raw form reuses the handle row.
        let b = link_account_handle(&mut conn, ACCOUNT_ID, "+15555550100", HandleType::Phone)
            .await
            .unwrap();
        assert_eq!(a, b);
        let normalized: String = sqlx::query_scalar("SELECT normalized FROM handles WHERE id = $1")
            .bind(a)
            .fetch_one(&mut *conn)
            .await
            .unwrap();
        assert_eq!(normalized, "+15555550100");
        let linked: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM account_handles WHERE account_id = $1")
                .bind(ACCOUNT_ID)
                .fetch_one(&mut *conn)
                .await
                .unwrap();
        assert_eq!(linked, 1);
        // Email handles are lowercased and stored separately by type.
        let email = link_account_handle(&mut conn, ACCOUNT_ID, "ME@EXAMPLE.com", HandleType::Email)
            .await
            .unwrap();
        let linked_ids: Vec<i64> =
            sqlx::query_scalar("SELECT handle_id FROM account_handles WHERE account_id = $1")
                .bind(ACCOUNT_ID)
                .fetch_all(&mut *conn)
                .await
                .unwrap();
        assert_eq!(linked_ids.len(), 2);
        assert!(linked_ids.contains(&email));
    }

    #[tokio::test]
    async fn guest_helpers_work() {
        let (pool, _dir) = setup().await;
        let mut conn = pool.acquire().await.unwrap();
        let guest_id = "22222222-2222-4222-8222-222222222222";
        insert_guest_account(&mut conn, guest_id, "guest-abc", Some("Guest"))
            .await
            .unwrap();
        assert_eq!(
            guest_status(&mut conn, guest_id).await.unwrap().as_deref(),
            Some("ready")
        );
        assert!(is_guest_account(&mut conn, guest_id).await.unwrap());
        set_guest_status(&mut conn, guest_id, "assigned")
            .await
            .unwrap();
        assert_eq!(
            guest_status(&mut conn, guest_id).await.unwrap().as_deref(),
            Some("assigned")
        );
        assert!(!is_guest_account(&mut conn, ACCOUNT_ID).await.unwrap());
    }

    #[tokio::test]
    async fn delete_all_messages_keeps_account_and_contacts() {
        let (pool, _dir) = setup().await;
        let mut conn = pool.acquire().await.unwrap();
        let handle_id =
            link_account_handle(&mut conn, ACCOUNT_ID, "+15555550100", HandleType::Phone)
                .await
                .unwrap();
        sqlx::query("INSERT INTO contacts (account_id, preferred_name) VALUES ($1, 'Pat')")
            .bind(ACCOUNT_ID)
            .execute(&mut *conn)
            .await
            .unwrap();
        let contact_id: i64 = sqlx::query_scalar("SELECT id FROM contacts WHERE account_id = $1")
            .bind(ACCOUNT_ID)
            .fetch_one(&mut *conn)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO conversations (
                id, account_id, chat_handle_id, conversation_type, source_file
             ) VALUES (1, $1, $2, 'individual', 'c.jsonl')",
        )
        .bind(ACCOUNT_ID)
        .bind(handle_id)
        .execute(&mut *conn)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO messages (
                conversation_id, account_id, source, timestamp, is_from_me, sort_order, body
             ) VALUES (1, $1, 'imessage', '2020-01-01T00:00:00Z', 1, 0, 'hi')",
        )
        .bind(ACCOUNT_ID)
        .execute(&mut *conn)
        .await
        .unwrap();
        let msg_id: i64 = sqlx::query_scalar("SELECT id FROM messages WHERE account_id = $1")
            .bind(ACCOUNT_ID)
            .fetch_one(&mut *conn)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO attachments (message_id, path, original_name, mime_type)
             VALUES ($1, 'a.jpg', 'a.jpg', 'image/jpeg')",
        )
        .bind(msg_id)
        .execute(&mut *conn)
        .await
        .unwrap();

        let stats = delete_all_messages_for_account(&mut conn, ACCOUNT_ID)
            .await
            .unwrap();
        assert_eq!(stats.conversations, 1);
        assert_eq!(stats.attachments, 1);
        let remaining_msgs: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM messages WHERE account_id = $1")
                .bind(ACCOUNT_ID)
                .fetch_one(&mut *conn)
                .await
                .unwrap();
        assert_eq!(remaining_msgs, 0);
        let contacts: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM contacts WHERE id = $1")
            .bind(contact_id)
            .fetch_one(&mut *conn)
            .await
            .unwrap();
        assert_eq!(contacts, 1);
        assert!(
            username_for_account(&mut conn, ACCOUNT_ID)
                .await
                .unwrap()
                .is_some()
        );
    }
}
