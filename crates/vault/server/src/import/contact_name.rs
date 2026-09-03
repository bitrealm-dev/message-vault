//! Contact linking and display-name merging during import.

use anyhow::Result;
use message_ir::HandleType;
use sqlx::AnyConnection;

use super::ImportStats;
use super::staging::nonempty_str;
use crate::db::contacts;
use crate::db::handles::{
    HandleIdCache, infer_handle_type_from_shape as infer_handle_type, upsert_handle_row_cached,
};

/// The contact that owns `handle_id`, creating one when nothing owns it yet.
///
/// Every participant an import meets becomes a contact. ADR-0006: a backup is
/// an address book the person already curated, so the name it supplies goes on
/// the contact — on creation, or later if an earlier backup left the contact
/// nameless. A contact that already has a name is untouched, because the same
/// number arrives spelled differently across backups and the first spelling is
/// as good as the second. A contact the person made or an address book loaded
/// is never renamed by an import.
pub(super) async fn ensure_contact_for_handle(
    tx: &mut AnyConnection,
    account_id: &str,
    handle_id: i64,
    backup_name: Option<&str>,
    stats: &mut ImportStats,
) -> Result<i64> {
    let name = nonempty_str(backup_name).unwrap_or("");
    if let Some(existing) = ensure_sibling_contact_link(tx, account_id, handle_id).await? {
        if !name.is_empty() {
            name_nameless_import_contact(tx, account_id, existing, name).await?;
        }
        return Ok(existing);
    }
    let contact_id =
        contacts::create_contact(tx, account_id, name, contacts::Origin::Import).await?;
    contacts::link_handle_to_contact(
        tx,
        account_id,
        handle_id,
        contact_id,
        contacts::Origin::Import,
    )
    .await?;
    stats.contacts_created += 1;
    Ok(contact_id)
}

/// Put the backup's name on a contact an earlier import left nameless.
///
/// The `origin = 'import'` clause is what keeps a typed or address-book name
/// safe, and the empty-name clause is what makes the first backup win.
async fn name_nameless_import_contact(
    conn: &mut AnyConnection,
    account_id: &str,
    contact_id: i64,
    name: &str,
) -> Result<()> {
    let updated = sqlx::query(
        "UPDATE contacts
         SET preferred_name = $1
         WHERE account_id = $2 AND id = $3
           AND origin = 'import'
           AND trim(preferred_name) = ''",
    )
    .bind(name)
    .bind(account_id)
    .bind(contact_id)
    .execute(&mut *conn)
    .await?
    .rows_affected();
    if updated > 0 {
        contacts::touch_contact(conn, account_id, contact_id).await?;
    }
    Ok(())
}

