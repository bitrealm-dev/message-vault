//! Per-account saved searches: named queries a user runs again from the sidebar.
//!
//! A saved search collects nothing. It stores a query string verbatim and is
//! never validated: each list accepts its own subset of the search language,
//! so a query legal for one list can be a 400 on another (see `search`).
//!
//! Rows are addressed by `id` rather than by name, unlike contact groups and
//! message tags: an edit changes the name and the query together, so a
//! name-addressed update would use the changing field as its key.

use serde::Serialize;
use sqlx::any::AnyRow;
use sqlx::{AnyConnection, Row};

use crate::db::dialect::{engine_of, name_eq_ci, order_by_name_ci};
use crate::named_membership::MAX_NAME_LEN;

/// How a saved search was created.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SavedSearchKind {
    /// A person wrote it.
    Manual,
    /// The server created it at the end of an import run.
    Import,
}

impl SavedSearchKind {
    /// Stored spelling of this kind.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Manual => "manual",
            Self::Import => "import",
        }
    }
}

/// One row of `saved_searches`.
#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct SavedSearch {
    /// Saved search id, unique across the vault.
    pub id: i64,
    /// Display name, unique per account.
    pub name: String,
    /// Query string, run against the conversation list.
    pub query: String,
    /// `manual` or `import`.
    pub kind: String,
}

/// Create / update / delete failures for a saved search.
#[derive(Debug)]
pub enum SavedSearchError {
    BadRequest(String),
    NotFound(String),
    Conflict(String),
    Internal(String),
}

impl From<sqlx::Error> for SavedSearchError {
    fn from(e: sqlx::Error) -> Self {
        Self::Internal(e.to_string())
    }
}

impl From<SavedSearchError> for crate::server::ApiError {
    fn from(e: SavedSearchError) -> Self {
        match e {
            SavedSearchError::BadRequest(m) => Self::BadRequest(m),
            SavedSearchError::NotFound(m) => Self::NotFound(m),
            SavedSearchError::Conflict(m) => Self::Conflict(m),
            SavedSearchError::Internal(m) => Self::Internal(m),
        }
    }
}

type Result<T> = std::result::Result<T, SavedSearchError>;

fn row_to_saved_search(row: &AnyRow) -> Result<SavedSearch> {
    Ok(SavedSearch {
        id: row
            .try_get::<i64, _>("id")
            .map_err(|e| SavedSearchError::Internal(e.to_string()))?,
        name: row
            .try_get::<String, _>("name")
            .map_err(|e| SavedSearchError::Internal(e.to_string()))?,
        query: row
            .try_get::<String, _>("query")
            .map_err(|e| SavedSearchError::Internal(e.to_string()))?,
        kind: row
            .try_get::<String, _>("kind")
            .map_err(|e| SavedSearchError::Internal(e.to_string()))?,
    })
}

/// Trim and length-check a name. Empty names and names over
/// [`MAX_NAME_LEN`] characters are rejected, matching the neighbouring
/// collections.
fn normalize_name(name: &str) -> Result<String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(SavedSearchError::BadRequest("name required".into()));
    }
    if trimmed.chars().count() > MAX_NAME_LEN {
        return Err(SavedSearchError::BadRequest(format!(
            "name must be {MAX_NAME_LEN} characters or fewer"
        )));
    }
    Ok(trimmed.to_string())
}

/// Trim a query. Empty queries are rejected; the contents are never inspected.
fn normalize_query(query: &str) -> Result<String> {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return Err(SavedSearchError::BadRequest("query required".into()));
    }
    Ok(trimmed.to_string())
}

/// Id of an account's saved search with this name, case-insensitively.
async fn find_id_by_name(
    conn: &mut AnyConnection,
    account_id: &str,
    name: &str,
) -> Result<Option<i64>> {
    let sql = format!(
        "SELECT id FROM saved_searches WHERE account_id = $1 AND {}",
        name_eq_ci(engine_of(conn), "name", "$2")
    );
    let id = sqlx::query_scalar::<_, i64>(&sql)
        .bind(account_id)
        .bind(name)
        .fetch_optional(&mut *conn)
        .await?;
    Ok(id)
}

/// One account's saved searches, A–Z.
pub async fn list(conn: &mut AnyConnection, account_id: &str) -> Result<Vec<SavedSearch>> {
    let sql = format!(
        "SELECT id, name, query, kind FROM saved_searches WHERE account_id = $1 {}",
        order_by_name_ci(engine_of(conn), "name")
    );
    let rows = sqlx::query(&sql)
        .bind(account_id)
        .fetch_all(&mut *conn)
        .await?;
    rows.iter().map(row_to_saved_search).collect()
}

