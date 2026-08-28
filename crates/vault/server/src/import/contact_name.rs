//! Contact linking and display-name merging during import.

use anyhow::{Result, bail};
use message_ir::HandleType;
use sqlx::AnyConnection;

use super::ImportStats;
use super::staging::nonempty_str;
use crate::db::contacts;
use crate::db::handles::{
    HandleIdCache, infer_handle_type_from_shape as infer_handle_type, upsert_handle_row_cached,
};

/// How account contacts supply participant display names during import.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ContactNameMode {
    /// Use the vault contact name only when the import name is empty.
    #[default]
    FillMissing,
    /// Prefer the vault contact name whenever one exists for the handle.
    Overwrite,
    /// Keep the import display name unchanged (including empty / unknown).
    AsIs,
}

impl ContactNameMode {
    /// Parse `fill_missing`, `overwrite`, or `as_is` (including hyphenated spellings).
    ///
    /// # Errors
    ///
    /// Returns an error when `s` is not one of those values.
    pub fn parse(s: &str) -> Result<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "" | "fill_missing" | "fill-missing" => Ok(Self::FillMissing),
            "overwrite" => Ok(Self::Overwrite),
            "as_is" | "as-is" | "leave" | "keep_import" | "keep-import" => Ok(Self::AsIs),
            other => bail!(
                "invalid contact_name_mode '{other}' (expected fill_missing, overwrite, or as_is)"
            ),
        }
    }
}

pub(super) async fn resolve_incoming_sender_handle(
    tx: &mut AnyConnection,
    cache: &mut HandleIdCache,
    account_id: &str,
    is_from_me: bool,
    sender: Option<&str>,
    handle_type: Option<HandleType>,
    platform: &str,
    stats: &mut ImportStats,
) -> Result<Option<i64>> {
    if is_from_me {
        return Ok(None);
    }
    let Some(sender) = nonempty_str(sender) else {
        return Ok(None);
    };
    let handle_type = handle_type.unwrap_or_else(|| infer_handle_type(sender));
    let (handle_id, flagged) =
        upsert_handle_row_cached(tx, cache, account_id, sender, handle_type, Some(platform))
            .await?;
    if flagged {
        stats.phones_needing_review += 1;
    }
    let _ = ensure_sibling_contact_link(tx, account_id, handle_id).await?;
    Ok(Some(handle_id))
}

/// If this handle has no contact but a sibling handle (same normalized + type,
/// different platform service) is already linked, attach this handle to that contact.
pub(super) async fn ensure_sibling_contact_link(
    conn: &mut AnyConnection,
    account_id: &str,
    handle_id: i64,
) -> Result<Option<i64>> {
    if let Some(existing) = contacts::contact_id_for_handle(conn, account_id, handle_id).await? {
        return Ok(Some(existing));
    }
    let sibling_contact: Option<i64> = sqlx::query_scalar(
        "SELECT ch.contact_id
         FROM handles h
         JOIN handles h2
           ON h2.account_id = h.account_id
          AND h2.normalized = h.normalized
          AND h2.handle_type = h.handle_type
          AND h2.id != h.id
         JOIN contact_handles ch
           ON ch.account_id = h.account_id AND ch.handle_id = h2.id
         WHERE h.id = $1 AND h.account_id = $2
         LIMIT 1",
    )
    .bind(handle_id)
    .bind(account_id)
    .fetch_optional(&mut *conn)
    .await?;
    let Some(contact_id) = sibling_contact else {
        return Ok(None);
    };
    let inserted = sqlx::query(
        "INSERT INTO contact_handles (account_id, handle_id, contact_id)
         VALUES ($1, $2, $3)
         ON CONFLICT DO NOTHING",
    )
    .bind(account_id)
    .bind(handle_id)
    .bind(contact_id)
    .execute(&mut *conn)
    .await?
    .rows_affected();
    if inserted > 0 {
        crate::db::contacts::touch_contact(conn, account_id, contact_id).await?;
    }
    Ok(Some(contact_id))
}