/// Bind a participant the source named without recording any address.
///
/// A single existing contact under that name is reused, so the same person
/// named across several conversations does not become several contacts. When
/// no contact matches — or when two do, which is ambiguous — a contact is
/// created carrying the name and no identity. Either way the result is Unknown
/// until the person supplies an address for them.
///
/// Returns the contact and the display name to record on the participant.
pub(super) async fn resolve_name_only_participant(
    tx: &mut AnyConnection,
    account_id: &str,
    name: Option<&str>,
) -> Result<(Option<i64>, Option<String>)> {
    let Some(name) = nonempty_str(name) else {
        // A participant with neither an address nor a name says nothing at
        // all; there is nothing to create and nothing to show.
        return Ok((None, None));
    };
    if let Some(existing) = contacts::contact_id_by_preferred_name(tx, account_id, name).await? {
        return Ok((Some(existing), Some(name.to_string())));
    }
    let contact_id =
        contacts::create_contact(tx, account_id, name, contacts::Origin::Import).await?;
    Ok((Some(contact_id), Some(name.to_string())))
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
    let (handle_id, flagged, cached) =
        upsert_handle_row_cached(tx, cache, account_id, sender, handle_type, Some(platform))
            .await?;
    if flagged {
        stats.phones_needing_review += 1;
    }
    if !cached {
        let _ = ensure_sibling_contact_link(tx, account_id, handle_id).await?;
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::schema;

    const TEST_ACCOUNT: &str = "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa";

    #[tokio::test]
    async fn an_import_creates_the_contact_with_the_backup_name() {
        let (pool, _dir) = crate::db::engine::test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        schema::ensure_vault_schema(&mut conn).await.unwrap();
        crate::db::account_profile::ensure_account_row(&mut conn, TEST_ACCOUNT)
            .await
            .unwrap();
        let handle_id: i64 = sqlx::query_scalar(
            "INSERT INTO handles (account_id, raw, normalized, handle_type, service)
             VALUES ($1, '+15555550700', '+15555550700', 'phone', 'imessage') RETURNING id",
        )
        .bind(TEST_ACCOUNT)
        .fetch_one(&mut *conn)
        .await
        .unwrap();

        let mut stats = ImportStats::default();
        let contact_id =
            ensure_contact_for_handle(&mut conn, TEST_ACCOUNT, handle_id, Some("Ada"), &mut stats)
                .await
                .unwrap();

        let (name, origin): (String, String) =
            sqlx::query_as("SELECT preferred_name, origin FROM contacts WHERE id = $1")
                .bind(contact_id)
                .fetch_one(&mut *conn)
                .await
                .unwrap();
        assert_eq!(name, "Ada");
        assert_eq!(origin, "import");
    }

    #[tokio::test]
    async fn a_later_backup_names_a_contact_an_earlier_one_left_nameless() {
        let (pool, _dir) = crate::db::engine::test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        schema::ensure_vault_schema(&mut conn).await.unwrap();
        crate::db::account_profile::ensure_account_row(&mut conn, TEST_ACCOUNT)
            .await
            .unwrap();
        let handle_id: i64 = sqlx::query_scalar(
            "INSERT INTO handles (account_id, raw, normalized, handle_type, service)
             VALUES ($1, '+15555550800', '+15555550800', 'phone', 'imessage') RETURNING id",
        )
        .bind(TEST_ACCOUNT)
        .fetch_one(&mut *conn)
        .await
        .unwrap();

        let mut stats = ImportStats::default();
        let first = ensure_contact_for_handle(&mut conn, TEST_ACCOUNT, handle_id, None, &mut stats)
            .await
            .unwrap();
        let second =
            ensure_contact_for_handle(&mut conn, TEST_ACCOUNT, handle_id, Some("Ada"), &mut stats)
                .await
                .unwrap();
        assert_eq!(first, second, "the same handle keeps the same contact");

        let name: String = sqlx::query_scalar("SELECT preferred_name FROM contacts WHERE id = $1")
            .bind(first)
            .fetch_one(&mut *conn)
            .await
            .unwrap();
        assert_eq!(name, "Ada");
    }

    #[tokio::test]
    async fn a_second_spelling_does_not_rename_anyone() {
        let (pool, _dir) = crate::db::engine::test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        schema::ensure_vault_schema(&mut conn).await.unwrap();
        crate::db::account_profile::ensure_account_row(&mut conn, TEST_ACCOUNT)
            .await
            .unwrap();
        let handle_id: i64 = sqlx::query_scalar(
            "INSERT INTO handles (account_id, raw, normalized, handle_type, service)
             VALUES ($1, '+15555550900', '+15555550900', 'phone', 'imessage') RETURNING id",
        )
        .bind(TEST_ACCOUNT)
        .fetch_one(&mut *conn)
        .await
        .unwrap();

        let mut stats = ImportStats::default();
        let contact_id = ensure_contact_for_handle(
            &mut conn,
            TEST_ACCOUNT,
            handle_id,
            Some("Ada Lovelace"),
            &mut stats,
        )
        .await
        .unwrap();
        ensure_contact_for_handle(
            &mut conn,
            TEST_ACCOUNT,
            handle_id,
            Some("ada l"),
            &mut stats,
        )
        .await
        .unwrap();

        let name: String = sqlx::query_scalar("SELECT preferred_name FROM contacts WHERE id = $1")
            .bind(contact_id)
            .fetch_one(&mut *conn)
            .await
            .unwrap();
        assert_eq!(name, "Ada Lovelace", "first backup wins");
    }

    /// A name the person typed carries `origin = 'user'` and outranks any
    /// backup, however many imports later run.
    #[tokio::test]
    async fn an_import_does_not_overwrite_a_name_the_person_typed() {
        let (pool, _dir) = crate::db::engine::test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        schema::ensure_vault_schema(&mut conn).await.unwrap();
        crate::db::account_profile::ensure_account_row(&mut conn, TEST_ACCOUNT)
            .await
            .unwrap();
        let handle_id: i64 = sqlx::query_scalar(
            "INSERT INTO handles (account_id, raw, normalized, handle_type, service)
             VALUES ($1, '+15555551000', '+15555551000', 'phone', 'imessage') RETURNING id",
        )
        .bind(TEST_ACCOUNT)
        .fetch_one(&mut *conn)
        .await
        .unwrap();
        let contact_id = crate::db::contacts::create_contact(
            &mut conn,
            TEST_ACCOUNT,
            "",
            crate::db::contacts::Origin::User,
        )
        .await
        .unwrap();
        crate::db::contacts::link_handle_to_contact(
            &mut conn,
            TEST_ACCOUNT,
            handle_id,
            contact_id,
            crate::db::contacts::Origin::User,
        )
        .await
        .unwrap();

        let mut stats = ImportStats::default();
        ensure_contact_for_handle(&mut conn, TEST_ACCOUNT, handle_id, Some("Ada"), &mut stats)
            .await
            .unwrap();

        let name: String = sqlx::query_scalar("SELECT preferred_name FROM contacts WHERE id = $1")
            .bind(contact_id)
            .fetch_one(&mut *conn)
            .await
            .unwrap();
        assert_eq!(name, "", "the person's contact is not the import's to name");
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