/// One saved search by id, scoped to the account that owns it.
pub async fn get(
    conn: &mut AnyConnection,
    account_id: &str,
    id: i64,
) -> Result<Option<SavedSearch>> {
    let row = sqlx::query(
        "SELECT id, name, query, kind FROM saved_searches WHERE account_id = $1 AND id = $2",
    )
    .bind(account_id)
    .bind(id)
    .fetch_optional(&mut *conn)
    .await?;
    row.as_ref().map(row_to_saved_search).transpose()
}

/// Create a saved search. The name must be free within the account.
pub async fn create(
    conn: &mut AnyConnection,
    account_id: &str,
    name: &str,
    query: &str,
    kind: SavedSearchKind,
) -> Result<SavedSearch> {
    let name = normalize_name(name)?;
    let query = normalize_query(query)?;
    if find_id_by_name(conn, account_id, &name).await?.is_some() {
        return Err(SavedSearchError::Conflict(
            "saved search already exists".into(),
        ));
    }
    sqlx::query(
        "INSERT INTO saved_searches (account_id, name, query, kind) VALUES ($1, $2, $3, $4)",
    )
    .bind(account_id)
    .bind(&name)
    .bind(&query)
    .bind(kind.as_str())
    .execute(&mut *conn)
    .await?;
    let Some(id) = find_id_by_name(conn, account_id, &name).await? else {
        return Err(SavedSearchError::Internal(
            "saved search vanished after insert".into(),
        ));
    };
    Ok(SavedSearch {
        id,
        name,
        query,
        kind: kind.as_str().to_string(),
    })
}

/// Replace a saved search's name and query. `kind` is not editable: it records
/// how the row was born.
pub async fn update(
    conn: &mut AnyConnection,
    account_id: &str,
    id: i64,
    name: &str,
    query: &str,
) -> Result<SavedSearch> {
    let name = normalize_name(name)?;
    let query = normalize_query(query)?;
    let Some(existing) = get(conn, account_id, id).await? else {
        return Err(SavedSearchError::NotFound("saved search not found".into()));
    };
    // A name already used by a *different* row is a conflict; keeping or
    // recasing this row's own name is not.
    if let Some(other) = find_id_by_name(conn, account_id, &name).await?
        && other != id
    {
        return Err(SavedSearchError::Conflict(
            "saved search already exists".into(),
        ));
    }
    sqlx::query(
        "UPDATE saved_searches SET name = $1, query = $2 WHERE account_id = $3 AND id = $4",
    )
    .bind(&name)
    .bind(&query)
    .bind(account_id)
    .bind(id)
    .execute(&mut *conn)
    .await?;
    Ok(SavedSearch {
        id,
        name,
        query,
        kind: existing.kind,
    })
}

/// Delete a saved search.
///
/// This never touches `vault_imports`: an import-created saved search is a
/// shortcut to a run's messages, and the run's own record is permanent.
pub async fn delete(conn: &mut AnyConnection, account_id: &str, id: i64) -> Result<()> {
    let result = sqlx::query("DELETE FROM saved_searches WHERE account_id = $1 AND id = $2")
        .bind(account_id)
        .bind(id)
        .execute(&mut *conn)
        .await?;
    if result.rows_affected() == 0 {
        return Err(SavedSearchError::NotFound("saved search not found".into()));
    }
    Ok(())
}

/// Name for an import's saved search, adding " 2", " 3", … when the account
/// already used the plain name on the same day.
async fn unique_import_name(
    conn: &mut AnyConnection,
    account_id: &str,
    source: &str,
    date_ymd: &str,
) -> Result<String> {
    let base = format!("Import {source} {date_ymd}");
    if find_id_by_name(conn, account_id, &base).await?.is_none() {
        return Ok(base);
    }
    for n in 2..1000 {
        let candidate = format!("{base} {n}");
        if find_id_by_name(conn, account_id, &candidate)
            .await?
            .is_none()
        {
            return Ok(candidate);
        }
    }
    Err(SavedSearchError::Conflict(
        "too many imports named alike on one day".into(),
    ))
}

