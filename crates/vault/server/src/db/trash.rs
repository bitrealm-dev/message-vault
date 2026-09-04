//! The trash marker tables: `trashed_conversations` and `trashed_contacts`.
//!
//! A row in either table is a soft-delete flag; the conversation or contact
//! it points at is untouched until [`purge_account`] (or account deletion,
//! which cascades) removes it for good. Every operation here first checks
//! that the target row belongs to `account_id` — explicitly, as its own
//! query, rather than inferring ownership from whether an insert or delete
//! affected a row. That is what lets `restore_conversation` (and
//! `restore_contact`) tell "not this account's id" (`false`) apart from
//! "this account's id, and it was not trashed" (`true`, a no-op): a `DELETE`
//! that matches zero rows looks identical in both cases.
//!
//! Trashing something already trashed, and restoring something not trashed,
//! both return `true` — these operations are idempotent, per the HTTP
//! routes built on top of them (Task 2).

use sqlx::AnyConnection;

/// True when `account_id` owns a `conversations` row with this id.
async fn owns_conversation(
    conn: &mut AnyConnection,
    account_id: &str,
    id: i64,
) -> Result<bool, sqlx::Error> {
    let found: Option<i64> =
        sqlx::query_scalar("SELECT 1 FROM conversations WHERE id = $1 AND account_id = $2")
            .bind(id)
            .bind(account_id)
            .fetch_optional(&mut *conn)
            .await?;
    Ok(found.is_some())
}

/// True when `account_id` owns a `contacts` row with this id.
async fn owns_contact(
    conn: &mut AnyConnection,
    account_id: &str,
    id: i64,
) -> Result<bool, sqlx::Error> {
    let found: Option<i64> =
        sqlx::query_scalar("SELECT 1 FROM contacts WHERE id = $1 AND account_id = $2")
            .bind(id)
            .bind(account_id)
            .fetch_optional(&mut *conn)
            .await?;
    Ok(found.is_some())
}

/// Mark a conversation trashed. Returns `false` when `id` is not
/// `account_id`'s conversation; `true` (a no-op) when it is already trashed.
pub async fn trash_conversation(
    conn: &mut AnyConnection,
    account_id: &str,
    id: i64,
) -> Result<bool, sqlx::Error> {
    if !owns_conversation(conn, account_id, id).await? {
        return Ok(false);
    }
    sqlx::query(
        "INSERT INTO trashed_conversations (account_id, conversation_id) VALUES ($1, $2)
         ON CONFLICT DO NOTHING",
    )
    .bind(account_id)
    .bind(id)
    .execute(&mut *conn)
    .await?;
    Ok(true)
}

/// Remove a conversation's trash marker, if any. Returns `false` when `id`
/// is not `account_id`'s conversation; `true` (a no-op) when it was not
/// trashed.
pub async fn restore_conversation(
    conn: &mut AnyConnection,
    account_id: &str,
    id: i64,
) -> Result<bool, sqlx::Error> {
    if !owns_conversation(conn, account_id, id).await? {
        return Ok(false);
    }
    sqlx::query("DELETE FROM trashed_conversations WHERE account_id = $1 AND conversation_id = $2")
        .bind(account_id)
        .bind(id)
        .execute(&mut *conn)
        .await?;
    Ok(true)
}

/// Mark a contact trashed. Returns `false` when `id` is not `account_id`'s
/// contact; `true` (a no-op) when it is already trashed.
pub async fn trash_contact(
    conn: &mut AnyConnection,
    account_id: &str,
    id: i64,
) -> Result<bool, sqlx::Error> {
    if !owns_contact(conn, account_id, id).await? {
        return Ok(false);
    }
    sqlx::query(
        "INSERT INTO trashed_contacts (account_id, contact_id) VALUES ($1, $2)
         ON CONFLICT DO NOTHING",
    )
    .bind(account_id)
    .bind(id)
    .execute(&mut *conn)
    .await?;
    Ok(true)
}

/// Remove a contact's trash marker, if any. Returns `false` when `id` is not
/// `account_id`'s contact; `true` (a no-op) when it was not trashed.
pub async fn restore_contact(
    conn: &mut AnyConnection,
    account_id: &str,
    id: i64,
) -> Result<bool, sqlx::Error> {
    if !owns_contact(conn, account_id, id).await? {
        return Ok(false);
    }
    sqlx::query("DELETE FROM trashed_contacts WHERE account_id = $1 AND contact_id = $2")
        .bind(account_id)
        .bind(id)
        .execute(&mut *conn)
        .await?;
    Ok(true)
}