/// First-wins seed of `contact_handles.name_alias` from an import display name.
/// Only fills when the linked row exists and `name_alias` is empty.
pub(super) async fn seed_contact_handle_alias(
    conn: &mut AnyConnection,
    account_id: &str,
    handle_id: i64,
    import_display: Option<&str>,
) -> Result<()> {
    let Some(alias) = nonempty_str(import_display) else {
        return Ok(());
    };
    sqlx::query(
        "UPDATE contact_handles
         SET name_alias = $1
         WHERE account_id = $2
           AND handle_id = $3
           AND (name_alias IS NULL OR trim(name_alias) = '')",
    )
    .bind(alias)
    .bind(account_id)
    .bind(handle_id)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

pub(super) async fn contact_preferred_name(
    conn: &mut AnyConnection,
    account_id: &str,
    contact_id: i64,
) -> Result<Option<String>> {
    let name: Option<String> =
        sqlx::query_scalar("SELECT preferred_name FROM contacts WHERE account_id = $1 AND id = $2")
            .bind(account_id)
            .bind(contact_id)
            .fetch_optional(&mut *conn)
            .await?;
    Ok(trim_nonempty(name))
}

pub(super) fn trim_nonempty(value: Option<String>) -> Option<String> {
    let raw = value?;
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

/// Merge an import display name with a vault contact name per [`ContactNameMode`].
pub fn apply_contact_name_mode(
    mode: ContactNameMode,
    import_name: Option<String>,
    vault_name: Option<String>,
) -> Option<String> {
    let import_empty = match import_name.as_deref() {
        Some(s) => s.trim().is_empty(),
        None => true,
    };
    match mode {
        ContactNameMode::FillMissing => {
            if import_empty {
                vault_name.or(import_name)
            } else {
                import_name
            }
        }
        ContactNameMode::Overwrite => vault_name.or(import_name),
        ContactNameMode::AsIs => import_name,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::schema;

    const TEST_ACCOUNT: &str = "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa";

    #[test]
    fn apply_contact_name_mode_unit() {
        assert_eq!(
            apply_contact_name_mode(ContactNameMode::FillMissing, None, Some("Vault".into())),
            Some("Vault".into())
        );
        assert_eq!(
            apply_contact_name_mode(
                ContactNameMode::FillMissing,
                Some("Import".into()),
                Some("Vault".into())
            ),
            Some("Import".into())
        );
        assert_eq!(
            apply_contact_name_mode(
                ContactNameMode::Overwrite,
                Some("Import".into()),
                Some("Vault".into())
            ),
            Some("Vault".into())
        );
        assert_eq!(
            apply_contact_name_mode(ContactNameMode::Overwrite, Some("Import".into()), None),
            Some("Import".into())
        );
        assert_eq!(
            apply_contact_name_mode(ContactNameMode::AsIs, None, Some("Vault".into())),
            None
        );
        assert_eq!(
            apply_contact_name_mode(
                ContactNameMode::AsIs,
                Some("Import".into()),
                Some("Vault".into())
            ),
            Some("Import".into())
        );
    }

    #[tokio::test]
    async fn seed_contact_handle_alias_unit_first_wins() {
        let (pool, _dir) = crate::db::engine::test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        schema::ensure_vault_schema(&mut conn).await.unwrap();
        crate::db::account_profile::ensure_account_row(&mut conn, TEST_ACCOUNT)
            .await
            .unwrap();
        let contact_id: i64 = sqlx::query_scalar(
            "INSERT INTO contacts (account_id, preferred_name) VALUES ($1, 'Pat') RETURNING id",
        )
        .bind(TEST_ACCOUNT)
        .fetch_one(&mut *conn)
        .await
        .unwrap();
        let handle_id: i64 = sqlx::query_scalar(
            "INSERT INTO handles (account_id, raw, normalized, handle_type, service)
             VALUES ($1, '+15555550999', '+15555550999', 'phone', 'phone') RETURNING id",
        )
        .bind(TEST_ACCOUNT)
        .fetch_one(&mut *conn)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO contact_handles (account_id, handle_id, contact_id)
             VALUES ($1, $2, $3)",
        )
        .bind(TEST_ACCOUNT)
        .bind(handle_id)
        .bind(contact_id)
        .execute(&mut *conn)
        .await
        .unwrap();

        seed_contact_handle_alias(&mut conn, TEST_ACCOUNT, handle_id, Some("First"))
            .await
            .unwrap();
        seed_contact_handle_alias(&mut conn, TEST_ACCOUNT, handle_id, Some("Second"))
            .await
            .unwrap();
        let alias: Option<String> =
            sqlx::query_scalar("SELECT name_alias FROM contact_handles WHERE handle_id = $1")
                .bind(handle_id)
                .fetch_one(&mut *conn)
                .await
                .unwrap();
        assert_eq!(alias.as_deref(), Some("First"));
    }

    #[tokio::test]
    async fn sibling_contact_link_bumps_last_modified_only_on_insert() {
        let (pool, _dir) = crate::db::engine::test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        schema::ensure_vault_schema(&mut conn).await.unwrap();
        crate::db::account_profile::ensure_account_row(&mut conn, TEST_ACCOUNT)
            .await
            .unwrap();

        let contact_id: i64 = sqlx::query_scalar(
            "INSERT INTO contacts (account_id, preferred_name) VALUES ($1, 'Pat') RETURNING id",
        )
        .bind(TEST_ACCOUNT)
        .fetch_one(&mut *conn)
        .await
        .unwrap();
        let phone_id: i64 = sqlx::query_scalar(
            "INSERT INTO handles (account_id, raw, normalized, handle_type, service)
             VALUES ($1, '+15555550100', '+15555550100', 'phone', 'phone') RETURNING id",
        )
        .bind(TEST_ACCOUNT)
        .fetch_one(&mut *conn)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO contact_handles (account_id, handle_id, contact_id)
             VALUES ($1, $2, $3)",
        )
        .bind(TEST_ACCOUNT)
        .bind(phone_id)
        .bind(contact_id)
        .execute(&mut *conn)
        .await
        .unwrap();

        let wa_id: i64 = sqlx::query_scalar(
            "INSERT INTO handles (account_id, raw, normalized, handle_type, service)
             VALUES ($1, '+15555550100', '+15555550100', 'phone', 'whatsapp') RETURNING id",
        )
        .bind(TEST_ACCOUNT)
        .fetch_one(&mut *conn)
        .await
        .unwrap();

        const OLD: &str = "2000-01-01 00:00:00";
        sqlx::query("UPDATE contacts SET last_modified = $1 WHERE id = $2")
            .bind(OLD)
            .bind(contact_id)
            .execute(&mut *conn)
            .await
            .unwrap();

        let linked = ensure_sibling_contact_link(&mut conn, TEST_ACCOUNT, wa_id)
            .await
            .unwrap()
            .expect("sibling link");
        assert_eq!(linked, contact_id);
        let after_insert: String =
            sqlx::query_scalar("SELECT last_modified FROM contacts WHERE id = $1")
                .bind(contact_id)
                .fetch_one(&mut *conn)
                .await
                .unwrap();
        assert_ne!(after_insert, OLD);

        sqlx::query("UPDATE contacts SET last_modified = $1 WHERE id = $2")
            .bind(OLD)
            .bind(contact_id)
            .execute(&mut *conn)
            .await
            .unwrap();
        let again = ensure_sibling_contact_link(&mut conn, TEST_ACCOUNT, wa_id)
            .await
            .unwrap()
            .expect("already linked");
        assert_eq!(again, contact_id);
        let after_noop: String =
            sqlx::query_scalar("SELECT last_modified FROM contacts WHERE id = $1")
                .bind(contact_id)
                .fetch_one(&mut *conn)
                .await
                .unwrap();
        assert_eq!(after_noop, OLD);
    }
}
