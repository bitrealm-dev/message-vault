//! The trash marker tables: `trashed_conversations` and `trashed_contacts`.
//!
//! A row in either table is a soft-delete flag; the conversation or contact
//! it points at is untouched until [`purge_account`] (or account deletion,
//! which cascades) removes it for good. Both operations first check that the
//! target row belongs to `account_id` — explicitly, through
//! [`crate::db::ownership`], rather than inferring ownership from whether an
//! insert or delete affected a row. That is what lets [`restore`] tell "not
//! this account's id" (`false`) apart from "this account's id, and it was not
//! trashed" (`true`, a no-op): a `DELETE` that matches zero rows looks
//! identical in both cases.
//!
//! Trashing something already trashed, and restoring something not trashed,
//! both return `true` — these operations are idempotent, per the HTTP routes
//! built on top of them.
//!
//! A conversation and a contact are trashed the same way, so one pair of
//! functions covers both and [`Trashable`] carries which is meant. The marker
//! table and its id column come from that enum and nowhere else, so no part
//! of a request reaches the SQL text.

use sqlx::AnyConnection;

use crate::db::ownership::{owns_contact, owns_conversation};

/// A thing that can be put in the trash, named by id.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trashable {
    /// A row of `conversations`, marked in `trashed_conversations`.
    Conversation(i64),
    /// A row of `contacts`, marked in `trashed_contacts`.
    Contact(i64),
}

impl Trashable {
    /// The row id, whichever kind this is.
    fn id(self) -> i64 {
        match self {
            Self::Conversation(id) | Self::Contact(id) => id,
        }
    }

    /// The marker table and the column in it that carries the row id.
    fn marker(self) -> (&'static str, &'static str) {
        match self {
            Self::Conversation(_) => ("trashed_conversations", "conversation_id"),
            Self::Contact(_) => ("trashed_contacts", "contact_id"),
        }
    }

    /// True when `account_id` owns the row this names.
    async fn is_owned(
        self,
        conn: &mut AnyConnection,
        account_id: &str,
    ) -> Result<bool, sqlx::Error> {
        match self {
            Self::Conversation(id) => owns_conversation(conn, account_id, id).await,
            Self::Contact(id) => owns_contact(conn, account_id, id).await,
        }
    }
}

/// Mark a conversation or contact trashed. Returns `false` when the id is not
/// `account_id`'s; `true` (a no-op) when it is already trashed.
///
/// # Errors
///
/// Returns a database error when a statement fails.
pub async fn move_to_trash(
    conn: &mut AnyConnection,
    account_id: &str,
    target: Trashable,
) -> Result<bool, sqlx::Error> {
    if !target.is_owned(conn, account_id).await? {
        return Ok(false);
    }
    let (table, id_column) = target.marker();
    sqlx::query(&format!(
        "INSERT INTO {table} (account_id, {id_column}) VALUES ($1, $2)
         ON CONFLICT DO NOTHING"
    ))
    .bind(account_id)
    .bind(target.id())
    .execute(&mut *conn)
    .await?;
    Ok(true)
}

/// Remove a conversation's or contact's trash marker, if any. Returns `false`
/// when the id is not `account_id`'s; `true` (a no-op) when it was not
/// trashed.
///
/// # Errors
///
/// Returns a database error when a statement fails.
pub async fn restore(
    conn: &mut AnyConnection,
    account_id: &str,
    target: Trashable,
) -> Result<bool, sqlx::Error> {
    if !target.is_owned(conn, account_id).await? {
        return Ok(false);
    }
    let (table, id_column) = target.marker();
    sqlx::query(&format!(
        "DELETE FROM {table} WHERE account_id = $1 AND {id_column} = $2"
    ))
    .bind(account_id)
    .bind(target.id())
    .execute(&mut *conn)
    .await?;
    Ok(true)
}

