//! The one query that decides the name shown for a participant.
//!
//! ADR-0006: the Contact's name, else what that backup called them in that
//! conversation, else the handle. Every route that names a participant calls
//! [`load_for_conversations`], so one person cannot show two names on one
//! screen. `participants.contact_id` is deliberately not consulted — a handle
//! counts as a Contact's the moment it is on the Contact, which is what makes
//! naming someone rename them everywhere at once.
//!
//! [`load_for_chat_handle`] is the same rule for the one conversation shape
//! that has no participants rows to read: a backup that recorded the thread's
//! address and nothing about who was in it. It stands here rather than beside
//! its caller so both naming paths sit under this doc comment and cannot
//! drift apart.

use std::collections::HashMap;

use serde::Serialize;
use sqlx::{AnyConnection, Row};

use crate::db::sql::group_rows_by_id;

/// One participant of a conversation, carrying the name to show for them:
/// the Contact's name, else what that backup called them in that
/// conversation, else the handle.
#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct Participant {
    /// What to show for this person. Never empty: the rule ends at the handle.
    pub name: String,
    /// Raw handle value (phone, email, or username). `None` when the source
    /// named this person without recording any address for them.
    pub handle: Option<String>,
    /// Platform service, e.g. `imessage`. `None` for the same reason as
    /// `handle`: with no address there is nothing to carry a service on.
    pub service: Option<String>,
    /// Linked vault contact id: when the handle is on a Contact, or — for a
    /// participant with no handle — the contact `resolve_name_only_participant`
    /// bound the name to directly, since that is the only place the link is
    /// recorded for them. Matches the `id` every other contact shape uses, so
    /// a caller can compare the two without converting either.
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
                                 h.raw, '') AS name,
                        h.raw AS handle,
                        COALESCE(NULLIF(trim(h.service), ''), h.handle_type) AS service,
                        CASE WHEN p.handle_id IS NULL THEN p.contact_id ELSE ch.contact_id END
                          AS contact_id
                 FROM participants p
                 LEFT JOIN handles h ON h.id = p.handle_id
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

