//! Ready-guest pool: assign a session, refill unused copies, and delete expired ones.

use std::collections::VecDeque;
use std::fs;
use std::path::Path;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use sqlx::AnyConnection;
use sqlx::{Connection, Row};

use crate::config::{Config, GuestDemoSettings};
use crate::db::account_profile::{self, DEMO_ACCOUNT_ID};
use crate::db::engine::DbEngine;
use crate::db::{dialect, session_tokens};
use crate::guest_clone::clone_template_to_guest;

/// How far back refill demand looks when counting recent Try it assignments.
const ASSIGNMENT_WINDOW: Duration = Duration::from_secs(15 * 60);

/// In-memory count of hosted Try it assignments in the last 15 minutes.
///
/// The worker passes `count_last_15m()` into `refill_pool` so a burst of
/// visitors grows unused ready copies up to the ceiling.
#[derive(Debug, Default)]
pub struct GuestPoolState {
    assignments: VecDeque<(Instant, u32)>,
}

impl GuestPoolState {
    /// Create an empty assignment counter.
    pub fn new() -> Self {
        Self::default()
    }

    /// Record one successful hosted Try it assignment.
    pub fn record_assignment(&mut self) {
        self.record_assignment_at(Instant::now());
    }

    fn record_assignment_at(&mut self, when: Instant) {
        self.assignments.push_back((when, 1));
    }

    /// Drop timestamps older than 15 minutes and return the remaining count.
    pub fn count_last_15m(&mut self) -> u32 {
        let cutoff = Instant::now().checked_sub(ASSIGNMENT_WINDOW);
        while let Some((when, _)) = self.assignments.front() {
            if cutoff.is_some_and(|c| *when < c) {
                self.assignments.pop_front();
            } else {
                break;
            }
        }
        self.assignments.iter().map(|(_, n)| *n).sum()
    }
}

/// Count unused ready guest accounts sitting in the pool.
///
/// # Errors
///
/// Returns an error when the count query fails.
pub async fn count_ready(conn: &mut AnyConnection) -> Result<u32> {
    let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM accounts WHERE guest_status = 'ready'")
        .fetch_one(&mut *conn)
        .await?;
    u32::try_from(n).context("ready guest count does not fit in u32")
}

/// Take one ready guest, mark it assigned, and issue a session token.
///
/// Uses `BEGIN IMMEDIATE` on SQLite (write lock up front) and
/// `SELECT … FOR UPDATE` on Postgres (plain BEGIN has no IMMEDIATE
/// equivalent) so two concurrent assigns cannot pick the same row. Returns
/// `Ok(None)` when the pool is empty.
///
/// # Errors
///
/// Returns an error when the transaction, status update, or token insert fails.
pub async fn assign_ready_guest(
    conn: &mut AnyConnection,
    session_secs: u64,
) -> Result<Option<(String, String, String)>> {
    let engine = dialect::engine_of(conn);
    let mut tx = conn
        .begin_with(dialect::begin_immediate_sql(engine))
        .await?;
    // Row lock on Postgres: a second assign blocks here, re-reads the row as
    // 'assigned' under READ COMMITTED, and takes the next ready guest
    // instead of racing this one. SQLite's IMMEDIATE begin already excludes
    // concurrent writers.
    let lock_row = match engine {
        DbEngine::Postgres => " FOR UPDATE",
        DbEngine::Sqlite => "",
    };
    let picked: Option<(String, String)> = sqlx::query(&format!(
        "SELECT id, username FROM accounts
         WHERE guest_status = 'ready'
         ORDER BY id
         LIMIT 1{lock_row}"
    ))
    .fetch_optional(&mut *tx)
    .await?
    .map(|row| Ok::<_, sqlx::Error>((row.try_get::<String, _>(0)?, row.try_get::<String, _>(1)?)))
    .transpose()?;
    let Some((account_id, username)) = picked else {
        tx.commit().await?;
        return Ok(None);
    };
    account_profile::set_guest_status(&mut tx, &account_id, "assigned").await?;
    let token =
        session_tokens::insert_account_session_token_with_ttl(&mut tx, &account_id, session_secs)
            .await?;
    tx.commit().await?;
    Ok(Some((account_id, username, token)))
}