/// Create the saved search that points at one import run's messages.
///
/// Called when a run finishes having inserted at least one message. A run
/// that failed, was cancelled, or stored nothing gets no saved search — it is
/// still recorded in `vault_imports` either way.
pub async fn create_for_import(
    conn: &mut AnyConnection,
    account_id: &str,
    import_id: i64,
    source: &str,
    date_ymd: &str,
) -> Result<SavedSearch> {
    let name = unique_import_name(conn, account_id, source, date_ymd).await?;
    create(
        conn,
        account_id,
        &name,
        &format!("import:{import_id}"),
        SavedSearchKind::Import,
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::db::engine;
    use crate::db::schema;

    async fn setup() -> (sqlx::AnyPool, tempfile::TempDir, String) {
        let (pool, dir) = engine::test_pool().await;
        schema::ensure_vault_schema(&mut pool.acquire().await.unwrap())
            .await
            .unwrap();
        let account = "00000000-0000-4000-8000-0000000000e1".to_string();
        let mut conn = pool.acquire().await.unwrap();
        sqlx::query("INSERT INTO accounts (id, username) VALUES ($1, 'alice')")
            .bind(&account)
            .execute(&mut *conn)
            .await
            .unwrap();
        (pool, dir, account)
    }

    #[tokio::test]
    async fn create_trims_and_defaults_to_manual() {
        let (pool, _dir, account) = setup().await;
        let mut conn = pool.acquire().await.unwrap();
        let made = create(
            &mut conn,
            &account,
            "  Work team  ",
            "  service:whatsapp kind:group  ",
            SavedSearchKind::Manual,
        )
        .await
        .unwrap();
        assert_eq!(made.name, "Work team");
        assert_eq!(made.query, "service:whatsapp kind:group");
        assert_eq!(made.kind, "manual");
    }

    #[tokio::test]
    async fn list_is_alphabetical_not_insertion_order() {
        let (pool, _dir, account) = setup().await;
        let mut conn = pool.acquire().await.unwrap();
        for name in ["zeta", "Alpha", "middle"] {
            create(
                &mut conn,
                &account,
                name,
                "kind:group",
                SavedSearchKind::Manual,
            )
            .await
            .unwrap();
        }
        let names: Vec<String> = list(&mut conn, &account)
            .await
            .unwrap()
            .into_iter()
            .map(|s| s.name)
            .collect();
        assert_eq!(names, vec!["Alpha", "middle", "zeta"]);
    }

    #[tokio::test]
    async fn names_collide_case_insensitively_within_an_account() {
        let (pool, _dir, account) = setup().await;
        let mut conn = pool.acquire().await.unwrap();
        create(
            &mut conn,
            &account,
            "Family",
            "kind:group",
            SavedSearchKind::Manual,
        )
        .await
        .unwrap();
        let err = create(
            &mut conn,
            &account,
            "family",
            "kind:direct",
            SavedSearchKind::Manual,
        )
        .await
        .unwrap_err();
        assert!(matches!(err, SavedSearchError::Conflict(_)), "got {err:?}");
    }

    #[tokio::test]
    async fn saved_searches_are_scoped_per_account() {
        let (pool, _dir, account) = setup().await;
        let mut conn = pool.acquire().await.unwrap();
        let other = "00000000-0000-4000-8000-0000000000e2".to_string();
        sqlx::query("INSERT INTO accounts (id, username) VALUES ($1, 'bob')")
            .bind(&other)
            .execute(&mut *conn)
            .await
            .unwrap();

        let mine = create(
            &mut conn,
            &account,
            "Family",
            "kind:group",
            SavedSearchKind::Manual,
        )
        .await
        .unwrap();
        // The same name is free for another account.
        create(
            &mut conn,
            &other,
            "Family",
            "kind:direct",
            SavedSearchKind::Manual,
        )
        .await
        .unwrap();

        assert_eq!(list(&mut conn, &other).await.unwrap().len(), 1);
        // One account cannot read or delete another's row by id.
        assert!(get(&mut conn, &other, mine.id).await.unwrap().is_none());
        let err = delete(&mut conn, &other, mine.id).await.unwrap_err();
        assert!(matches!(err, SavedSearchError::NotFound(_)), "got {err:?}");
    }

    #[tokio::test]
    async fn update_replaces_both_fields_and_keeps_id_and_kind() {
        let (pool, _dir, account) = setup().await;
        let mut conn = pool.acquire().await.unwrap();
        let made = create_for_import(&mut conn, &account, 7, "imessage", "2026-08-30")
            .await
            .unwrap();
        let edited = update(&mut conn, &account, made.id, "Renamed", "kind:direct")
            .await
            .unwrap();
        assert_eq!(edited.id, made.id);
        assert_eq!(edited.name, "Renamed");
        assert_eq!(edited.query, "kind:direct");
        assert_eq!(edited.kind, "import", "kind records how a row was born");
    }

    #[tokio::test]
    async fn update_allows_a_row_to_keep_or_recase_its_own_name() {
        let (pool, _dir, account) = setup().await;
        let mut conn = pool.acquire().await.unwrap();
        let made = create(
            &mut conn,
            &account,
            "Family",
            "kind:group",
            SavedSearchKind::Manual,
        )
        .await
        .unwrap();
        let same = update(&mut conn, &account, made.id, "Family", "kind:direct")
            .await
            .unwrap();
        assert_eq!(same.query, "kind:direct");
        let recased = update(&mut conn, &account, made.id, "FAMILY", "kind:direct")
            .await
            .unwrap();
        assert_eq!(recased.name, "FAMILY");
    }

    #[tokio::test]
    async fn update_rejects_a_name_another_row_already_uses() {
        let (pool, _dir, account) = setup().await;
        let mut conn = pool.acquire().await.unwrap();
        create(
            &mut conn,
            &account,
            "Family",
            "kind:group",
            SavedSearchKind::Manual,
        )
        .await
        .unwrap();
        let second = create(
            &mut conn,
            &account,
            "Work",
            "kind:direct",
            SavedSearchKind::Manual,
        )
        .await
        .unwrap();
        let err = update(&mut conn, &account, second.id, "family", "kind:direct")
            .await
            .unwrap_err();
        assert!(matches!(err, SavedSearchError::Conflict(_)), "got {err:?}");
    }

    #[tokio::test]
    async fn empty_name_or_query_is_rejected() {
        let (pool, _dir, account) = setup().await;
        let mut conn = pool.acquire().await.unwrap();
        let err = create(
            &mut conn,
            &account,
            "   ",
            "kind:group",
            SavedSearchKind::Manual,
        )
        .await
        .unwrap_err();
        assert!(
            matches!(err, SavedSearchError::BadRequest(_)),
            "got {err:?}"
        );
        let err = create(&mut conn, &account, "Name", "   ", SavedSearchKind::Manual)
            .await
            .unwrap_err();
        assert!(
            matches!(err, SavedSearchError::BadRequest(_)),
            "got {err:?}"
        );
    }

    #[tokio::test]
    async fn names_over_max_len_are_rejected() {
        let (pool, _dir, account) = setup().await;
        let mut conn = pool.acquire().await.unwrap();
        let long = "a".repeat(MAX_NAME_LEN + 1);
        let err = create(
            &mut conn,
            &account,
            &long,
            "kind:group",
            SavedSearchKind::Manual,
        )
        .await
        .unwrap_err();
        assert!(
            matches!(err, SavedSearchError::BadRequest(_)),
            "got {err:?}"
        );
    }

    #[tokio::test]
    async fn any_query_string_is_stored_verbatim() {
        let (pool, _dir, account) = setup().await;
        let mut conn = pool.acquire().await.unwrap();
        // Nonsense in both grammars. The vault stores it anyway: the two
        // parsers disagree about what is legal, so nothing validates here.
        let made = create(
            &mut conn,
            &account,
            "Nonsense",
            "from:bob service:discord",
            SavedSearchKind::Manual,
        )
        .await
        .unwrap();
        assert_eq!(made.query, "from:bob service:discord");
    }

    #[tokio::test]
    async fn import_saved_search_is_named_and_marked() {
        let (pool, _dir, account) = setup().await;
        let mut conn = pool.acquire().await.unwrap();
        let made = create_for_import(&mut conn, &account, 42, "imessage", "2026-08-30")
            .await
            .unwrap();
        assert_eq!(made.name, "Import imessage 2026-08-30");
        assert_eq!(made.query, "import:42");
        assert_eq!(made.kind, "import");
    }

    #[tokio::test]
    async fn repeat_imports_on_one_day_get_numbered_names() {
        let (pool, _dir, account) = setup().await;
        let mut conn = pool.acquire().await.unwrap();
        let first = create_for_import(&mut conn, &account, 1, "imessage", "2026-08-30")
            .await
            .unwrap();
        let second = create_for_import(&mut conn, &account, 2, "imessage", "2026-08-30")
            .await
            .unwrap();
        let third = create_for_import(&mut conn, &account, 3, "imessage", "2026-08-30")
            .await
            .unwrap();
        assert_eq!(first.name, "Import imessage 2026-08-30");
        assert_eq!(second.name, "Import imessage 2026-08-30 2");
        assert_eq!(third.name, "Import imessage 2026-08-30 3");
        assert_eq!(third.query, "import:3");
    }

    #[tokio::test]
    async fn deleting_a_saved_search_leaves_the_import_record() {
        let (pool, _dir, account) = setup().await;
        let mut conn = pool.acquire().await.unwrap();
        sqlx::query(
            "INSERT INTO vault_imports
             (id, account_id, source, mode, status, started_at, message_count)
             VALUES (99, $1, 'imessage', 'append', 'completed', '2026-08-30T00:00:00Z', 12)",
        )
        .bind(&account)
        .execute(&mut *conn)
        .await
        .unwrap();

        let made = create_for_import(&mut conn, &account, 99, "imessage", "2026-08-30")
            .await
            .unwrap();
        delete(&mut conn, &account, made.id).await.unwrap();

        assert!(list(&mut conn, &account).await.unwrap().is_empty());
        let still_there: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM vault_imports WHERE id = 99")
                .fetch_one(&mut *conn)
                .await
                .unwrap();
        assert_eq!(
            still_there, 1,
            "deleting the shortcut must not touch the import record"
        );
    }
}
