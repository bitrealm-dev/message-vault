//! The one query that decides the name shown for a participant.
//!
//! ADR-0006: the Contact's name, else what that backup called them in that
//! conversation, else the handle. Every route that names a participant calls
//! [`load_for_conversations`], so one person cannot show two names on one
//! screen. `participants.contact_id` is deliberately not consulted — a handle
//! counts as a Contact's the moment it is on the Contact, which is what makes
//! naming someone rename them everywhere at once.

use std::collections::HashMap;

use serde::Serialize;
use sqlx::{AnyConnection, Row};

use crate::db::sql::group_rows_by_id;

/// One participant of a conversation, named by the rule above.
#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct Participant {
    /// What to show for this person. Never empty: the rule ends at the handle.
    pub name: String,
    /// Raw handle value (phone, email, or username).
    pub handle: String,
    /// Platform service, e.g. `imessage`.
    pub service: String,
    /// Linked vault contact id, when the handle is on a Contact. Matches the
    /// `id` every other contact shape uses, so a caller can compare the two
    /// without converting either.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contact_id: Option<i64>,
}

/// Participants of each conversation in `conversation_ids`, ordered by
/// participant id within a conversation.
///
/// # Errors
///
/// Returns a database error when the query fails.
pub async fn load_for_conversations(
    conn: &mut AnyConnection,
    conversation_ids: &[i64],
) -> Result<HashMap<i64, Vec<Participant>>, sqlx::Error> {
    group_rows_by_id(
        conn,
        conversation_ids,
        |placeholders| {
            format!(
                "SELECT p.conversation_id,
                        COALESCE(NULLIF(trim(c.preferred_name), ''),
                                 NULLIF(trim(p.name_alias), ''),
                                 h.raw) AS name,
                        h.raw AS handle,
                        COALESCE(NULLIF(trim(h.service), ''), h.handle_type) AS service,
                        ch.contact_id
                 FROM participants p
                 JOIN handles h ON h.id = p.handle_id
                 JOIN conversations conv ON conv.id = p.conversation_id
                 LEFT JOIN contact_handles ch
                   ON ch.handle_id = p.handle_id AND ch.account_id = conv.account_id
                 LEFT JOIN contacts c
                   ON c.id = ch.contact_id AND c.account_id = conv.account_id
                 WHERE p.conversation_id IN ({placeholders})
                 ORDER BY p.conversation_id, p.id"
            )
        },
        |row| {
            Ok((
                row.try_get::<i64, _>(0)?,
                Participant {
                    name: row.try_get(1)?,
                    handle: row.try_get(2)?,
                    service: row.try_get(3)?,
                    contact_id: row.try_get(4)?,
                },
            ))
        },
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::schema;

    const TEST_ACCOUNT: &str = "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa";

    /// Insert an account, one conversation, and one participant on `handle`
    /// whose backup name is `name_alias`. Returns (conversation_id, handle_id).
    async fn seed(
        conn: &mut sqlx::AnyConnection,
        handle: &str,
        name_alias: Option<&str>,
    ) -> (i64, i64) {
        schema::ensure_vault_schema(conn).await.unwrap();
        crate::db::account_profile::ensure_account_row(conn, TEST_ACCOUNT)
            .await
            .unwrap();
        let handle_id: i64 = sqlx::query_scalar(
            "INSERT INTO handles (account_id, raw, normalized, handle_type, service)
             VALUES ($1, $2, $2, 'phone', 'imessage') RETURNING id",
        )
        .bind(TEST_ACCOUNT)
        .bind(handle)
        .fetch_one(&mut *conn)
        .await
        .unwrap();
        let conversation_id: i64 = sqlx::query_scalar(
            "INSERT INTO conversations
                 (account_id, chat_handle_id, conversation_type, source_file)
             VALUES ($1, $2, 'individual', 'c.jsonl') RETURNING id",
        )
        .bind(TEST_ACCOUNT)
        .bind(handle_id)
        .fetch_one(&mut *conn)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO participants (conversation_id, handle_id, name_alias)
             VALUES ($1, $2, $3)",
        )
        .bind(conversation_id)
        .bind(handle_id)
        .bind(name_alias)
        .execute(&mut *conn)
        .await
        .unwrap();
        (conversation_id, handle_id)
    }

    async fn link(conn: &mut sqlx::AnyConnection, handle_id: i64, preferred_name: &str) -> i64 {
        let contact_id: i64 = sqlx::query_scalar(
            "INSERT INTO contacts (account_id, preferred_name) VALUES ($1, $2) RETURNING id",
        )
        .bind(TEST_ACCOUNT)
        .bind(preferred_name)
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
        contact_id
    }

    #[tokio::test]
    async fn contact_name_wins_over_the_backup_name() {
        let (pool, _dir) = crate::db::engine::test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        let (conversation_id, handle_id) = seed(&mut conn, "+15555550100", Some("Bobby")).await;
        let contact_id = link(&mut conn, handle_id, "Robert Smith").await;

        let loaded = load_for_conversations(&mut conn, &[conversation_id])
            .await
            .unwrap();
        let p = &loaded[&conversation_id][0];
        assert_eq!(p.name, "Robert Smith");
        assert_eq!(p.handle, "+15555550100");
        assert_eq!(p.service, "imessage");
        assert_eq!(p.contact_id, Some(contact_id));
    }

    #[tokio::test]
    async fn backup_name_shows_when_the_contact_has_none() {
        let (pool, _dir) = crate::db::engine::test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        let (conversation_id, handle_id) = seed(&mut conn, "+15555550200", Some("Bobby")).await;
        link(&mut conn, handle_id, "   ").await;

        let loaded = load_for_conversations(&mut conn, &[conversation_id])
            .await
            .unwrap();
        assert_eq!(loaded[&conversation_id][0].name, "Bobby");
    }

    #[tokio::test]
    async fn the_handle_shows_when_nothing_names_the_person() {
        let (pool, _dir) = crate::db::engine::test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        let (conversation_id, _handle_id) = seed(&mut conn, "+15555550300", None).await;

        let loaded = load_for_conversations(&mut conn, &[conversation_id])
            .await
            .unwrap();
        let p = &loaded[&conversation_id][0];
        assert_eq!(p.name, "+15555550300");
        assert_eq!(p.contact_id, None);
    }

    /// `participants.contact_id` is not consulted: only the link in
    /// `contact_handles` names someone, so naming a Contact renames them in
    /// every conversation at once.
    #[tokio::test]
    async fn a_participant_contact_id_does_not_name_anyone() {
        let (pool, _dir) = crate::db::engine::test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        let (conversation_id, handle_id) = seed(&mut conn, "+15555550400", Some("Bobby")).await;
        let stranger: i64 = sqlx::query_scalar(
            "INSERT INTO contacts (account_id, preferred_name) VALUES ($1, 'Wrong') RETURNING id",
        )
        .bind(TEST_ACCOUNT)
        .fetch_one(&mut *conn)
        .await
        .unwrap();
        sqlx::query("UPDATE participants SET contact_id = $1 WHERE handle_id = $2")
            .bind(stranger)
            .bind(handle_id)
            .execute(&mut *conn)
            .await
            .unwrap();

        let loaded = load_for_conversations(&mut conn, &[conversation_id])
            .await
            .unwrap();
        let p = &loaded[&conversation_id][0];
        assert_eq!(p.name, "Bobby");
        assert_eq!(p.contact_id, None);
    }
}