fn refill_target(settings: GuestDemoSettings, assignments_last_15m: u32) -> u32 {
    settings
        .pool_min
        .max(assignments_last_15m)
        .min(settings.pool_max)
}

/// Delete unused ready guests when the pool is above `pool_max`.
///
/// Oldest ready rows (by `id`) are removed first. Assigned guests are left
/// alone. Returns how many accounts were deleted.
///
/// # Errors
///
/// Returns an error when the count, delete, or directory remove fails.
pub async fn shrink_over_ceiling(
    conn: &mut AnyConnection,
    cfg: &Config,
    settings: GuestDemoSettings,
) -> Result<u32> {
    let ready = count_ready(conn).await?;
    if ready <= settings.pool_max {
        return Ok(0);
    }
    let excess = ready - settings.pool_max;
    drop_oldest_ready(conn, &cfg.paths.data_dir, excess).await?;
    Ok(excess)
}

/// Clone at most one ready guest when the unused pool is below the refill target.
///
/// Target is `max(pool_min, assignments_last_15m)` capped at `pool_max`.
/// Returns `1` when a guest was cloned, or `0` when the pool is already at
/// the target.
///
/// # Errors
///
/// Returns an error when the count or clone fails.
pub async fn refill_one(
    conn: &mut AnyConnection,
    cfg: &Config,
    settings: GuestDemoSettings,
    assignments_last_15m: u32,
) -> Result<u32> {
    let target = refill_target(settings, assignments_last_15m);
    if count_ready(conn).await? >= target {
        return Ok(0);
    }
    clone_template_to_guest(conn, cfg, DEMO_ACCOUNT_ID).await?;
    Ok(1)
}

/// Grow unused ready guests up to the refill target, and shrink if over the ceiling.
///
/// Target is `max(pool_min, assignments_last_15m)` capped at `pool_max`.
/// Excess ready guests (oldest by `id`) are deleted. New copies are cloned from
/// the template demo account.
///
/// # Errors
///
/// Returns an error when a query, delete, or clone fails.
pub async fn refill_pool(
    conn: &mut AnyConnection,
    cfg: &Config,
    settings: GuestDemoSettings,
    assignments_last_15m: u32,
) -> Result<u32> {
    shrink_over_ceiling(conn, cfg, settings).await?;
    let mut created = 0u32;
    while refill_one(conn, cfg, settings, assignments_last_15m).await? == 1 {
        created += 1;
    }
    Ok(created)
}

/// Delete assigned guests whose session is missing or already expired.
///
/// Ready guests are left alone. Each deleted account also loses
/// `data_dir/<account_id>/`.
///
/// # Errors
///
/// Returns an error when the lookup, account delete, or directory remove fails.
pub async fn sweep_expired_guests(conn: &mut AnyConnection, data_dir: &Path) -> Result<u32> {
    let now = now_unix_secs();
    let ids = list_expired_assigned(conn, now).await?;
    let n = u32::try_from(ids.len()).context("expired guest count does not fit in u32")?;
    for id in ids {
        delete_guest_account_and_files(conn, data_dir, &id).await?;
    }
    Ok(n)
}

/// Delete unused ready guests (for `reset-demo` before a refill).
///
/// Assigned guests are left alone.
///
/// # Errors
///
/// Returns an error when the lookup, account delete, or directory remove fails.
pub async fn drop_ready_guests(conn: &mut AnyConnection, data_dir: &Path) -> Result<u32> {
    let ids = list_ready_ids(conn).await?;
    let n = u32::try_from(ids.len()).context("ready guest count does not fit in u32")?;
    for id in ids {
        delete_guest_account_and_files(conn, data_dir, &id).await?;
    }
    Ok(n)
}

async fn list_ready_ids(conn: &mut AnyConnection) -> Result<Vec<String>> {
    let rows = sqlx::query("SELECT id FROM accounts WHERE guest_status = 'ready' ORDER BY id")
        .fetch_all(&mut *conn)
        .await?;
    let mut ids = Vec::with_capacity(rows.len());
    for row in &rows {
        ids.push(row.try_get::<String, _>(0)?);
    }
    Ok(ids)
}

