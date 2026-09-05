//! Database engine detection and pool construction.

use std::fmt;
use std::path::Path;
use std::str::FromStr;

use anyhow::{Context, Result, bail};
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

/// `sqlite://` URL for a file path, with create-if-missing set.
fn sqlite_url_from_path(path: &Path) -> String {
    SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(true)
        .to_url_lossy()
        .to_string()
}

/// Pool options for SQLite: four connections plus the vault pragmas.
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

/// Where a command finds its database: a connection URL when one was given,
/// otherwise a SQLite file from the config or `--db`.
///
/// The URL always wins because it can name a Postgres server, which a path
/// never can. Every CLI entry point and the HTTP server resolve their
/// database through this type so the choice, the error context, and the
/// credential redaction live in one place.
#[derive(Debug, Clone, Copy)]
pub enum DbTarget<'a> {
    /// `sqlite://…` or `postgres://…` connection URL.
    Url(&'a str),
    /// SQLite file path.
    Path(&'a Path),
}

impl<'a> DbTarget<'a> {
    /// The URL when one was supplied, else the path.
    pub fn new(url: Option<&'a str>, path: &'a Path) -> Self {
        match url {
            Some(url) => Self::Url(url),
            None => Self::Path(path),
        }
    }

    /// Open the pool for this target, naming it (with credentials redacted)
    /// in the error context.
    ///
    /// # Errors
    ///
    /// Returns an error when the URL scheme is unknown or the connection fails.
    pub async fn open(self) -> Result<AnyPool> {
        match self {
            Self::Url(url) => open_pool_from_url(url).await,
            Self::Path(path) => open_pool_for_path(path).await,
        }
        .with_context(|| format!("failed to open database {self}"))
    }
}

impl fmt::Display for DbTarget<'_> {
    /// The target for status lines and errors; never includes credentials.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Url(url) => f.write_str(&redact_db_url(url)),
            Self::Path(path) => write!(f, "{}", path.display()),
        }
    }
}

/// A database URL with credentials stripped, safe for status and error
/// output: `postgres://user:secret@host:5432/db` prints as
/// `postgres://host:5432/db`. Query parameters (which can carry secrets of
/// their own) are dropped too. Inputs that are not `scheme://…` URLs print
/// as a placeholder instead of being echoed raw.
///
/// Best effort: a malformed URL — a `/` or `#` inside the password, for
/// instance — can defeat the splits and leak credentials into the error
/// context. sqlx rejects such URLs before any output is produced, so this
/// only ever prints URLs that failed to open for other reasons.
pub fn redact_db_url(url: &str) -> String {
    let Some((scheme, rest)) = url.split_once("://") else {
        return "<db url>".to_string();
    };
    let rest = rest.split_once('?').map_or(rest, |(r, _)| r);
    let (authority, path) = match rest.split_once('/') {
        Some((authority, path)) => (authority, format!("/{path}")),
        None => (rest, String::new()),
    };
    let host = authority
        .rsplit_once('@')
        .map_or(authority, |(_, host)| host);
    format!("{scheme}://{host}{path}")
}

/// Shared test pool, plus a fresh temp dir for the test's files.
///
/// File-backed SQLite in that temp dir by default. When `MV_TEST_POSTGRES_URL`
/// is set, a schema of its own on that Postgres server instead, so the same
/// suite runs against the other engine and a green Postgres job means the
/// SQL ran on Postgres (#339). A test about SQLite itself takes
/// [`sqlite_test_pool`].
#[cfg(test)]
pub(crate) async fn test_pool() -> (AnyPool, tempfile::TempDir) {
    if let Some(url) = crate::pg_test_url() {
        sqlx::any::install_default_drivers();
        let dir = tempfile::tempdir().unwrap();
        return (pg_test_schema_pool(&url).await, dir);
    }
    sqlite_test_pool().await
}

/// File-backed SQLite in a fresh temp dir, whatever `MV_TEST_POSTGRES_URL`
/// says: for a test whose subject is SQLite (a pragma, the file on disk,
/// `sqlite_master`).
#[cfg(test)]
pub(crate) async fn sqlite_test_pool() -> (AnyPool, tempfile::TempDir) {
    sqlx::any::install_default_drivers();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("vault.db");
    let pool = sqlite_pool_options()
        .connect_with(AnyConnectOptions::from_str(&sqlite_url_from_path(&path)).unwrap())
        .await
        .unwrap();
    (pool, dir)
}

/// A pool whose connections all run in a schema created for this one test.
///
/// The schema is named `mvtest_<pid>_<n>`, so tests in one process never
/// share one and a later process can tell the leftovers of an earlier one
/// apart from its own. The first call in a process drops every `mvtest_`
/// schema another process left behind: a crashed or interrupted run must
/// not fill the shared test database. The connection URL carries
/// `options=-csearch_path=<schema>`, so every table `ensure_vault_schema`
/// creates lands in that schema and every query reads from it.
#[cfg(test)]
async fn pg_test_schema_pool(url: &str) -> AnyPool {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    static SWEPT: tokio::sync::OnceCell<()> = tokio::sync::OnceCell::const_new();

    let pid = std::process::id();
    let schema = format!("mvtest_{pid}_{}", NEXT.fetch_add(1, Ordering::Relaxed));
    let admin = AnyPoolOptions::new()
        .max_connections(1)
        .connect(url)
        .await
        .expect("connect to MV_TEST_POSTGRES_URL");
    SWEPT
        .get_or_init(|| async {
            let mine = format!("mvtest_{pid}_%");
            let stale: Vec<String> = sqlx::query_scalar(
                "SELECT nspname FROM pg_namespace WHERE nspname LIKE 'mvtest_%' AND nspname NOT LIKE $1",
            )
            .bind(&mine)
            .fetch_all(&admin)
            .await
            .unwrap_or_default();
            for name in stale {
                let _ = sqlx::query(&format!("DROP SCHEMA IF EXISTS \"{name}\" CASCADE"))
                    .execute(&admin)
                    .await;
            }
        })
        .await;
    sqlx::query(&format!("CREATE SCHEMA \"{schema}\""))
        .execute(&admin)
        .await
        .expect("create the test schema");
    admin.close().await;

    let separator = if url.contains('?') { '&' } else { '?' };
    let scoped = format!("{url}{separator}options=-csearch_path%3D{schema}");
    AnyPoolOptions::new()
        .max_connections(4)
        .connect(&scoped)
        .await
        .expect("connect to the test schema")
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
        let (pool, _dir) = sqlite_test_pool().await;
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