/// The conversation's chat handle as its sole participant, for a conversation
/// that has no participants rows at all.
///
/// Same rule, one clause shorter: with no participants row there is no
/// per-conversation backup name, so it is the Contact's name, else the handle.
/// The Contact is reached through `contact_handles` exactly as above, so a
/// person the vault has a name for is named here too and their row opens the
/// contact drawer instead of showing a bare phone number.
///
/// Returns an empty vector when the conversation has no chat handle row.
///
/// # Errors
///
/// Returns a database error when the query fails.
pub async fn load_for_chat_handle(
    conn: &mut AnyConnection,
    conversation_id: i64,
) -> Result<Vec<Participant>, sqlx::Error> {
    // `conv.chat_handle_id` is `NOT NULL`, so this join always matches and
    // `handle`/`service` are never actually absent here — the tuple types
    // just have to match `Participant`'s, which carry the address-less case
    // that only `load_for_conversations` can produce.
    let row: Option<(String, Option<String>, Option<String>, Option<i64>)> = sqlx::query_as(
        "SELECT COALESCE(NULLIF(trim(c.preferred_name), ''), h.raw) AS name,
                h.raw AS handle,
                COALESCE(NULLIF(trim(h.service), ''), h.handle_type) AS service,
                ch.contact_id
         FROM conversations conv
         JOIN handles h ON h.id = conv.chat_handle_id
         LEFT JOIN contact_handles ch
           ON ch.handle_id = h.id AND ch.account_id = conv.account_id
         LEFT JOIN contacts c
           ON c.id = ch.contact_id AND c.account_id = conv.account_id
         WHERE conv.id = $1",
    )
    .bind(conversation_id)
    .fetch_optional(&mut *conn)
    .await?;
    Ok(row
        .map(|(name, handle, service, contact_id)| {
            vec![Participant {
                name,
                handle,
                service,
                contact_id,
            }]
        })
        .unwrap_or_default())
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

    /// Insert an address-less participant on `conversation_id`: `handle_id
    /// IS NULL`, bound to a fresh contact carrying `name_alias`. This is the
    /// row shape `resolve_name_only_participant` produces — the contact link
    /// lives on `participants.contact_id` directly, since there is no handle
    /// for `contact_handles` to key on. Returns the contact id.
    async fn seed_address_less(
        conn: &mut sqlx::AnyConnection,
        conversation_id: i64,
        name_alias: &str,
    ) -> i64 {
        let contact_id: i64 = sqlx::query_scalar(
            "INSERT INTO contacts (account_id, preferred_name) VALUES ($1, $2) RETURNING id",
        )
        .bind(TEST_ACCOUNT)
        .bind(name_alias)
        .fetch_one(&mut *conn)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO participants (conversation_id, handle_id, contact_id, name_alias)
             VALUES ($1, NULL, $2, $3)",
        )
        .bind(conversation_id)
        .bind(contact_id)
        .bind(name_alias)
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
        assert_eq!(p.handle, Some("+15555550100".to_string()));
        assert_eq!(p.service, Some("imessage".to_string()));
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

    /// A backup that recorded the thread's address and nothing about who was
    /// in it leaves no participants rows, but the vault may still have a name
    /// for the person on the other end.
    #[tokio::test]
    async fn the_chat_handle_takes_the_contact_name_and_id() {
        let (pool, _dir) = crate::db::engine::test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        let (conversation_id, handle_id) = seed(&mut conn, "+15555550500", None).await;
        sqlx::query("DELETE FROM participants WHERE conversation_id = $1")
            .bind(conversation_id)
            .execute(&mut *conn)
            .await
            .unwrap();
        let contact_id = link(&mut conn, handle_id, "Robert Smith").await;

        let loaded = load_for_chat_handle(&mut conn, conversation_id)
            .await
            .unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].name, "Robert Smith");
        assert_eq!(loaded[0].handle, Some("+15555550500".to_string()));
        assert_eq!(loaded[0].service, Some("imessage".to_string()));
        assert_eq!(loaded[0].contact_id, Some(contact_id));
    }

    /// With nothing naming them, the handle stands in, and there is no
    /// contact drawer to open.
    #[tokio::test]
    async fn the_chat_handle_falls_back_to_itself() {
        let (pool, _dir) = crate::db::engine::test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        let (conversation_id, _handle_id) = seed(&mut conn, "+15555550600", None).await;
        sqlx::query("DELETE FROM participants WHERE conversation_id = $1")
            .bind(conversation_id)
            .execute(&mut *conn)
            .await
            .unwrap();

        let loaded = load_for_chat_handle(&mut conn, conversation_id)
            .await
            .unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].name, "+15555550600");
        assert_eq!(loaded[0].contact_id, None);
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

    /// `resolve_name_only_participant` binds a name-only participant straight
    /// to a contact with `handle_id IS NULL`; the `INNER JOIN handles` this
    /// module used to have dropped that row from every conversation it
    /// belongs to. This pins that a `LEFT JOIN` brings it back, carrying the
    /// name from `p.name_alias` (the naming rule's second clause — no handle
    /// means no `h.raw` fallback and no contact to consult via
    /// `contact_handles`), no handle, no service, and the contact bound
    /// directly on the participant row, since that is the only place an
    /// address-less participant's contact link is recorded.
    #[tokio::test]
    async fn an_address_less_participant_appears_in_their_conversation() {
        let (pool, _dir) = crate::db::engine::test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        let (conversation_id, _handle_id) = seed(&mut conn, "+15555550700", None).await;
        sqlx::query("DELETE FROM participants WHERE conversation_id = $1")
            .bind(conversation_id)
            .execute(&mut *conn)
            .await
            .unwrap();
        let contact_id = seed_address_less(&mut conn, conversation_id, "Sarah Vale").await;

        let loaded = load_for_conversations(&mut conn, &[conversation_id])
            .await
            .unwrap();
        let p = &loaded[&conversation_id][0];
        assert_eq!(p.name, "Sarah Vale");
        assert_eq!(p.handle, None);
        assert_eq!(p.service, None);
        assert_eq!(p.contact_id, Some(contact_id));
    }

    /// A conversation can hold both kinds of participant at once — one with
    /// an address, one without — and both come back in participant-id order,
    /// so a conversation's roster is stable regardless of which shape each
    /// member is.
    #[tokio::test]
    async fn addressed_and_address_less_participants_both_return_in_id_order() {
        let (pool, _dir) = crate::db::engine::test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        let (conversation_id, _handle_id) = seed(&mut conn, "+15555550800", Some("Bobby")).await;
        let address_less_contact =
            seed_address_less(&mut conn, conversation_id, "Sarah Vale").await;

        let loaded = load_for_conversations(&mut conn, &[conversation_id])
            .await
            .unwrap();
        let participants = &loaded[&conversation_id];
        assert_eq!(participants.len(), 2);
        assert_eq!(participants[0].name, "Bobby");
        assert_eq!(participants[0].handle, Some("+15555550800".to_string()));
        assert_eq!(participants[1].name, "Sarah Vale");
        assert_eq!(participants[1].handle, None);
        assert_eq!(participants[1].service, None);
        assert_eq!(participants[1].contact_id, Some(address_less_contact));
    }
}