async fn drop_oldest_ready(conn: &mut AnyConnection, data_dir: &Path, n: u32) -> Result<()> {
    let rows =
        sqlx::query("SELECT id FROM accounts WHERE guest_status = 'ready' ORDER BY id LIMIT $1")
            .bind(i64::from(n))
            .fetch_all(&mut *conn)
            .await?;
    let mut ids = Vec::with_capacity(rows.len());
    for row in &rows {
        ids.push(row.try_get::<String, _>(0)?);
    }
    for id in ids {
        delete_guest_account_and_files(conn, data_dir, &id).await?;
    }
    Ok(())
}

async fn list_expired_assigned(conn: &mut AnyConnection, now_secs: u64) -> Result<Vec<String>> {
    let now = i64::try_from(now_secs).unwrap_or(i64::MAX);
    let rows = sqlx::query(
        r#"
        SELECT a.id
        FROM accounts a
        LEFT JOIN account_session_tokens t ON t.account_id = a.id
        WHERE a.guest_status = 'assigned'
          AND (
            t.account_id IS NULL
            OR CAST(t.expires_at AS BIGINT) <= $1
          )
        "#,
    )
    .bind(now)
    .fetch_all(&mut *conn)
    .await?;
    let mut ids = Vec::with_capacity(rows.len());
    for row in &rows {
        ids.push(row.try_get::<String, _>(0)?);
    }
    Ok(ids)
}

async fn delete_guest_account_and_files(
    conn: &mut AnyConnection,
    data_dir: &Path,
    account_id: &str,
) -> Result<()> {
    account_profile::delete_account(conn, account_id).await?;
    let dir = data_dir.join(account_id);
    if dir.exists() {
        fs::remove_dir_all(&dir)
            .with_context(|| format!("remove guest data dir {}", dir.display()))?;
    }
    Ok(())
}

