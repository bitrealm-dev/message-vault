//! Database engine detection and pool construction.

use std::path::Path;
use std::str::FromStr;

use anyhow::{Result, bail};
use sqlx::any::{AnyConnectOptions, AnyPoolOptions};
use sqlx::sqlite::SqliteConnectOptions;
use sqlx::{AnyPool, ConnectOptions};

/// Which database engine a connection URL selects.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DbEngine {
    Sqlite,
    Postgres,
}

/// Resolve the engine from a connection URL scheme.
pub fn detect_engine(url: &str) -> Result<DbEngine> {
    let scheme = url.split("://").next().unwrap_or("");
    match scheme {
        "sqlite" | "sqlite-file" => Ok(DbEngine::Sqlite),
        "postgres" | "postgresql" => Ok(DbEngine::Postgres),
        _ => bail!("unsupported database URL scheme {scheme:?} (want sqlite:// or postgres://)"),
    }
}

/// The vault's historical pragma set, applied to each new connection:
/// busy timeout first (overlapping auth and UI writes wait), foreign keys on,
/// synchronous NORMAL, temp_store MEMORY, cache_size -200000.
fn with_vault_pragmas(pool: AnyPoolOptions) -> AnyPoolOptions {
    pool.after_connect(|conn, _meta| {
        Box::pin(async move {
            sqlx::query("PRAGMA busy_timeout = 15000")
                .execute(&mut *conn)
                .await?;
            sqlx::query("PRAGMA foreign_keys = ON")
                .execute(&mut *conn)
                .await?;
            sqlx::query("PRAGMA synchronous = NORMAL")
                .execute(&mut *conn)
                .await?;
            sqlx::query("PRAGMA temp_store = MEMORY")
                .execute(&mut *conn)
                .await?;
            sqlx::query("PRAGMA cache_size = -200000")
                .execute(&mut *conn)
                .await?;
            Ok(())
        })
    })
}

fn sqlite_url_from_path(path: &Path) -> String {
    SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(true)
        .to_url_lossy()
        .to_string()
}

fn sqlite_pool_options() -> AnyPoolOptions {
    with_vault_pragmas(AnyPoolOptions::new().max_connections(4))
}

/// Best-effort WAL enablement shared by the path- and URL-based SQLite
/// pools: a hot rollback journal or another process holding the database
/// can make it fail, and callers still get a usable pool.
async fn try_enable_wal(pool: &AnyPool) {
    match sqlx::query("PRAGMA journal_mode = WAL").execute(pool).await {
        Ok(_) => {}
        Err(err) => {
            eprintln!(
                "warning: could not enable write-ahead logging ({err}); continuing without it"
            );
        }
    }
}

/// Open the configured pool for a SQLite file.
pub async fn open_pool_for_path(path: &Path) -> Result<AnyPool> {
    let pool = sqlite_pool_options()
        .connect_with(AnyConnectOptions::from_str(&sqlite_url_from_path(path))?)
        .await?;
    try_enable_wal(&pool).await;
    Ok(pool)
}

/// Open a pool from a user-supplied connection URL (`sqlite://…` or
/// `postgres://…`). The scheme selects the engine.
///
/// The `sqlite://` path matches [`open_pool_for_path`]: the file is created
/// when missing and WAL is enabled best-effort. A `mode=` in the URL is
/// overridden on purpose — the vault always reads and writes its database.
/// Postgres has no equivalent.
pub async fn open_pool_from_url(url: &str) -> Result<AnyPool> {
    let engine = detect_engine(url)?;
    if engine == DbEngine::Sqlite {
        let options: SqliteConnectOptions =
            SqliteConnectOptions::from_str(url)?.create_if_missing(true);
        // Round-trip through the URL form: that is the conversion path
        // `open_pool_for_path` uses, and it preserves create_if_missing
        // (mode=rwc).
        let pool = sqlite_pool_options()
            .connect_with(AnyConnectOptions::from_str(
                options.to_url_lossy().as_ref(),
            )?)
            .await?;
        try_enable_wal(&pool).await;
        return Ok(pool);
    }
    Ok(AnyPoolOptions::new()
        .max_connections(4)
        .connect(url)
        .await?)
}

/// Shared test pool: file-backed SQLite in a fresh temp dir.
#[cfg(test)]
pub(crate) async fn test_pool() -> (AnyPool, tempfile::TempDir) {
    sqlx::any::install_default_drivers();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("vault.db");
    let pool = sqlite_pool_options()
        .connect_with(AnyConnectOptions::from_str(&sqlite_url_from_path(&path)).unwrap())
        .await
        .unwrap();
    (pool, dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_engine_from_scheme() {
        assert_eq!(
            detect_engine("sqlite://data/vault.db").unwrap(),
            DbEngine::Sqlite
        );
        assert_eq!(
            detect_engine("sqlite:///abs/path.db").unwrap(),
            DbEngine::Sqlite
        );
        assert_eq!(
            detect_engine("postgres://u:p@h/db").unwrap(),
            DbEngine::Postgres
        );
        assert_eq!(
            detect_engine("postgresql://h/db").unwrap(),
            DbEngine::Postgres
        );
        assert!(detect_engine("mysql://h/db").is_err());
        assert!(detect_engine("not-a-url").is_err());
    }

    #[tokio::test]
    async fn opens_sqlite_pool_and_applies_pragmas() {
        let (pool, _dir) = test_pool().await;
        // All five vault pragmas, read back through their pragma table
        // functions (values must match with_vault_pragmas).
        let busy_timeout: i64 = sqlx::query_scalar("SELECT timeout FROM pragma_busy_timeout")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(busy_timeout, 15000, "busy_timeout");
        let on: i64 = sqlx::query_scalar("SELECT foreign_keys FROM pragma_foreign_keys")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(on, 1, "foreign_keys");
        let synchronous: i64 = sqlx::query_scalar("SELECT synchronous FROM pragma_synchronous")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(synchronous, 1, "synchronous must be NORMAL");
        let temp_store: i64 = sqlx::query_scalar("SELECT temp_store FROM pragma_temp_store")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(temp_store, 2, "temp_store must be MEMORY");
        let cache_size: i64 = sqlx::query_scalar("SELECT cache_size FROM pragma_cache_size")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(cache_size, -200000, "cache_size");
        // The pool is usable for real work.
        sqlx::query("CREATE TABLE t1 (id INTEGER PRIMARY KEY, v TEXT)")
            .execute(&pool)
            .await
            .unwrap();
    }
}
