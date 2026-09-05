//! Contact linking and display-name merging during import.

use anyhow::Result;
use message_ir::{HandleType, trimmed};
use sqlx::AnyConnection;

use super::ImportStats;
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
    let name = backup_name.and_then(trimmed).unwrap_or("");
    if let Some(existing) = ensure_sibling_contact_link(tx, account_id, handle_id).await? {
        // An import names only a contact an earlier import left nameless;
        // `contacts::propose_name` is where that rule and its two siblings
        // live.
        contacts::propose_name(tx, account_id, existing, name, contacts::Origin::Import).await?;
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
    let Some(name) = name.and_then(trimmed) else {
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

/// What one message says about who sent it. Its own type because these four
/// facts travel together and come from the message, while the connection,
/// handle cache, account and stats around them belong to the import run.
pub(super) struct IncomingSender<'a> {
    /// True when the account owner sent it, in which case there is no sender
    /// handle to resolve.
    pub is_from_me: bool,
    /// The sender's address as the backup recorded it, when it recorded one.
    pub address: Option<&'a str>,
    /// The address's type when the source stated it; inferred from the
    /// address's shape when it did not.
    pub handle_type: Option<HandleType>,
    /// Platform service the message arrived on, e.g. `imessage`.
    pub platform: &'a str,
}

/// The `handles` row for an incoming message's sender, creating it when this
/// import is the first to meet that address. `None` for a message the account
/// owner sent, and for one whose source recorded no sender address.
pub(super) async fn resolve_incoming_sender_handle(
    tx: &mut AnyConnection,
    cache: &mut HandleIdCache,
    account_id: &str,
    sender: IncomingSender<'_>,
    stats: &mut ImportStats,
) -> Result<Option<i64>> {
    if sender.is_from_me {
        return Ok(None);
    }
    let Some(address) = sender.address.and_then(trimmed) else {
        return Ok(None);
    };
    let handle_type = sender
        .handle_type
        .unwrap_or_else(|| infer_handle_type(address));
    let (handle_id, flagged, cached) = upsert_handle_row_cached(
        tx,
        cache,
        account_id,
        address,
        handle_type,
        Some(sender.platform),
    )
    .await?;
    if flagged {
        stats.phones_needing_review += 1;
    }
    if !cached {
        let _ = ensure_sibling_contact_link(tx, account_id, handle_id).await?;
    }
    Ok(Some(handle_id))
}

/// If this handle has no contact but a sibling handle (same normalized value
/// and type, different platform service) is already linked, attach this handle
/// to that contact.
pub(super) async fn ensure_sibling_contact_link(
    conn: &mut AnyConnection,
    account_id: &str,
    handle_id: i64,
) -> Result<Option<i64>> {
    if let Some(existing) = contacts::contact_id_for_handle(conn, account_id, handle_id).await? {
        return Ok(Some(existing));
    }
    let Some(contact_id) =
        contacts::contact_id_of_sibling_handle(conn, account_id, handle_id).await?
    else {
        return Ok(None);
    };
    if contacts::link_handle_to_contact(
        conn,
        account_id,
        handle_id,
        contact_id,
        contacts::Origin::Import,
    )
    .await?
    {
        contacts::touch_contact(conn, account_id, contact_id).await?;
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