/// Remove every trash marker `account_id` holds. Called when an account's
/// conversations (and, by extension, whatever they trashed) are purged.
///
/// # Errors
///
/// Returns a database error when a statement fails.
pub async fn purge_account(conn: &mut AnyConnection, account_id: &str) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM trashed_conversations WHERE account_id = $1")
        .bind(account_id)
        .execute(&mut *conn)
        .await?;
    sqlx::query("DELETE FROM trashed_contacts WHERE account_id = $1")
        .bind(account_id)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    const ACCOUNT_A: &str = "00000000-0000-4000-8000-000000000001";
    const ACCOUNT_B: &str = "00000000-0000-4000-8000-000000000002";

    /// Insert a conversation owned by `account_id`, returning its id. Each
    /// call creates its own chat handle so repeat calls for the same
    /// account don't collide on `conversations`' `(account_id,
    /// chat_handle_id)` uniqueness.
    async fn insert_conversation(conn: &mut AnyConnection, account_id: &str) -> i64 {
        sqlx::query(
            "INSERT INTO handles (account_id, raw, normalized, handle_type, service)
             VALUES ($1, '+15555550100', '+15555550100', 'phone', 'phone')",
        )
        .bind(account_id)
        .execute(&mut *conn)
        .await
        .unwrap();
        let handle_id: i64 = sqlx::query_scalar(
            "SELECT id FROM handles WHERE account_id = $1 ORDER BY id DESC LIMIT 1",
        )
        .bind(account_id)
        .fetch_one(&mut *conn)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO conversations (
                account_id, chat_handle_id, conversation_type, source_file
             ) VALUES ($1, $2, 'individual', 'c.jsonl')",
        )
        .bind(account_id)
        .bind(handle_id)
        .execute(&mut *conn)
        .await
        .unwrap();
        sqlx::query_scalar("SELECT id FROM conversations WHERE chat_handle_id = $1")
            .bind(handle_id)
            .fetch_one(&mut *conn)
            .await
            .unwrap()
    }

    /// Insert a contact owned by `account_id`, returning its id.
    async fn insert_contact(conn: &mut AnyConnection, account_id: &str) -> i64 {
        sqlx::query("INSERT INTO contacts (account_id, preferred_name) VALUES ($1, 'Pat')")
            .bind(account_id)
            .execute(&mut *conn)
            .await
            .unwrap();
        sqlx::query_scalar("SELECT id FROM contacts WHERE account_id = $1")
            .bind(account_id)
            .fetch_one(&mut *conn)
            .await
            .unwrap()
    }

    async fn trashed_conversation_count(conn: &mut AnyConnection, account_id: &str) -> i64 {
        sqlx::query_scalar("SELECT COUNT(*) FROM trashed_conversations WHERE account_id = $1")
            .bind(account_id)
            .fetch_one(&mut *conn)
            .await
            .unwrap()
    }

    async fn trashed_contact_count(conn: &mut AnyConnection, account_id: &str) -> i64 {
        sqlx::query_scalar("SELECT COUNT(*) FROM trashed_contacts WHERE account_id = $1")
            .bind(account_id)
            .fetch_one(&mut *conn)
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn trash_conversation_marks_an_owned_row() {
        let vault = crate::test_support::test_vault().await;
        for account in [ACCOUNT_A, ACCOUNT_B] {
            vault.account_with_id(account, account).await;
        }
        let mut conn = vault.conn().await;
        let id = insert_conversation(&mut conn, ACCOUNT_A).await;

        assert!(
            move_to_trash(&mut conn, ACCOUNT_A, Trashable::Conversation(id))
                .await
                .unwrap()
        );
        assert_eq!(trashed_conversation_count(&mut conn, ACCOUNT_A).await, 1);
    }

    #[tokio::test]
    async fn trash_conversation_twice_stays_one_row() {
        let vault = crate::test_support::test_vault().await;
        for account in [ACCOUNT_A, ACCOUNT_B] {
            vault.account_with_id(account, account).await;
        }
        let mut conn = vault.conn().await;
        let id = insert_conversation(&mut conn, ACCOUNT_A).await;

        assert!(
            move_to_trash(&mut conn, ACCOUNT_A, Trashable::Conversation(id))
                .await
                .unwrap()
        );
        assert!(
            move_to_trash(&mut conn, ACCOUNT_A, Trashable::Conversation(id))
                .await
                .unwrap()
        );
        assert_eq!(trashed_conversation_count(&mut conn, ACCOUNT_A).await, 1);
    }

    #[tokio::test]
    async fn restore_conversation_removes_the_marker() {
        let vault = crate::test_support::test_vault().await;
        for account in [ACCOUNT_A, ACCOUNT_B] {
            vault.account_with_id(account, account).await;
        }
        let mut conn = vault.conn().await;
        let id = insert_conversation(&mut conn, ACCOUNT_A).await;
        move_to_trash(&mut conn, ACCOUNT_A, Trashable::Conversation(id))
            .await
            .unwrap();

        assert!(
            restore(&mut conn, ACCOUNT_A, Trashable::Conversation(id))
                .await
                .unwrap()
        );
        assert_eq!(trashed_conversation_count(&mut conn, ACCOUNT_A).await, 0);
    }

    #[tokio::test]
    async fn restore_conversation_not_trashed_is_a_noop() {
        let vault = crate::test_support::test_vault().await;
        for account in [ACCOUNT_A, ACCOUNT_B] {
            vault.account_with_id(account, account).await;
        }
        let mut conn = vault.conn().await;
        let id = insert_conversation(&mut conn, ACCOUNT_A).await;

        assert!(
            restore(&mut conn, ACCOUNT_A, Trashable::Conversation(id))
                .await
                .unwrap()
        );
        assert_eq!(trashed_conversation_count(&mut conn, ACCOUNT_A).await, 0);
    }

    #[tokio::test]
    async fn conversation_operations_refuse_another_accounts_id() {
        let vault = crate::test_support::test_vault().await;
        for account in [ACCOUNT_A, ACCOUNT_B] {
            vault.account_with_id(account, account).await;
        }
        let mut conn = vault.conn().await;
        let id = insert_conversation(&mut conn, ACCOUNT_A).await;

        assert!(
            !move_to_trash(&mut conn, ACCOUNT_B, Trashable::Conversation(id))
                .await
                .unwrap()
        );
        assert_eq!(trashed_conversation_count(&mut conn, ACCOUNT_A).await, 0);
        assert_eq!(trashed_conversation_count(&mut conn, ACCOUNT_B).await, 0);

        // Trash it as its rightful owner, then confirm B still can't restore it.
        move_to_trash(&mut conn, ACCOUNT_A, Trashable::Conversation(id))
            .await
            .unwrap();
        assert!(
            !restore(&mut conn, ACCOUNT_B, Trashable::Conversation(id))
                .await
                .unwrap()
        );
        assert_eq!(trashed_conversation_count(&mut conn, ACCOUNT_A).await, 1);
    }

    #[tokio::test]
    async fn trash_contact_marks_an_owned_row() {
        let vault = crate::test_support::test_vault().await;
        for account in [ACCOUNT_A, ACCOUNT_B] {
            vault.account_with_id(account, account).await;
        }
        let mut conn = vault.conn().await;
        let id = insert_contact(&mut conn, ACCOUNT_A).await;

        assert!(
            move_to_trash(&mut conn, ACCOUNT_A, Trashable::Contact(id))
                .await
                .unwrap()
        );
        assert_eq!(trashed_contact_count(&mut conn, ACCOUNT_A).await, 1);
    }

    #[tokio::test]
    async fn trash_contact_twice_stays_one_row() {
        let vault = crate::test_support::test_vault().await;
        for account in [ACCOUNT_A, ACCOUNT_B] {
            vault.account_with_id(account, account).await;
        }
        let mut conn = vault.conn().await;
        let id = insert_contact(&mut conn, ACCOUNT_A).await;

        assert!(
            move_to_trash(&mut conn, ACCOUNT_A, Trashable::Contact(id))
                .await
                .unwrap()
        );
        assert!(
            move_to_trash(&mut conn, ACCOUNT_A, Trashable::Contact(id))
                .await
                .unwrap()
        );
        assert_eq!(trashed_contact_count(&mut conn, ACCOUNT_A).await, 1);
    }

    #[tokio::test]
    async fn restore_contact_removes_the_marker() {
        let vault = crate::test_support::test_vault().await;
        for account in [ACCOUNT_A, ACCOUNT_B] {
            vault.account_with_id(account, account).await;
        }
        let mut conn = vault.conn().await;
        let id = insert_contact(&mut conn, ACCOUNT_A).await;
        move_to_trash(&mut conn, ACCOUNT_A, Trashable::Contact(id))
            .await
            .unwrap();

        assert!(
            restore(&mut conn, ACCOUNT_A, Trashable::Contact(id))
                .await
                .unwrap()
        );
        assert_eq!(trashed_contact_count(&mut conn, ACCOUNT_A).await, 0);
    }

    #[tokio::test]
    async fn restore_contact_not_trashed_is_a_noop() {
        let vault = crate::test_support::test_vault().await;
        for account in [ACCOUNT_A, ACCOUNT_B] {
            vault.account_with_id(account, account).await;
        }
        let mut conn = vault.conn().await;
        let id = insert_contact(&mut conn, ACCOUNT_A).await;

        assert!(
            restore(&mut conn, ACCOUNT_A, Trashable::Contact(id))
                .await
                .unwrap()
        );
        assert_eq!(trashed_contact_count(&mut conn, ACCOUNT_A).await, 0);
    }

    #[tokio::test]
    async fn contact_operations_refuse_another_accounts_id() {
        let vault = crate::test_support::test_vault().await;
        for account in [ACCOUNT_A, ACCOUNT_B] {
            vault.account_with_id(account, account).await;
        }
        let mut conn = vault.conn().await;
        let id = insert_contact(&mut conn, ACCOUNT_A).await;

        assert!(
            !move_to_trash(&mut conn, ACCOUNT_B, Trashable::Contact(id))
                .await
                .unwrap()
        );
        assert_eq!(trashed_contact_count(&mut conn, ACCOUNT_A).await, 0);
        assert_eq!(trashed_contact_count(&mut conn, ACCOUNT_B).await, 0);

        move_to_trash(&mut conn, ACCOUNT_A, Trashable::Contact(id))
            .await
            .unwrap();
        assert!(
            !restore(&mut conn, ACCOUNT_B, Trashable::Contact(id))
                .await
                .unwrap()
        );
        assert_eq!(trashed_contact_count(&mut conn, ACCOUNT_A).await, 1);
    }

    #[tokio::test]
    async fn purge_account_clears_only_that_accounts_trash() {
        let vault = crate::test_support::test_vault().await;
        for account in [ACCOUNT_A, ACCOUNT_B] {
            vault.account_with_id(account, account).await;
        }
        let mut conn = vault.conn().await;
        let conv_a = insert_conversation(&mut conn, ACCOUNT_A).await;
        let contact_a = insert_contact(&mut conn, ACCOUNT_A).await;
        let conv_b = insert_conversation(&mut conn, ACCOUNT_B).await;
        let contact_b = insert_contact(&mut conn, ACCOUNT_B).await;
        move_to_trash(&mut conn, ACCOUNT_A, Trashable::Conversation(conv_a))
            .await
            .unwrap();
        move_to_trash(&mut conn, ACCOUNT_A, Trashable::Contact(contact_a))
            .await
            .unwrap();
        move_to_trash(&mut conn, ACCOUNT_B, Trashable::Conversation(conv_b))
            .await
            .unwrap();
        move_to_trash(&mut conn, ACCOUNT_B, Trashable::Contact(contact_b))
            .await
            .unwrap();

        purge_account(&mut conn, ACCOUNT_A).await.unwrap();

        assert_eq!(trashed_conversation_count(&mut conn, ACCOUNT_A).await, 0);
        assert_eq!(trashed_contact_count(&mut conn, ACCOUNT_A).await, 0);
        assert_eq!(trashed_conversation_count(&mut conn, ACCOUNT_B).await, 1);
        assert_eq!(trashed_contact_count(&mut conn, ACCOUNT_B).await, 1);
    }
}