/// Remove every trash marker `account_id` holds. Called when an account's
/// conversations (and, by extension, whatever they trashed) are purged.
///
/// `trashed_handles` is included here for now, moved verbatim out of
/// `account_profile::delete_all_messages_for_account`. It is not one of the
/// two tables this module otherwise owns — there is no `trash_handle` /
/// `restore_handle` pair, and the table itself is dropped in a later task —
/// but until that drop lands, purging an account still means clearing
/// whatever rows it holds.
pub async fn purge_account(conn: &mut AnyConnection, account_id: &str) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM trashed_conversations WHERE account_id = $1")
        .bind(account_id)
        .execute(&mut *conn)
        .await?;
    sqlx::query("DELETE FROM trashed_contacts WHERE account_id = $1")
        .bind(account_id)
        .execute(&mut *conn)
        .await?;
    sqlx::query("DELETE FROM trashed_handles WHERE account_id = $1")
        .bind(account_id)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::schema;

    const ACCOUNT_A: &str = "00000000-0000-4000-8000-000000000001";
    const ACCOUNT_B: &str = "00000000-0000-4000-8000-000000000002";

    async fn setup() -> (sqlx::AnyPool, tempfile::TempDir) {
        let (pool, dir) = crate::db::engine::test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        schema::ensure_vault_schema(&mut conn).await.unwrap();
        for account in [ACCOUNT_A, ACCOUNT_B] {
            sqlx::query("INSERT INTO accounts (id, username) VALUES ($1, $1)")
                .bind(account)
                .execute(&mut *conn)
                .await
                .unwrap();
        }
        (pool, dir)
    }

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
        let (pool, _dir) = setup().await;
        let mut conn = pool.acquire().await.unwrap();
        let id = insert_conversation(&mut conn, ACCOUNT_A).await;

        assert!(trash_conversation(&mut conn, ACCOUNT_A, id).await.unwrap());
        assert_eq!(trashed_conversation_count(&mut conn, ACCOUNT_A).await, 1);
    }

    #[tokio::test]
    async fn trash_conversation_twice_stays_one_row() {
        let (pool, _dir) = setup().await;
        let mut conn = pool.acquire().await.unwrap();
        let id = insert_conversation(&mut conn, ACCOUNT_A).await;

        assert!(trash_conversation(&mut conn, ACCOUNT_A, id).await.unwrap());
        assert!(trash_conversation(&mut conn, ACCOUNT_A, id).await.unwrap());
        assert_eq!(trashed_conversation_count(&mut conn, ACCOUNT_A).await, 1);
    }

    #[tokio::test]
    async fn restore_conversation_removes_the_marker() {
        let (pool, _dir) = setup().await;
        let mut conn = pool.acquire().await.unwrap();
        let id = insert_conversation(&mut conn, ACCOUNT_A).await;
        trash_conversation(&mut conn, ACCOUNT_A, id).await.unwrap();

        assert!(
            restore_conversation(&mut conn, ACCOUNT_A, id)
                .await
                .unwrap()
        );
        assert_eq!(trashed_conversation_count(&mut conn, ACCOUNT_A).await, 0);
    }

    #[tokio::test]
    async fn restore_conversation_not_trashed_is_a_noop() {
        let (pool, _dir) = setup().await;
        let mut conn = pool.acquire().await.unwrap();
        let id = insert_conversation(&mut conn, ACCOUNT_A).await;

        assert!(
            restore_conversation(&mut conn, ACCOUNT_A, id)
                .await
                .unwrap()
        );
        assert_eq!(trashed_conversation_count(&mut conn, ACCOUNT_A).await, 0);
    }

    #[tokio::test]
    async fn conversation_operations_refuse_another_accounts_id() {
        let (pool, _dir) = setup().await;
        let mut conn = pool.acquire().await.unwrap();
        let id = insert_conversation(&mut conn, ACCOUNT_A).await;

        assert!(!trash_conversation(&mut conn, ACCOUNT_B, id).await.unwrap());
        assert_eq!(trashed_conversation_count(&mut conn, ACCOUNT_A).await, 0);
        assert_eq!(trashed_conversation_count(&mut conn, ACCOUNT_B).await, 0);

        // Trash it as its rightful owner, then confirm B still can't restore it.
        trash_conversation(&mut conn, ACCOUNT_A, id).await.unwrap();
        assert!(
            !restore_conversation(&mut conn, ACCOUNT_B, id)
                .await
                .unwrap()
        );
        assert_eq!(trashed_conversation_count(&mut conn, ACCOUNT_A).await, 1);
    }

    #[tokio::test]
    async fn trash_contact_marks_an_owned_row() {
        let (pool, _dir) = setup().await;
        let mut conn = pool.acquire().await.unwrap();
        let id = insert_contact(&mut conn, ACCOUNT_A).await;

        assert!(trash_contact(&mut conn, ACCOUNT_A, id).await.unwrap());
        assert_eq!(trashed_contact_count(&mut conn, ACCOUNT_A).await, 1);
    }

    #[tokio::test]
    async fn trash_contact_twice_stays_one_row() {
        let (pool, _dir) = setup().await;
        let mut conn = pool.acquire().await.unwrap();
        let id = insert_contact(&mut conn, ACCOUNT_A).await;

        assert!(trash_contact(&mut conn, ACCOUNT_A, id).await.unwrap());
        assert!(trash_contact(&mut conn, ACCOUNT_A, id).await.unwrap());
        assert_eq!(trashed_contact_count(&mut conn, ACCOUNT_A).await, 1);
    }

    #[tokio::test]
    async fn restore_contact_removes_the_marker() {
        let (pool, _dir) = setup().await;
        let mut conn = pool.acquire().await.unwrap();
        let id = insert_contact(&mut conn, ACCOUNT_A).await;
        trash_contact(&mut conn, ACCOUNT_A, id).await.unwrap();

        assert!(restore_contact(&mut conn, ACCOUNT_A, id).await.unwrap());
        assert_eq!(trashed_contact_count(&mut conn, ACCOUNT_A).await, 0);
    }

    #[tokio::test]
    async fn restore_contact_not_trashed_is_a_noop() {
        let (pool, _dir) = setup().await;
        let mut conn = pool.acquire().await.unwrap();
        let id = insert_contact(&mut conn, ACCOUNT_A).await;

        assert!(restore_contact(&mut conn, ACCOUNT_A, id).await.unwrap());
        assert_eq!(trashed_contact_count(&mut conn, ACCOUNT_A).await, 0);
    }

    #[tokio::test]
    async fn contact_operations_refuse_another_accounts_id() {
        let (pool, _dir) = setup().await;
        let mut conn = pool.acquire().await.unwrap();
        let id = insert_contact(&mut conn, ACCOUNT_A).await;

        assert!(!trash_contact(&mut conn, ACCOUNT_B, id).await.unwrap());
        assert_eq!(trashed_contact_count(&mut conn, ACCOUNT_A).await, 0);
        assert_eq!(trashed_contact_count(&mut conn, ACCOUNT_B).await, 0);

        trash_contact(&mut conn, ACCOUNT_A, id).await.unwrap();
        assert!(!restore_contact(&mut conn, ACCOUNT_B, id).await.unwrap());
        assert_eq!(trashed_contact_count(&mut conn, ACCOUNT_A).await, 1);
    }

    #[tokio::test]
    async fn purge_account_clears_only_that_accounts_trash() {
        let (pool, _dir) = setup().await;
        let mut conn = pool.acquire().await.unwrap();
        let conv_a = insert_conversation(&mut conn, ACCOUNT_A).await;
        let contact_a = insert_contact(&mut conn, ACCOUNT_A).await;
        let conv_b = insert_conversation(&mut conn, ACCOUNT_B).await;
        let contact_b = insert_contact(&mut conn, ACCOUNT_B).await;
        trash_conversation(&mut conn, ACCOUNT_A, conv_a)
            .await
            .unwrap();
        trash_contact(&mut conn, ACCOUNT_A, contact_a)
            .await
            .unwrap();
        trash_conversation(&mut conn, ACCOUNT_B, conv_b)
            .await
            .unwrap();
        trash_contact(&mut conn, ACCOUNT_B, contact_b)
            .await
            .unwrap();

        purge_account(&mut conn, ACCOUNT_A).await.unwrap();

        assert_eq!(trashed_conversation_count(&mut conn, ACCOUNT_A).await, 0);
        assert_eq!(trashed_contact_count(&mut conn, ACCOUNT_A).await, 0);
        assert_eq!(trashed_conversation_count(&mut conn, ACCOUNT_B).await, 1);
        assert_eq!(trashed_contact_count(&mut conn, ACCOUNT_B).await, 1);
    }
}
