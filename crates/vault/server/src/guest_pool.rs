//! Ready-guest pool: assign a session, refill unused copies, and delete expired ones.

use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};

use crate::config::{Config, GuestDemoSettings};
use crate::db::account_profile::{self, DEMO_ACCOUNT_ID};
use crate::db::session_tokens;
use crate::guest_clone::clone_template_to_guest;

/// Count unused ready guest accounts sitting in the pool.
///
/// # Errors
///
/// Returns an error when the count query fails.
pub fn count_ready(conn: &Connection) -> Result<u32> {
    let n: i64 = conn.query_row(
        "SELECT COUNT(*) FROM accounts WHERE guest_status = 'ready'",
        [],
        |row| row.get(0),
    )?;
    u32::try_from(n).context("ready guest count does not fit in u32")
}

/// Take one ready guest, mark it assigned, and issue a session token.
///
/// Uses `BEGIN IMMEDIATE` so two concurrent assigns cannot pick the same row.
/// Returns `Ok(None)` when the pool is empty.
///
/// # Errors
///
/// Returns an error when the transaction, status update, or token insert fails.
pub fn assign_ready_guest(
    conn: &mut Connection,
    session_secs: u64,
) -> Result<Option<(String, String, String)>> {
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let picked: Option<(String, String)> = tx
        .query_row(
            r#"
            SELECT id, username FROM accounts
            WHERE guest_status = 'ready'
            ORDER BY id
            LIMIT 1
            "#,
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    let Some((account_id, username)) = picked else {
        tx.commit()?;
        return Ok(None);
    };
    account_profile::set_guest_status(&tx, &account_id, "assigned")?;
    let token =
        session_tokens::insert_account_session_token_with_ttl(&tx, &account_id, session_secs)?;
    tx.commit()?;
    Ok(Some((account_id, username, token)))
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
pub fn refill_pool(
    conn: &mut Connection,
    cfg: &Config,
    settings: GuestDemoSettings,
    assignments_last_15m: u32,
) -> Result<u32> {
    let target = settings
        .pool_min
        .max(assignments_last_15m)
        .min(settings.pool_max);
    let ready = count_ready(conn)?;
    if ready > settings.pool_max {
        let excess = ready - settings.pool_max;
        drop_oldest_ready(conn, &cfg.paths.data_dir, excess)?;
    }
    let mut created = 0u32;
    while count_ready(conn)? < target {
        clone_template_to_guest(conn, cfg, DEMO_ACCOUNT_ID)?;
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
pub fn sweep_expired_guests(conn: &Connection, data_dir: &Path) -> Result<u32> {
    let now = now_unix_secs();
    let ids = list_expired_assigned(conn, now)?;
    let n = u32::try_from(ids.len()).context("expired guest count does not fit in u32")?;
    for id in ids {
        delete_guest_account_and_files(conn, data_dir, &id)?;
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
pub fn drop_ready_guests(conn: &Connection, data_dir: &Path) -> Result<u32> {
    let ids = list_ready_ids(conn)?;
    let n = u32::try_from(ids.len()).context("ready guest count does not fit in u32")?;
    for id in ids {
        delete_guest_account_and_files(conn, data_dir, &id)?;
    }
    Ok(n)
}

fn list_ready_ids(conn: &Connection) -> Result<Vec<String>> {
    let mut stmt =
        conn.prepare("SELECT id FROM accounts WHERE guest_status = 'ready' ORDER BY id")?;
    let rows = stmt
        .query_map([], |row| row.get(0))?
        .collect::<rusqlite::Result<Vec<String>>>()?;
    Ok(rows)
}

fn drop_oldest_ready(conn: &Connection, data_dir: &Path, n: u32) -> Result<()> {
    let mut stmt =
        conn.prepare("SELECT id FROM accounts WHERE guest_status = 'ready' ORDER BY id LIMIT ?1")?;
    let ids = stmt
        .query_map(params![n], |row| row.get(0))?
        .collect::<rusqlite::Result<Vec<String>>>()?;
    drop(stmt);
    for id in ids {
        delete_guest_account_and_files(conn, data_dir, &id)?;
    }
    Ok(())
}

fn list_expired_assigned(conn: &Connection, now_secs: u64) -> Result<Vec<String>> {
    let now = i64::try_from(now_secs).unwrap_or(i64::MAX);
    let mut stmt = conn.prepare(
        r#"
        SELECT a.id
        FROM accounts a
        LEFT JOIN account_session_tokens t ON t.account_id = a.id
        WHERE a.guest_status = 'assigned'
          AND (
            t.account_id IS NULL
            OR CAST(t.expires_at AS INTEGER) <= ?1
          )
        "#,
    )?;
    let rows = stmt
        .query_map(params![now], |row| row.get(0))?
        .collect::<rusqlite::Result<Vec<String>>>()?;
    Ok(rows)
}

fn delete_guest_account_and_files(
    conn: &Connection,
    data_dir: &Path,
    account_id: &str,
) -> Result<()> {
    account_profile::delete_account(conn, account_id)?;
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
    use crate::db::schema;
    use crate::db::session_tokens;
    use crate::guest_clone::clone_template_to_guest;
    use rusqlite::{Connection, params};
    use std::path::PathBuf;
    use std::sync::{Arc, Mutex};
    use std::thread;

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
            },
            _tmp: tmp,
        }
    }

    fn tiny_template(conn: &Connection) {
        schema::ensure_vault_schema(conn).unwrap();
        conn.execute(
            "INSERT INTO accounts (id, username, read_only, preferred_name)
             VALUES (?1, 'demo', 1, 'Alex Demo')",
            params![DEMO_ACCOUNT_ID],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO handles (account_id, raw, normalized, handle_type, service)
             VALUES (?1, '+15555550100', '+15555550100', 'phone', 'phone')",
            params![DEMO_ACCOUNT_ID],
        )
        .unwrap();
        let hid = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO account_handles (account_id, handle_id) VALUES (?1, ?2)",
            params![DEMO_ACCOUNT_ID, hid],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO conversations (account_id, chat_handle_id, conversation_type, source_file)
             VALUES (?1, ?2, 'individual', 'a.jsonl')",
            params![DEMO_ACCOUNT_ID, hid],
        )
        .unwrap();
        let cid = conn.last_insert_rowid();
        conn.execute(
            r#"INSERT INTO messages (
                conversation_id, account_id, source, guid, timestamp, is_from_me, sort_order, body
            ) VALUES (?1, ?2, 'imessage', 'g1', '2020-01-01T00:00:00Z', 1, 0, 'hello')"#,
            params![cid, DEMO_ACCOUNT_ID],
        )
        .unwrap();
    }

    fn memory_conn() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        conn
    }

    fn insert_ready(conn: &Connection, id: &str, username: &str) {
        account_profile::insert_guest_account(conn, id, username, Some("Guest")).unwrap();
    }

    #[test]
    fn assign_marks_assigned_and_issues_token() {
        let mut conn = memory_conn();
        tiny_template(&conn);
        let env = test_config();
        clone_template_to_guest(&mut conn, &env, DEMO_ACCOUNT_ID).unwrap();
        clone_template_to_guest(&mut conn, &env, DEMO_ACCOUNT_ID).unwrap();
        assert_eq!(count_ready(&conn).unwrap(), 2);

        let (account_id, username, token) = assign_ready_guest(&mut conn, 120)
            .unwrap()
            .expect("ready guest");
        assert!(username.starts_with("guest-"), "{username}");
        assert!(token.starts_with("mv-user-"), "{token}");
        assert_eq!(
            account_profile::guest_status(&conn, &account_id)
                .unwrap()
                .as_deref(),
            Some("assigned")
        );
        assert_eq!(
            session_tokens::lookup_account_for_token(&conn, &token).unwrap(),
            Some(account_id.clone())
        );
        assert_eq!(count_ready(&conn).unwrap(), 1);
        assert!(assign_ready_guest(&mut conn, 120).unwrap().is_some());
        assert_eq!(count_ready(&conn).unwrap(), 0);
        assert!(assign_ready_guest(&mut conn, 120).unwrap().is_none());
    }

    #[test]
    fn two_assigns_never_share_a_guest() {
        let mut conn = memory_conn();
        schema::ensure_vault_schema(&conn).unwrap();
        insert_ready(&conn, "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaa1", "guest-seq1");
        insert_ready(&conn, "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaa2", "guest-seq2");
        let a = assign_ready_guest(&mut conn, 60).unwrap().unwrap();
        let b = assign_ready_guest(&mut conn, 60).unwrap().unwrap();
        assert_ne!(a.0, b.0, "sequential assigns shared a guest");
        assert_ne!(a.2, b.2, "sequential assigns shared a token");

        let tmp = tempfile::tempdir().expect("file db dir");
        let db_path: PathBuf = tmp.path().join("pool.db");
        {
            let conn = schema::open_configured(&db_path).unwrap();
            schema::ensure_vault_schema(&conn).unwrap();
            insert_ready(&conn, "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbb1", "guest-thr1");
            insert_ready(&conn, "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbb2", "guest-thr2");
        }

        let results = Arc::new(Mutex::new(Vec::new()));
        let mut handles = Vec::new();
        for _ in 0..2 {
            let path = db_path.clone();
            let results = Arc::clone(&results);
            handles.push(thread::spawn(move || {
                let mut conn = schema::open_configured(&path).unwrap();
                let assigned = assign_ready_guest(&mut conn, 60).unwrap();
                results.lock().unwrap().push(assigned);
            }));
        }
        for h in handles {
            h.join().expect("assign thread");
        }
        let got = results.lock().unwrap();
        let ids: Vec<String> = got
            .iter()
            .map(|row| row.as_ref().expect("thread got a ready guest").0.clone())
            .collect();
        assert_eq!(ids.len(), 2);
        assert_ne!(ids[0], ids[1], "concurrent assigns shared a guest");
    }

    #[test]
    fn sweep_deletes_expired_assigned_only() {
        let conn = memory_conn();
        schema::ensure_vault_schema(&conn).unwrap();
        let env = test_config();
        let data_dir = &env.paths.data_dir;

        insert_ready(&conn, "cccccccc-cccc-4ccc-8ccc-ccccccccccc1", "guest-ready");
        std::fs::create_dir_all(data_dir.join("cccccccc-cccc-4ccc-8ccc-ccccccccccc1")).unwrap();

        insert_ready(&conn, "cccccccc-cccc-4ccc-8ccc-ccccccccccc2", "guest-live");
        account_profile::set_guest_status(
            &conn,
            "cccccccc-cccc-4ccc-8ccc-ccccccccccc2",
            "assigned",
        )
        .unwrap();
        session_tokens::insert_account_session_token_with_ttl(
            &conn,
            "cccccccc-cccc-4ccc-8ccc-ccccccccccc2",
            3600,
        )
        .unwrap();
        std::fs::create_dir_all(data_dir.join("cccccccc-cccc-4ccc-8ccc-ccccccccccc2")).unwrap();

        insert_ready(
            &conn,
            "cccccccc-cccc-4ccc-8ccc-ccccccccccc3",
            "guest-expired",
        );
        account_profile::set_guest_status(
            &conn,
            "cccccccc-cccc-4ccc-8ccc-ccccccccccc3",
            "assigned",
        )
        .unwrap();
        session_tokens::insert_account_session_token_with_ttl(
            &conn,
            "cccccccc-cccc-4ccc-8ccc-ccccccccccc3",
            3600,
        )
        .unwrap();
        conn.execute(
            "UPDATE account_session_tokens SET expires_at = '1' WHERE account_id = ?1",
            params!["cccccccc-cccc-4ccc-8ccc-ccccccccccc3"],
        )
        .unwrap();
        std::fs::create_dir_all(data_dir.join("cccccccc-cccc-4ccc-8ccc-ccccccccccc3")).unwrap();

        insert_ready(
            &conn,
            "cccccccc-cccc-4ccc-8ccc-ccccccccccc4",
            "guest-nosess",
        );
        account_profile::set_guest_status(
            &conn,
            "cccccccc-cccc-4ccc-8ccc-ccccccccccc4",
            "assigned",
        )
        .unwrap();
        std::fs::create_dir_all(data_dir.join("cccccccc-cccc-4ccc-8ccc-ccccccccccc4")).unwrap();

        let deleted = sweep_expired_guests(&conn, data_dir).unwrap();
        assert_eq!(deleted, 2);
        assert_eq!(
            account_profile::guest_status(&conn, "cccccccc-cccc-4ccc-8ccc-ccccccccccc1")
                .unwrap()
                .as_deref(),
            Some("ready")
        );
        assert_eq!(
            account_profile::guest_status(&conn, "cccccccc-cccc-4ccc-8ccc-ccccccccccc2")
                .unwrap()
                .as_deref(),
            Some("assigned")
        );
        assert!(
            account_profile::username_for_account(&conn, "cccccccc-cccc-4ccc-8ccc-ccccccccccc3")
                .unwrap()
                .is_none()
        );
        assert!(
            account_profile::username_for_account(&conn, "cccccccc-cccc-4ccc-8ccc-ccccccccccc4")
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

    #[test]
    fn refill_respects_max() {
        let mut conn = memory_conn();
        tiny_template(&conn);
        let env = test_config();
        let settings = GuestDemoSettings {
            enabled: true,
            pool_min: 1,
            pool_max: 2,
            session_secs: 60,
        };
        let created = refill_pool(&mut conn, &env, settings, 50).unwrap();
        assert!(created <= 2, "created {created} clones");
        assert_eq!(count_ready(&conn).unwrap(), 2);
        assert_eq!(created, 2);

        clone_template_to_guest(&mut conn, &env, DEMO_ACCOUNT_ID).unwrap();
        assert_eq!(count_ready(&conn).unwrap(), 3);
        let created_again = refill_pool(&mut conn, &env, settings, 50).unwrap();
        assert_eq!(created_again, 0);
        assert_eq!(count_ready(&conn).unwrap(), 2);
    }

    #[test]
    fn drop_ready_guests_leaves_assigned() {
        let conn = memory_conn();
        schema::ensure_vault_schema(&conn).unwrap();
        let env = test_config();
        let data_dir = &env.paths.data_dir;
        insert_ready(&conn, "dddddddd-dddd-4ddd-8ddd-ddddddddddd1", "guest-drop1");
        insert_ready(&conn, "dddddddd-dddd-4ddd-8ddd-ddddddddddd2", "guest-keep");
        account_profile::set_guest_status(
            &conn,
            "dddddddd-dddd-4ddd-8ddd-ddddddddddd2",
            "assigned",
        )
        .unwrap();
        std::fs::create_dir_all(data_dir.join("dddddddd-dddd-4ddd-8ddd-ddddddddddd1")).unwrap();
        std::fs::create_dir_all(data_dir.join("dddddddd-dddd-4ddd-8ddd-ddddddddddd2")).unwrap();

        let dropped = drop_ready_guests(&conn, data_dir).unwrap();
        assert_eq!(dropped, 1);
        assert_eq!(count_ready(&conn).unwrap(), 0);
        assert_eq!(
            account_profile::guest_status(&conn, "dddddddd-dddd-4ddd-8ddd-ddddddddddd2")
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