fn now_unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Config, GuestDemoSettings, PathsConfig};
    use crate::db::account_profile::{self, DEMO_ACCOUNT_ID};
    use crate::db::engine;
    use crate::db::schema;
    use crate::db::session_tokens;
    use crate::guest_clone::clone_template_to_guest;
    use std::sync::{Arc, Mutex};

    struct TestEnv {
        cfg: Config,
        _tmp: tempfile::TempDir,
    }

    impl std::ops::Deref for TestEnv {
        type Target = Config;
        fn deref(&self) -> &Self::Target {
            &self.cfg
        }
    }

    fn test_config() -> TestEnv {
        let tmp = tempfile::tempdir().expect("temp data_dir");
        let data_dir = tmp.path().to_path_buf();
        TestEnv {
            cfg: Config {
                paths: PathsConfig {
                    db: data_dir.join("vault.db"),
                    data_dir,
                    assets_dir: "assets".into(),
                    assets_converted_dir: "assets_converted".into(),
                },
                server: None,
                database: crate::config::DatabaseConfig::default(),
            },
            _tmp: tmp,
        }
    }

    async fn tiny_template(pool: &sqlx::AnyPool) {
        let mut conn = pool.acquire().await.unwrap();
        schema::ensure_vault_schema(&mut conn).await.unwrap();
        sqlx::query(
            "INSERT INTO accounts (id, username, read_only, preferred_name)
             VALUES ($1, 'demo', 1, 'Alex Demo')",
        )
        .bind(DEMO_ACCOUNT_ID)
        .execute(&mut *conn)
        .await
        .unwrap();
        let hid: i64 = sqlx::query_scalar(
            "INSERT INTO handles (account_id, raw, normalized, handle_type, service)
             VALUES ($1, '+15555550100', '+15555550100', 'phone', 'phone')
             RETURNING id",
        )
        .bind(DEMO_ACCOUNT_ID)
        .fetch_one(&mut *conn)
        .await
        .unwrap();
        sqlx::query("INSERT INTO account_handles (account_id, handle_id) VALUES ($1, $2)")
            .bind(DEMO_ACCOUNT_ID)
            .bind(hid)
            .execute(&mut *conn)
            .await
            .unwrap();
        let cid: i64 = sqlx::query_scalar(
            "INSERT INTO conversations (account_id, chat_handle_id, conversation_type, source_file)
             VALUES ($1, $2, 'individual', 'a.jsonl')
             RETURNING id",
        )
        .bind(DEMO_ACCOUNT_ID)
        .bind(hid)
        .fetch_one(&mut *conn)
        .await
        .unwrap();
        sqlx::query(
            r#"INSERT INTO messages (
                conversation_id, account_id, source, guid, timestamp, is_from_me, sort_order, body
            ) VALUES ($1, $2, 'imessage', 'g1', '2020-01-01T00:00:00Z', 1, 0, 'hello')"#,
        )
        .bind(cid)
        .bind(DEMO_ACCOUNT_ID)
        .execute(&mut *conn)
        .await
        .unwrap();
    }

    async fn insert_ready(conn: &mut AnyConnection, id: &str, username: &str) {
        account_profile::insert_guest_account(conn, id, username, Some("Guest"))
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn assign_marks_assigned_and_issues_token() {
        let (pool, _dir) = engine::test_pool().await;
        tiny_template(&pool).await;
        let env = test_config();
        let mut conn = pool.acquire().await.unwrap();
        clone_template_to_guest(&mut conn, &env, DEMO_ACCOUNT_ID)
            .await
            .unwrap();
        clone_template_to_guest(&mut conn, &env, DEMO_ACCOUNT_ID)
            .await
            .unwrap();
        assert_eq!(count_ready(&mut conn).await.unwrap(), 2);

        let (account_id, username, token) = assign_ready_guest(&mut conn, 120)
            .await
            .unwrap()
            .expect("ready guest");
        assert!(username.starts_with("guest-"), "{username}");
        assert!(token.starts_with("mv-user-"), "{token}");
        assert_eq!(
            account_profile::guest_status(&mut conn, &account_id)
                .await
                .unwrap()
                .as_deref(),
            Some("assigned")
        );
        assert_eq!(
            session_tokens::lookup_account_for_token(&mut conn, &token)
                .await
                .unwrap(),
            Some(account_id.clone())
        );
        assert_eq!(count_ready(&mut conn).await.unwrap(), 1);
        assert!(assign_ready_guest(&mut conn, 120).await.unwrap().is_some());
        assert_eq!(count_ready(&mut conn).await.unwrap(), 0);
        assert!(assign_ready_guest(&mut conn, 120).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn two_assigns_never_share_a_guest() {
        let (pool, _dir) = engine::test_pool().await;
        {
            let mut conn = pool.acquire().await.unwrap();
            schema::ensure_vault_schema(&mut conn).await.unwrap();
            insert_ready(
                &mut conn,
                "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaa1",
                "guest-seq1",
            )
            .await;
            insert_ready(
                &mut conn,
                "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaa2",
                "guest-seq2",
            )
            .await;
            let a = assign_ready_guest(&mut conn, 60).await.unwrap().unwrap();
            let b = assign_ready_guest(&mut conn, 60).await.unwrap().unwrap();
            assert_ne!(a.0, b.0, "sequential assigns shared a guest");
            assert_ne!(a.2, b.2, "sequential assigns shared a token");
        }

        {
            let mut conn = pool.acquire().await.unwrap();
            schema::ensure_vault_schema(&mut conn).await.unwrap();
            insert_ready(
                &mut conn,
                "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbb1",
                "guest-thr1",
            )
            .await;
            insert_ready(
                &mut conn,
                "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbb2",
                "guest-thr2",
            )
            .await;
        }

        let results = Arc::new(Mutex::new(Vec::new()));
        let mut handles = Vec::new();
        for _ in 0..2 {
            let pool = pool.clone();
            let results = Arc::clone(&results);
            handles.push(tokio::spawn(async move {
                let mut conn = pool.acquire().await.unwrap();
                let assigned = assign_ready_guest(&mut conn, 60).await.unwrap();
                results.lock().unwrap().push(assigned);
            }));
        }
        for h in handles {
            h.await.expect("assign task");
        }
        let got = results.lock().unwrap();
        let ids: Vec<String> = got
            .iter()
            .map(|row| row.as_ref().expect("task got a ready guest").0.clone())
            .collect();
        assert_eq!(ids.len(), 2);
        assert_ne!(ids[0], ids[1], "concurrent assigns shared a guest");
    }

    #[tokio::test]
    async fn sweep_deletes_expired_assigned_only() {
        let (pool, _dir) = engine::test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        schema::ensure_vault_schema(&mut conn).await.unwrap();
        let env = test_config();
        let data_dir = &env.paths.data_dir;

        insert_ready(
            &mut conn,
            "cccccccc-cccc-4ccc-8ccc-ccccccccccc1",
            "guest-ready",
        )
        .await;
        std::fs::create_dir_all(data_dir.join("cccccccc-cccc-4ccc-8ccc-ccccccccccc1")).unwrap();

        insert_ready(
            &mut conn,
            "cccccccc-cccc-4ccc-8ccc-ccccccccccc2",
            "guest-live",
        )
        .await;
        account_profile::set_guest_status(
            &mut conn,
            "cccccccc-cccc-4ccc-8ccc-ccccccccccc2",
            "assigned",
        )
        .await
        .unwrap();
        session_tokens::insert_account_session_token_with_ttl(
            &mut conn,
            "cccccccc-cccc-4ccc-8ccc-ccccccccccc2",
            3600,
        )
        .await
        .unwrap();
        std::fs::create_dir_all(data_dir.join("cccccccc-cccc-4ccc-8ccc-ccccccccccc2")).unwrap();

        insert_ready(
            &mut conn,
            "cccccccc-cccc-4ccc-8ccc-ccccccccccc3",
            "guest-expired",
        )
        .await;
        account_profile::set_guest_status(
            &mut conn,
            "cccccccc-cccc-4ccc-8ccc-ccccccccccc3",
            "assigned",
        )
        .await
        .unwrap();
        session_tokens::insert_account_session_token_with_ttl(
            &mut conn,
            "cccccccc-cccc-4ccc-8ccc-ccccccccccc3",
            3600,
        )
        .await
        .unwrap();
        sqlx::query("UPDATE account_session_tokens SET expires_at = '1' WHERE account_id = $1")
            .bind("cccccccc-cccc-4ccc-8ccc-ccccccccccc3")
            .execute(&mut *conn)
            .await
            .unwrap();
        std::fs::create_dir_all(data_dir.join("cccccccc-cccc-4ccc-8ccc-ccccccccccc3")).unwrap();

        insert_ready(
            &mut conn,
            "cccccccc-cccc-4ccc-8ccc-ccccccccccc4",
            "guest-nosess",
        )
        .await;
        account_profile::set_guest_status(
            &mut conn,
            "cccccccc-cccc-4ccc-8ccc-ccccccccccc4",
            "assigned",
        )
        .await
        .unwrap();
        std::fs::create_dir_all(data_dir.join("cccccccc-cccc-4ccc-8ccc-ccccccccccc4")).unwrap();

        let deleted = sweep_expired_guests(&mut conn, data_dir).await.unwrap();
        assert_eq!(deleted, 2);
        assert_eq!(
            account_profile::guest_status(&mut conn, "cccccccc-cccc-4ccc-8ccc-ccccccccccc1")
                .await
                .unwrap()
                .as_deref(),
            Some("ready")
        );
        assert_eq!(
            account_profile::guest_status(&mut conn, "cccccccc-cccc-4ccc-8ccc-ccccccccccc2")
                .await
                .unwrap()
                .as_deref(),
            Some("assigned")
        );
        assert!(
            account_profile::username_for_account(
                &mut conn,
                "cccccccc-cccc-4ccc-8ccc-ccccccccccc3"
            )
            .await
            .unwrap()
            .is_none()
        );
        assert!(
            account_profile::username_for_account(
                &mut conn,
                "cccccccc-cccc-4ccc-8ccc-ccccccccccc4"
            )
            .await
            .unwrap()
            .is_none()
        );
        assert!(
            data_dir
                .join("cccccccc-cccc-4ccc-8ccc-ccccccccccc1")
                .is_dir()
        );
        assert!(
            data_dir
                .join("cccccccc-cccc-4ccc-8ccc-ccccccccccc2")
                .is_dir()
        );
        assert!(
            !data_dir
                .join("cccccccc-cccc-4ccc-8ccc-ccccccccccc3")
                .exists()
        );
        assert!(
            !data_dir
                .join("cccccccc-cccc-4ccc-8ccc-ccccccccccc4")
                .exists()
        );
    }

    #[tokio::test]
    async fn refill_respects_max() {
        let (pool, _dir) = engine::test_pool().await;
        tiny_template(&pool).await;
        let env = test_config();
        let settings = GuestDemoSettings {
            enabled: true,
            pool_min: 1,
            pool_max: 2,
            session_secs: 60,
        };
        let mut conn = pool.acquire().await.unwrap();
        let created = refill_pool(&mut conn, &env, settings, 50).await.unwrap();
        assert!(created <= 2, "created {created} clones");
        assert_eq!(count_ready(&mut conn).await.unwrap(), 2);
        assert_eq!(created, 2);

        clone_template_to_guest(&mut conn, &env, DEMO_ACCOUNT_ID)
            .await
            .unwrap();
        assert_eq!(count_ready(&mut conn).await.unwrap(), 3);
        let created_again = refill_pool(&mut conn, &env, settings, 50).await.unwrap();
        assert_eq!(created_again, 0);
        assert_eq!(count_ready(&mut conn).await.unwrap(), 2);
    }

    #[tokio::test]
    async fn refill_one_creates_at_most_one() {
        let (pool, _dir) = engine::test_pool().await;
        tiny_template(&pool).await;
        let env = test_config();
        let settings = GuestDemoSettings {
            enabled: true,
            pool_min: 2,
            pool_max: 2,
            session_secs: 60,
        };
        let mut conn = pool.acquire().await.unwrap();
        assert_eq!(refill_one(&mut conn, &env, settings, 0).await.unwrap(), 1);
        assert_eq!(count_ready(&mut conn).await.unwrap(), 1);
        assert_eq!(refill_one(&mut conn, &env, settings, 0).await.unwrap(), 1);
        assert_eq!(count_ready(&mut conn).await.unwrap(), 2);
        assert_eq!(refill_one(&mut conn, &env, settings, 0).await.unwrap(), 0);
        assert_eq!(count_ready(&mut conn).await.unwrap(), 2);
    }

    #[test]
    fn guest_pool_state_counts_assignments_inside_15m_window() {
        let mut state = GuestPoolState::new();
        assert_eq!(state.count_last_15m(), 0);
        state.record_assignment_at(Instant::now() - Duration::from_secs(16 * 60));
        state.record_assignment();
        state.record_assignment();
        assert_eq!(state.count_last_15m(), 2);
    }

    #[tokio::test]
    async fn drop_ready_guests_leaves_assigned() {
        let (pool, _dir) = engine::test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        schema::ensure_vault_schema(&mut conn).await.unwrap();
        let env = test_config();
        let data_dir = &env.paths.data_dir;
        insert_ready(
            &mut conn,
            "dddddddd-dddd-4ddd-8ddd-ddddddddddd1",
            "guest-drop1",
        )
        .await;
        insert_ready(
            &mut conn,
            "dddddddd-dddd-4ddd-8ddd-ddddddddddd2",
            "guest-keep",
        )
        .await;
        account_profile::set_guest_status(
            &mut conn,
            "dddddddd-dddd-4ddd-8ddd-ddddddddddd2",
            "assigned",
        )
        .await
        .unwrap();
        std::fs::create_dir_all(data_dir.join("dddddddd-dddd-4ddd-8ddd-ddddddddddd1")).unwrap();
        std::fs::create_dir_all(data_dir.join("dddddddd-dddd-4ddd-8ddd-ddddddddddd2")).unwrap();

        let dropped = drop_ready_guests(&mut conn, data_dir).await.unwrap();
        assert_eq!(dropped, 1);
        assert_eq!(count_ready(&mut conn).await.unwrap(), 0);
        assert_eq!(
            account_profile::guest_status(&mut conn, "dddddddd-dddd-4ddd-8ddd-ddddddddddd2")
                .await
                .unwrap()
                .as_deref(),
            Some("assigned")
        );
        assert!(
            !data_dir
                .join("dddddddd-dddd-4ddd-8ddd-ddddddddddd1")
                .exists()
        );
        assert!(
            data_dir
                .join("dddddddd-dddd-4ddd-8ddd-ddddddddddd2")
                .is_dir()
        );
    }
}
