//! Regenerate the demo bundle, clear the demo account's data, re-import, and process media.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use message_ir::HandleType;
use serde::Deserialize;
use sqlx::Row;

use crate::config::Config;
use crate::db::account_profile;
use crate::db::dialect;
use crate::db::engine::{self, DbTarget};
use crate::db::schema;
use crate::dedupe;
use crate::import::{self, ImportExportArgs, ImportMode};
use crate::process_assets::{self, ProcessAssetsOptions};

/// Stable demo account id used when `reset-demo` runs without `--account`.
pub use crate::db::account_profile::DEMO_ACCOUNT_ID;

const IMESSAGE_SOURCE: &str = "imessage";
const SBR_SOURCE: &str = "sms-backup-restore";
const WHATSAPP_SOURCE: &str = "whatsapp";

/// Counts reported when a demo reset finishes.
#[derive(Debug)]
pub struct ResetDemoStats {
    /// Stats from regenerating the demo bundle.
    pub seed: demo_seed::GenStats,
    /// Stats from importing the regenerated bundle.
    pub import: import::ImportStats,
    /// Dedupe content keys filled during the reset (one per message; not a duplicate count).
    pub dedupe_keys_filled: u64,
    /// Stats from the post-import media processing pass.
    pub process_assets: process_assets::ProcessAssetsStats,
}

#[derive(Debug, Deserialize)]
struct DemoSeed {
    owner: DemoOwner,
    account: DemoAccount,
}

#[derive(Debug, Deserialize)]
struct DemoOwner {
    display_name: String,
    /// `(raw handle, handle type)` pairs linked into `account_handles`.
    #[serde(default)]
    handle_specs: Vec<(String, HandleType)>,
    #[serde(default)]
    emails: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct DemoAccount {
    username: String,
}

struct PreparedBundle {
    seed: DemoSeed,
    imessage_dir: PathBuf,
    sbr_dir: PathBuf,
    whatsapp_dir: PathBuf,
    contacts_vcf: PathBuf,
}

/// One per-source import in a demo reset. [`import_demo_sources`] loops over
/// [`DEMO_IMPORT_SOURCES`] for both transports, so the sources, their order,
/// and their Replace-then-Append modes stay in sync.
struct DemoImportSource {
    /// Label printed in the "Reset demo — preparing replacement" header.
    label: &'static str,
    /// Source id recorded on the imported conversations.
    source: &'static str,
    /// Staging directory inside the prepared bundle.
    staging_dir: fn(&PreparedBundle) -> &PathBuf,
    /// The first source replaces the demo account's data; the rest append.
    mode: ImportMode,
    /// Load the bundle's contacts VCF and overwrite existing contacts.
    with_contacts: bool,
}

const DEMO_IMPORT_SOURCES: [DemoImportSource; 3] = [
    DemoImportSource {
        label: "imessage",
        source: IMESSAGE_SOURCE,
        staging_dir: |bundle| &bundle.imessage_dir,
        mode: ImportMode::Replace,
        with_contacts: true,
    },
    DemoImportSource {
        label: "android",
        source: SBR_SOURCE,
        staging_dir: |bundle| &bundle.sbr_dir,
        mode: ImportMode::Append,
        with_contacts: false,
    },
    DemoImportSource {
        label: "whatsapp",
        source: WHATSAPP_SOURCE,
        staging_dir: |bundle| &bundle.whatsapp_dir,
        mode: ImportMode::Append,
        with_contacts: false,
    },
];

/// Print the "preparing replacement" header both reset transports share.
fn print_reset_header(account_id: &str, prepared: &PreparedBundle, db: &dyn std::fmt::Display) {
    println!("Reset demo — preparing replacement");
    println!("  account:      {account_id}");
    for source in &DEMO_IMPORT_SOURCES {
        println!(
            "  {:<14}{}",
            format!("{}:", source.label),
            (source.staging_dir)(prepared).display()
        );
    }
    println!("  db:           {db}");
}

/// Shared post-import tail: fill dedupe content keys, convert media, and warn
/// (but continue) when some attachments fail conversion.
async fn dedupe_and_process_assets(
    cfg: &Config,
    account_id: &str,
    target: DbTarget<'_>,
) -> Result<(dedupe::DedupeStats, process_assets::ProcessAssetsStats)> {
    let dedupe_stats = dedupe::run_dedupe(target, account_id, 2).await?;
    println!("Reset demo — processing prepared assets");
    let (db, db_url) = match target {
        DbTarget::Url(url) => (None, Some(url.to_string())),
        DbTarget::Path(path) => (Some(path.to_path_buf()), None),
    };
    let process_stats = process_assets::run(
        cfg,
        &ProcessAssetsOptions {
            force: false,
            dry_run: false,
            skip_image: false,
            skip_video: false,
            skip_audio: false,
            db,
            source: None,
            db_url,
        },
    )
    .await
    .context("process-assets after prepared demo import")?;
    if process_stats.errors > 0 {
        eprintln!(
            "warning: {} demo attachment(s) failed conversion; originals stay in place and reset-demo continues",
            process_stats.errors
        );
    }
    Ok((dedupe_stats, process_stats))
}

struct ResetPreparedStats {
    import: import::ImportStats,
    dedupe_keys_filled: u64,
    process_assets: process_assets::ProcessAssetsStats,
}

/// Parent directory of `path`, or `.` when the path has no parent.
fn parent_dir_or_cwd(path: &Path) -> &Path {
    match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent,
        _ => Path::new("."),
    }
}

/// Work directory for the prepared account tree. Must live on the same mount
/// as `data_dir` so the later install can `rename` into `data_dir/<account>`.
fn reset_account_work_dir(data_dir: &Path) -> Result<tempfile::TempDir> {
    fs::create_dir_all(data_dir)
        .with_context(|| format!("create data directory {}", data_dir.display()))?;
    tempfile::Builder::new()
        .prefix(".reset-demo-data-")
        .tempdir_in(data_dir)
        .with_context(|| {
            format!(
                "create temporary demo account directory in {}",
                data_dir.display()
            )
        })
}

/// Rename, or copy-then-remove when the paths sit on different mounts.
fn rename_prepared_path(source: &Path, destination: &Path) -> Result<()> {
    match fs::rename(source, destination) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::CrossesDevices => {
            move_across_devices(source, destination).with_context(|| {
                format!(
                    "copy {} to {} after a cross-device rename",
                    source.display(),
                    destination.display()
                )
            })
        }
        Err(error) => Err(error)
            .with_context(|| format!("rename {} to {}", source.display(), destination.display())),
    }
}

/// Move a file or folder even when `fs::rename` cannot (different filesystems): copy, then remove the source.
fn move_across_devices(source: &Path, destination: &Path) -> Result<()> {
    if source.is_dir() {
        copy_dir_recursive(source, destination)?;
        fs::remove_dir_all(source)
            .with_context(|| format!("remove copied directory {}", source.display()))?;
    } else {
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("create parent {}", parent.display()))?;
        }
        fs::copy(source, destination)
            .with_context(|| format!("copy {} to {}", source.display(), destination.display()))?;
        fs::remove_file(source)
            .with_context(|| format!("remove copied file {}", source.display()))?;
    }
    Ok(())
}

/// Copy a folder tree, creating `destination` as needed.
fn copy_dir_recursive(source: &Path, destination: &Path) -> Result<()> {
    fs::create_dir_all(destination).with_context(|| format!("create {}", destination.display()))?;
    for entry in fs::read_dir(source).with_context(|| format!("read {}", source.display()))? {
        let entry = entry?;
        let from = entry.path();
        let to = destination.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir_recursive(&from, &to)?;
        } else {
            fs::copy(&from, &to)
                .with_context(|| format!("copy {} to {}", from.display(), to.display()))?;
        }
    }
    Ok(())
}

/// Rebuild the demo vault from the bundle at `bundle` and write the active
/// config to `config_dest`.
///
/// # Errors
///
/// Returns an error when the bundle is incomplete, the database cannot be
/// replaced, or import / media processing fails.
pub async fn run_reset_demo(
    bundle: &Path,
    config_dest: &Path,
    db_url: Option<&str>,
) -> Result<ResetDemoStats> {
    let bundle = if bundle.is_absolute() {
        bundle.to_path_buf()
    } else {
        std::env::current_dir()?.join(bundle)
    };

    println!("  bundle:       {}", bundle.display());
    let seed_stats = maybe_regenerate_bundle(&bundle)?;
    let reset_stats =
        prepare_config_and_reset(&bundle, config_dest, DEMO_ACCOUNT_ID, db_url).await?;

    Ok(ResetDemoStats {
        seed: seed_stats,
        import: reset_stats.import,
        dedupe_keys_filled: reset_stats.dedupe_keys_filled,
        process_assets: reset_stats.process_assets,
    })
}

/// Refuse a config that serves the database from a URL. The SQLite reset
/// replaces the file at `paths.db`, which cannot reach a URL-served database;
/// `--db-url` takes the other transport and never reaches this check.
fn refuse_url_config(cfg: &Config) -> Result<()> {
    if let Some(url) = cfg.database.url.as_deref() {
        bail!(
            "reset-demo replaces the on-disk vault at paths.db, but this config serves the database from {}; URL-served databases cannot be reset this way — run reset-demo on the host that owns the database file, or pass --db-url",
            engine::redact_db_url(url)
        );
    }
    Ok(())
}

/// Copy the bundle's config into place and reset the account: the connection-URL path when
/// `db_url` is set, else the SQLite snapshot-and-swap path.
async fn prepare_config_and_reset(
    bundle: &Path,
    config_dest: &Path,
    account_id: &str,
    db_url: Option<&str>,
) -> Result<ResetPreparedStats> {
    validate_prepared_bundle(bundle)?;
    let demo_config = bundle.join("config/config.toml");
    if !demo_config.is_file() {
        bail!(
            "incomplete demo bundle under {} (need config/config.toml)",
            bundle.display()
        );
    }
    if let Some(url) = db_url {
        let cfg = if config_dest.is_file() {
            Config::load(config_dest)?
        } else {
            Config::load(&demo_config)?
        };
        return reset_prepared_bundle_at_url(&cfg, bundle, account_id, url).await;
    }
    let config_parent = parent_dir_or_cwd(config_dest);
    fs::create_dir_all(config_parent)
        .with_context(|| format!("create config directory {}", config_parent.display()))?;
    let temporary_config = tempfile::Builder::new()
        .prefix(".reset-demo-config-")
        .tempfile_in(config_parent)
        .context("create temporary demo config")?;
    fs::copy(&demo_config, temporary_config.path()).with_context(|| {
        format!(
            "copy prepared config {} to {}",
            demo_config.display(),
            temporary_config.path().display()
        )
    })?;
    let cfg = Config::load(temporary_config.path())?;
    refuse_url_config(&cfg)?;
    let temporary_config = temporary_config.into_temp_path();
    reset_prepared_bundle(
        &cfg,
        bundle,
        account_id,
        config_dest,
        temporary_config.as_ref(),
    )
    .await
}

/// Connection-URL reset (Postgres, or SQLite by URL): rebuild the demo account
/// in the live database. There is no snapshot to swap, so this path relies on
/// the wipe being scoped to one account.
async fn reset_prepared_bundle_at_url(
    cfg: &Config,
    bundle: &Path,
    account_id: &str,
    db_url: &str,
) -> Result<ResetPreparedStats> {
    let prepared = validate_prepared_bundle(bundle)?;
    rebuild_demo_account(cfg, &prepared, account_id, DbTarget::Url(db_url)).await
}

/// SQLite reset: build the new state in a prepared database next to the active one, prove
/// nothing outside the demo account changed, then swap it in.
async fn reset_prepared_bundle(
    cfg: &Config,
    bundle: &Path,
    account_id: &str,
    config_dest: &Path,
    prepared_config: &Path,
) -> Result<ResetPreparedStats> {
    let prepared = validate_prepared_bundle(bundle)?;
    let _operation_lock = crate::operation_lock::acquire_for_reset(&cfg.paths.db)?;
    crate::operation_lock::clear_ready(&cfg.paths.db)?;
    let db_parent = parent_dir_or_cwd(&cfg.paths.db);
    fs::create_dir_all(db_parent)
        .with_context(|| format!("create database parent {}", db_parent.display()))?;
    let db_work = tempfile::Builder::new()
        .prefix(".reset-demo-db-")
        .tempdir_in(db_parent)
        .context("create temporary demo database directory")?;
    // Keep the prepared account tree on the same mount as data_dir so
    // install can rename into data_dir/<account>. A work directory on
    // another mount (tmp, a nested bind, a named volume) fails with EXDEV.
    let data_work = reset_account_work_dir(&cfg.paths.data_dir)?;
    let prepared_db = db_work.path().join("vault.db");
    checkpoint_and_clean_sidecars(&cfg.paths.db, "before creating the reset snapshot").await?;
    prepare_database_snapshot(&cfg.paths.db, &prepared_db).await?;

    let mut temporary_cfg = cfg.clone();
    temporary_cfg.paths.db = prepared_db.clone();
    temporary_cfg.paths.data_dir = data_work.path().to_path_buf();
    let stats = rebuild_demo_account(
        &temporary_cfg,
        &prepared,
        account_id,
        DbTarget::Path(&prepared_db),
    )
    .await?;

    verify_non_demo_state_preserved(&cfg.paths.db, &prepared_db, account_id).await?;
    let active_account = cfg.paths.data_dir.join(account_id);
    let prepared_account = temporary_cfg.paths.data_dir.join(account_id);
    let paths = ResetPaths {
        active_db: &cfg.paths.db,
        prepared_db: &prepared_db,
        active_account: &active_account,
        prepared_account: &prepared_account,
        active_config: config_dest,
        prepared_config,
    };
    install_reset_state_or_keep_work(&paths, db_work, data_work).await?;
    crate::operation_lock::mark_ready(&cfg.paths.db)?;
    Ok(stats)
}

/// Swap the prepared state in. When the swap fails and its rollback left any
/// of the previous state in the work directories, keep those directories on
/// disk and name them in the error so nothing is lost.
async fn install_reset_state_or_keep_work(
    paths: &ResetPaths<'_>,
    db_work: tempfile::TempDir,
    data_work: tempfile::TempDir,
) -> Result<()> {
    let Err(error) = install_reset_state(paths).await else {
        return Ok(());
    };
    let config_backup = sqlite_sidecar(paths.prepared_config, ".previous-active");
    let previous_state_still_in_work = db_work.path().join("previous-vault.db").exists()
        || data_work.path().join("previous-account").exists()
        || config_backup.exists();
    if previous_state_still_in_work {
        let db_work = db_work.keep();
        let data_work = data_work.keep();
        return Err(error.context(format!(
            "reset-demo rollback was incomplete; temporary database and account state were kept at {} and {}",
            db_work.display(),
            data_work.display()
        )));
    }
    Err(error)
}

/// Wipe, seed, import, dedupe, convert media, and vacuum the demo account on
/// `target`. Both transports run exactly this; what differs is what `target`
/// names and what the caller does around it (the SQLite path snapshots the
/// database first and swaps it in after).
async fn rebuild_demo_account(
    cfg: &Config,
    prepared: &PreparedBundle,
    account_id: &str,
    target: DbTarget<'_>,
) -> Result<ResetPreparedStats> {
    wipe_demo_account(cfg, account_id, target).await?;
    print_reset_header(account_id, prepared, &target);
    seed_demo_account(target, account_id, &prepared.seed).await?;
    let import = import_demo_sources(cfg, prepared, account_id, target).await?;
    let (dedupe_stats, process_stats) = dedupe_and_process_assets(cfg, account_id, target).await?;
    vacuum_after_demo(target).await;
    Ok(ResetPreparedStats {
        import,
        dedupe_keys_filled: dedupe_stats.keys_filled,
        process_assets: process_stats,
    })
}

/// Import the staged sources in [`DEMO_IMPORT_SOURCES`] order: the first
/// replaces the account's data and the rest append. Returns the summed counts.
async fn import_demo_sources(
    cfg: &Config,
    prepared: &PreparedBundle,
    account_id: &str,
    target: DbTarget<'_>,
) -> Result<import::ImportStats> {
    let mut totals = import::ImportStats::default();
    for source in &DEMO_IMPORT_SOURCES {
        let assets_dir = cfg.paths.assets_dir_for_account(account_id, source.source);
        let stats = import::import_export(&ImportExportArgs {
            export_dir: (source.staging_dir)(prepared),
            db: target,
            assets_dir: &assets_dir,
            contacts: source
                .with_contacts
                .then_some(prepared.contacts_vcf.as_path()),
            overwrite_contacts: source.with_contacts,
            mode: source.mode,
            source: source.source,
            account_id,
        })
        .await?;
        totals.add_run(&stats);
    }
    Ok(totals)
}
/// Check the bundle has its seed, the three staging folders, and the contacts file, and return their paths.
fn validate_prepared_bundle(bundle: &Path) -> Result<PreparedBundle> {
    let demo_seed = bundle.join("config/seed.toml");
    let imessage_dir = bundle.join("staging").join(IMESSAGE_SOURCE);
    let sbr_dir = bundle.join("staging").join(SBR_SOURCE);
    let whatsapp_dir = bundle.join("staging").join(WHATSAPP_SOURCE);
    let contacts_vcf = bundle.join("config/contacts.vcf");
    if !demo_seed.is_file()
        || !imessage_dir.is_dir()
        || !sbr_dir.is_dir()
        || !whatsapp_dir.is_dir()
        || !contacts_vcf.is_file()
    {
        bail!(
            "incomplete demo bundle under {} (need config/seed.toml, \
             staging/{IMESSAGE_SOURCE}/, staging/{SBR_SOURCE}/, staging/{WHATSAPP_SOURCE}/, config/contacts.vcf)",
            bundle.display()
        );
    }
    Ok(PreparedBundle {
        seed: load_demo_seed(&demo_seed)?,
        imessage_dir,
        sbr_dir,
        whatsapp_dir,
        contacts_vcf,
    })
}

/// Copy the active database to the prepared path (checkpointing the WAL first) so the reset
/// works on a snapshot and the live file is untouched until the swap.
async fn prepare_database_snapshot(active: &Path, prepared: &Path) -> Result<()> {
    if active.is_file() {
        let pool = engine::open_pool_for_path(active)
            .await
            .with_context(|| format!("open {} for reset snapshot", active.display()))?;
        let mut conn = pool
            .acquire()
            .await
            .with_context(|| format!("open {} for reset snapshot", active.display()))?;
        sqlx::query("VACUUM INTO $1")
            .bind(prepared.to_string_lossy().as_ref())
            .execute(&mut *conn)
            .await
            .with_context(|| {
                format!(
                    "copy database snapshot {} to {}",
                    active.display(),
                    prepared.display()
                )
            })?;
        conn.close().await?;
        pool.close().await;
    } else {
        let pool = engine::open_pool_for_path(prepared)
            .await
            .with_context(|| format!("create prepared database {}", prepared.display()))?;
        let mut conn = pool
            .acquire()
            .await
            .with_context(|| format!("create prepared database {}", prepared.display()))?;
        schema::ensure_vault_schema(&mut conn).await?;
        conn.close().await?;
        pool.close().await;
    }
    Ok(())
}

/// Copy pending SQLite writes into the main database file, then remove the
/// write-ahead log (`-wal`) and shared-memory (`-shm`) sidecar files so the
/// database can be renamed safely.
async fn checkpoint_and_clean_sidecars(db: &Path, operation: &str) -> Result<()> {
    if !db.is_file() {
        return Ok(());
    }
    let pool = engine::open_pool_for_path(db)
        .await
        .with_context(|| format!("open {} {operation}", db.display()))?;
    let mut conn = pool
        .acquire()
        .await
        .with_context(|| format!("open {} {operation}", db.display()))?;
    let row = sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)")
        .fetch_one(&mut *conn)
        .await
        .with_context(|| {
            format!(
                "checkpoint SQLite write-ahead log for {} {operation}",
                db.display()
            )
        })?;
    let busy: i64 = row.try_get(0)?;
    let _log: i64 = row.try_get(1)?;
    let _checkpointed: i64 = row.try_get(2)?;
    // Close the connection deterministically: `pool.close()` only waits for
    // checked-out connections to be *returned*, and the sqlx worker thread
    // runs `sqlite3_close` later. A close that lands after the TRUNCATE
    // checkpoint can read the truncated `-shm` mapping and crash the process.
    conn.close().await?;
    // Close the pool so no connection stays attached to the database while
    // reset-demo replaces or renames it.
    pool.close().await;
    if busy != 0 {
        bail!(
            "cannot replace {} because its WAL could not be checkpointed; stop every process using the vault and run reset-demo offline",
            db.display()
        );
    }

    let wal = sqlite_sidecar(db, "-wal");
    if wal.exists() {
        let length = fs::metadata(&wal)
            .with_context(|| format!("inspect SQLite WAL {}", wal.display()))?
            .len();
        if length != 0 {
            bail!(
                "cannot replace {} because {} still contains {length} bytes after WAL checkpoint; stop every process using the vault and run reset-demo offline",
                db.display(),
                wal.display()
            );
        }
        fs::remove_file(&wal).with_context(|| {
            format!(
                "remove empty SQLite WAL {}; reset-demo requires offline database access",
                wal.display()
            )
        })?;
    }
    let shm = sqlite_sidecar(db, "-shm");
    if shm.exists() {
        fs::remove_file(&shm).with_context(|| {
            format!(
                "remove SQLite shared-memory sidecar {}; stop every process using the vault and run reset-demo offline",
                shm.display()
            )
        })?;
    }
    Ok(())
}

/// The `-wal` or `-shm` sidecar path next to a SQLite database file.
fn sqlite_sidecar(db: &Path, suffix: &str) -> PathBuf {
    let mut path: OsString = db.as_os_str().to_owned();
    path.push(suffix);
    PathBuf::from(path)
}

/// Refuse to install the prepared database if any non-demo account's row counts differ from
/// the active one: a reset must only ever touch the demo account.
async fn verify_non_demo_state_preserved(
    active: &Path,
    prepared: &Path,
    demo_id: &str,
) -> Result<()> {
    if !active.is_file() {
        return Ok(());
    }
    let active_state = non_demo_state(active, demo_id).await?;
    let prepared_state = non_demo_state(prepared, demo_id).await?;
    if active_state != prepared_state {
        bail!(
            "prepared reset database changed non-demo account state; active={active_state:?}, prepared={prepared_state:?}"
        );
    }
    Ok(())
}

/// Row counts per table for every account except the demo one, used to prove a reset changed nothing else.
async fn non_demo_state(db: &Path, demo_id: &str) -> Result<BTreeMap<String, i64>> {
    let pool = engine::open_pool_for_path(db)
        .await
        .with_context(|| format!("open {} to verify non-demo accounts", db.display()))?;
    let mut conn = pool
        .acquire()
        .await
        .with_context(|| format!("open {} to verify non-demo accounts", db.display()))?;
    let has_accounts: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'accounts'",
    )
    .fetch_one(&mut *conn)
    .await
    .with_context(|| format!("check accounts table in {}", db.display()))?;
    if has_accounts == 0 {
        conn.close().await?;
        pool.close().await;
        return Ok(BTreeMap::new());
    }
    let rows = sqlx::query(
        "SELECT a.id, COUNT(m.id)
         FROM accounts a
         LEFT JOIN messages m ON m.account_id = a.id
         WHERE a.id != $1
         GROUP BY a.id
         ORDER BY a.id",
    )
    .bind(demo_id)
    .fetch_all(&mut *conn)
    .await?;
    let mut state = BTreeMap::new();
    for row in rows {
        let account_id: String = row.try_get(0)?;
        let message_count: i64 = row.try_get(1)?;
        state.insert(account_id, message_count);
    }
    conn.close().await?;
    pool.close().await;
    Ok(state)
}

#[derive(Clone, Copy)]
struct ResetPaths<'a> {
    active_db: &'a Path,
    prepared_db: &'a Path,
    active_account: &'a Path,
    prepared_account: &'a Path,
    active_config: &'a Path,
    prepared_config: &'a Path,
}

/// Swap the prepared database, account folder, and config into their active paths.
async fn install_reset_state(paths: &ResetPaths<'_>) -> Result<()> {
    install_reset_state_with(paths, rename_prepared_path).await
}

/// [`install_reset_state`] with the rename step injected, so tests can simulate a rename that fails midway.
async fn install_reset_state_with<F>(paths: &ResetPaths<'_>, rename: F) -> Result<()>
where
    F: FnMut(&Path, &Path) -> Result<()>,
{
    checkpoint_and_clean_sidecars(paths.prepared_db, "before installing the prepared database")
        .await?;
    checkpoint_and_clean_sidecars(
        paths.active_db,
        "immediately before replacing the active database",
    )
    .await?;
    replace_reset_state_with(paths, rename)
}

/// The swap itself: move the active state to backups, move the prepared state in, and roll
/// the backups back if any step fails.
fn replace_reset_state_with<F>(paths: &ResetPaths<'_>, mut rename: F) -> Result<()>
where
    F: FnMut(&Path, &Path) -> Result<()>,
{
    let ResetPaths {
        active_db,
        prepared_db,
        active_account,
        prepared_account,
        active_config,
        prepared_config,
    } = *paths;
    if !prepared_db.is_file() || !prepared_account.is_dir() || !prepared_config.is_file() {
        bail!("prepared reset state is incomplete");
    }
    if let Some(parent) = active_account.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("create account data parent {}", parent.display()))?;
    }
    let db_backup = prepared_db
        .parent()
        .context("prepared database has no parent")?
        .join("previous-vault.db");
    let account_backup = prepared_account
        .parent()
        .context("prepared account has no parent")?
        .join("previous-account");
    let config_backup = sqlite_sidecar(prepared_config, ".previous-active");
    let had_db = active_db.exists();
    let had_account = active_account.exists();
    let had_config = active_config.exists();
    let mut installed_db = false;
    let mut installed_account = false;
    let mut installed_config = false;

    let replacement = (|| -> Result<()> {
        if had_db {
            rename(active_db, &db_backup).with_context(|| {
                format!("move existing database {} into backup", active_db.display())
            })?;
        }
        if had_account {
            rename(active_account, &account_backup).with_context(|| {
                format!(
                    "move existing account directory {} into backup",
                    active_account.display()
                )
            })?;
        }
        if had_config {
            rename(active_config, &config_backup).with_context(|| {
                format!(
                    "move existing config {} into backup",
                    active_config.display()
                )
            })?;
        }
        rename(prepared_db, active_db).with_context(|| {
            format!(
                "install prepared database {} at {}",
                prepared_db.display(),
                active_db.display()
            )
        })?;
        installed_db = true;
        rename(prepared_account, active_account).with_context(|| {
            format!(
                "install prepared account directory {} at {}",
                prepared_account.display(),
                active_account.display()
            )
        })?;
        installed_account = true;
        rename(prepared_config, active_config).with_context(|| {
            format!(
                "install prepared config {} at {}",
                prepared_config.display(),
                active_config.display()
            )
        })?;
        installed_config = true;
        Ok(())
    })();

    if let Err(error) = replacement {
        let mut rollback_errors = Vec::new();
        if installed_config && let Err(rollback_error) = remove_any_if_exists(active_config) {
            rollback_errors.push(format!(
                "remove installed config {}: {rollback_error:#}",
                active_config.display()
            ));
        }
        if installed_account && let Err(rollback_error) = remove_any_if_exists(active_account) {
            rollback_errors.push(format!(
                "remove installed account directory {}: {rollback_error:#}",
                active_account.display()
            ));
        }
        if installed_db
            && active_db.exists()
            && let Err(rollback_error) = remove_any_if_exists(active_db)
        {
            rollback_errors.push(format!(
                "remove installed database {}: {rollback_error:#}",
                active_db.display()
            ));
        }
        if had_config
            && config_backup.exists()
            && let Err(rollback_error) = rename(&config_backup, active_config)
        {
            rollback_errors.push(format!(
                "restore previous config {}: {rollback_error:#}",
                active_config.display()
            ));
        }
        if had_account
            && account_backup.exists()
            && let Err(rollback_error) = rename(&account_backup, active_account)
        {
            rollback_errors.push(format!(
                "restore previous account directory {}: {rollback_error:#}",
                active_account.display()
            ));
        }
        if had_db
            && db_backup.exists()
            && let Err(rollback_error) = rename(&db_backup, active_db)
        {
            rollback_errors.push(format!(
                "restore previous database {}: {rollback_error:#}",
                active_db.display()
            ));
        }
        if rollback_errors.is_empty() {
            cleanup_reset_backups(&db_backup, &account_backup, &config_backup);
            return Err(error.context("replace demo account state"));
        }
        return Err(anyhow::anyhow!(
            "replace demo account state: {error:#}; rollback incomplete; backups kept at {}, {}, and {}: {}",
            db_backup.display(),
            account_backup.display(),
            config_backup.display(),
            rollback_errors.join("; ")
        ));
    }

    cleanup_reset_backups(&db_backup, &account_backup, &config_backup);
    Ok(())
}

/// Remove the backups a successful swap left behind. Best effort: a leftover backup is reported, not fatal.
fn cleanup_reset_backups(db: &Path, account: &Path, config: &Path) {
    for backup in [db, account, config] {
        if let Err(error) = remove_any_if_exists(backup) {
            eprintln!(
                "warning: reset-demo installed or restored active state but could not remove backup {}: {error:#}",
                backup.display()
            );
        }
    }
}

/// Remove a file or folder tree; a missing path is not an error.
fn remove_any_if_exists(path: &Path) -> Result<()> {
    if path.is_dir() {
        fs::remove_dir_all(path).with_context(|| format!("remove {}", path.display()))?;
    } else if path.exists() {
        fs::remove_file(path).with_context(|| format!("remove {}", path.display()))?;
    }
    Ok(())
}

/// Rebuild the demo bundle from `demo_seed.toml` when that file is present
/// (a development checkout). Release images copy a generated staging/config
/// tree and omit `demo_seed.toml` — skip regeneration there.
///
/// Staging here is the temporary import area under the demo bundle
/// (`staging/imessage`, and so on).
fn maybe_regenerate_bundle(bundle: &Path) -> Result<demo_seed::GenStats> {
    let seed_toml = demo_seed::SeedConfig::default_path();
    if seed_toml.is_file() {
        println!(
            "Reset demo — regenerating bundle from {}",
            seed_toml.display()
        );
        return demo_seed::generate_to(bundle, None).context("regenerate demo bundle (demo-seed)");
    }

    let complete = bundle.join("config/seed.toml").is_file()
        && bundle.join("staging").join(IMESSAGE_SOURCE).is_dir()
        && bundle.join("staging").join(SBR_SOURCE).is_dir()
        && bundle.join("staging").join(WHATSAPP_SOURCE).is_dir()
        && bundle.join("config/contacts.vcf").is_file();
    if complete {
        println!(
            "Reset demo — using image bundle (no {} in this image)",
            seed_toml.display()
        );
        return Ok(demo_seed::GenStats {
            contacts: 0,
            conversation_files: 0,
            messages: 0,
            attachment_refs: 0,
            groups: 0,
        });
    }

    bail!(
        "cannot reset demo: {} is missing and {} is not a complete demo bundle \
         (need staging/{IMESSAGE_SOURCE}/, staging/{SBR_SOURCE}/, and staging/{WHATSAPP_SOURCE}/)",
        seed_toml.display(),
        bundle.display()
    );
}

/// Compact import tables after the sample inbox is fully loaded. Best effort:
/// failures are printed, not returned, because the demo rows are already committed.
async fn vacuum_after_demo(target: DbTarget<'_>) {
    let pool = match target.open().await {
        Ok(pool) => pool,
        Err(err) => {
            eprintln!("  sql:      warning: vacuum after demo failed to open the database: {err}");
            return;
        }
    };
    vacuum_after_demo_on_pool(pool).await;
}
/// Reclaim space after the demo import replaced most rows. Best effort: a failed vacuum only costs disk space.
async fn vacuum_after_demo_on_pool(pool: sqlx::AnyPool) {
    let mut conn = match pool.acquire().await {
        Ok(conn) => conn,
        Err(err) => {
            eprintln!("  sql:      warning: vacuum after demo failed to open a connection: {err}");
            pool.close().await;
            return;
        }
    };
    dialect::vacuum_import_tables(&mut conn).await;
    // Close deterministically before reset-demo renames the SQLite file.
    // `pool.close()` only waits for the connection to be returned; the
    // sqlx worker runs sqlite3_close later.
    if let Err(err) = conn.close().await {
        eprintln!("  sql:      warning: vacuum after demo failed to close the connection: {err}");
    }
    pool.close().await;
}

/// Parse `config/seed.toml` from the bundle.
fn load_demo_seed(path: &Path) -> Result<DemoSeed> {
    let text = fs::read_to_string(path)
        .with_context(|| format!("failed to read demo seed {}", path.display()))?;
    toml::from_str(&text).with_context(|| format!("failed to parse demo seed {}", path.display()))
}

/// Open the target database and seed the demo account row and profile.
async fn seed_demo_account(target: DbTarget<'_>, account_id: &str, seed: &DemoSeed) -> Result<()> {
    let pool = target.open().await?;
    let mut conn = pool.acquire().await?;
    schema::ensure_vault_schema(&mut conn).await?;
    seed_demo_account_on_conn(&mut conn, account_id, seed).await?;
    conn.close().await?;
    pool.close().await;
    Ok(())
}
/// Create the demo account row and the profile fields the seed names, so the demo signs in without setup.
async fn seed_demo_account_on_conn(
    conn: &mut sqlx::AnyConnection,
    account_id: &str,
    seed: &DemoSeed,
) -> Result<()> {
    account_profile::ensure_account_row(conn, account_id).await?;

    // The demo account exists so someone can try the whole vault without
    // making an account of their own, so it may import, export, and delete
    // like any other account. Reading a conversation goes through the export
    // route, so a demo account without export cannot open a single thread.
    sqlx::query(
        r#"
        INSERT INTO accounts (
            id, username, password_hash, preferred_name, can_import, can_export, can_delete
        )
        VALUES ($1, $2, NULL, $3, 1, 1, 1)
        ON CONFLICT(id) DO UPDATE SET
            username = excluded.username,
            preferred_name = excluded.preferred_name,
            can_import = excluded.can_import,
            can_export = excluded.can_export,
            can_delete = excluded.can_delete
        "#,
    )
    .bind(account_id)
    .bind(&seed.account.username)
    .bind(&seed.owner.display_name)
    .execute(&mut *conn)
    .await?;
    // Extra email addresses used only to recognize "you" in messages, not for login.
    sqlx::query("DELETE FROM account_emails WHERE account_id = $1")
        .bind(account_id)
        .execute(&mut *conn)
        .await?;
    for email in &seed.owner.emails {
        sqlx::query(
            r#"
            INSERT INTO account_emails (account_id, email, is_primary)
            VALUES ($1, $2, 0)
            ON CONFLICT DO NOTHING
            "#,
        )
        .bind(account_id)
        .bind(email)
        .execute(&mut *conn)
        .await?;
    }
    // Demo has no Import API token until the user generates one in Settings.

    // Phone and email identities that mark messages as from "you" live in
    // `handles`, linked through `account_handles`.
    sqlx::query("DELETE FROM account_handles WHERE account_id = $1")
        .bind(account_id)
        .execute(&mut *conn)
        .await?;
    for (raw, handle_type) in &seed.owner.handle_specs {
        account_profile::link_account_handle(conn, account_id, raw, *handle_type).await?;
    }
    Ok(())
}

/// Delete the demo account's vault rows (child rows follow via CASCADE) and
/// on-disk attachments. Leaves the database and other accounts intact.
async fn wipe_demo_account(cfg: &Config, account_id: &str, target: DbTarget<'_>) -> Result<()> {
    println!("Reset demo — clearing account data in {target}");
    let pool = target.open().await?;
    let mut conn = pool
        .acquire()
        .await
        .with_context(|| format!("open {target} for demo account wipe"))?;
    schema::ensure_vault_schema(&mut conn).await?;
    let deleted = sqlx::query("DELETE FROM accounts WHERE id = $1")
        .bind(account_id)
        .execute(&mut *conn)
        .await
        .with_context(|| format!("delete account {account_id}"))?
        .rows_affected();
    println!("  sql:      demo account rows removed (accounts matched={deleted})");
    conn.close().await?;
    pool.close().await;

    let account_root = cfg.paths.data_dir.join(account_id);
    remove_tree_if_exists(&account_root)?;
    Ok(())
}
/// Remove a folder tree; a missing folder is not an error.
fn remove_tree_if_exists(path: &Path) -> Result<()> {
    if path.exists() {
        fs::remove_dir_all(path).with_context(|| format!("remove {}", path.display()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::PathsConfig;
    use sqlx::AnyConnection;

    fn url_config_for_refuse_tests() -> Config {
        Config {
            paths: PathsConfig {
                db: PathBuf::from("data/vault.db"),
                data_dir: PathBuf::from("data"),
                assets_dir: "assets".into(),
                assets_converted_dir: "assets_converted".into(),
            },
            server: None,
            database: crate::config::DatabaseConfig {
                url: Some("postgres://vault:vault@127.0.0.1:5432/vault".into()),
            },
        }
    }

    #[test]
    fn refuse_url_config_errors_when_config_has_url() {
        let err = refuse_url_config(&url_config_for_refuse_tests())
            .expect_err("config URL without --db-url must fail");
        assert!(
            err.to_string()
                .contains("URL-served databases cannot be reset"),
            "{err}"
        );
    }

    fn write_tiny_reset_bundle(root: &Path) {
        fs::create_dir_all(root.join("config")).expect("create bundle config");
        fs::create_dir_all(root.join("staging").join(IMESSAGE_SOURCE)).expect("imessage dir");
        fs::create_dir_all(root.join("staging").join(SBR_SOURCE)).expect("sbr dir");
        fs::create_dir_all(root.join("staging").join(WHATSAPP_SOURCE)).expect("whatsapp dir");
        fs::write(
            root.join("config/config.toml"),
            "[paths]\ndb = \"data/vault.db\"\ndata_dir = \"data\"\n",
        )
        .expect("write bundle config");
        fs::write(
            root.join("config/seed.toml"),
            r#"
[owner]
display_name = "Demo User"
handle_specs = [["+14155559000", "phone"]]
emails = ["demo.ingest@example.com"]

[account]
username = "demo"
"#,
        )
        .expect("write seed.toml");
        fs::write(
            root.join("config/contacts.vcf"),
            "BEGIN:VCARD\nVERSION:3.0\nFN:Test\nTEL:+15555550100\nEND:VCARD\n",
        )
        .expect("write contacts");
        let conversation = |source: &str, chat: &str, guid: &str| {
            format!(
                r#"{{"schema_version":4,"export":{{"source":"{source}","tool":"t","tool_version":"0","owner_handle":null,"owner_display_name":null}},"conversation":{{"chat_identifier":"{chat}","conversation_type":"individual","group_title":null,"participants":[{{"handle":"{chat}","display_name":null}}],"stats":{{"message_count":1,"attachment_count":0,"first_timestamp_unix_ms":1426183462000,"last_timestamp_unix_ms":1426183462000}}}}}}
{{"guid":"{guid}","timestamp_unix_ms":1426183462000,"direction":"incoming","service":"sms","message_kind":"sms","sender_handle":"{chat}","sender_display_name":null,"subject":null,"text":"hello","attachments":[],"imessage":null,"source":null}}
"#
            )
        };
        fs::write(
            root.join("staging").join(IMESSAGE_SOURCE).join("a.jsonl"),
            conversation(IMESSAGE_SOURCE, "+15555550101", "pg-demo-imessage"),
        )
        .expect("write imessage jsonl");
        fs::write(
            root.join("staging").join(SBR_SOURCE).join("a.jsonl"),
            conversation(SBR_SOURCE, "+15555550102", "pg-demo-sbr"),
        )
        .expect("write sbr jsonl");
        fs::write(
            root.join("staging").join(WHATSAPP_SOURCE).join("a.jsonl"),
            conversation(WHATSAPP_SOURCE, "+15555550103", "pg-demo-wa"),
        )
        .expect("write whatsapp jsonl");
    }

    #[tokio::test]
    async fn reset_demo_db_url_creates_demo_account_on_postgres() {
        let Some(url) = crate::pg_test_url() else {
            return;
        };
        let _pg_guard = crate::acquire_pg_test_lock().await;
        sqlx::any::install_default_drivers();

        let temp = tempfile::tempdir().expect("temp dir");
        let bundle = temp.path().join("bundle");
        write_tiny_reset_bundle(&bundle);
        let data_dir = temp.path().join("data");
        fs::create_dir_all(&data_dir).expect("data dir");
        let unused_db = temp.path().join("unused.db");
        let config_dest = temp.path().join("config.toml");
        fs::write(
            &config_dest,
            format!(
                "[paths]\ndb = \"{}\"\ndata_dir = \"{}\"\n",
                unused_db.display(),
                data_dir.display()
            ),
        )
        .expect("write host config");

        let pool = engine::open_pool_from_url(&url)
            .await
            .expect("open postgres");
        let mut conn = pool.acquire().await.expect("acquire");
        schema::ensure_vault_schema(&mut conn)
            .await
            .expect("schema");
        sqlx::query("DELETE FROM accounts WHERE id = $1")
            .bind(DEMO_ACCOUNT_ID)
            .execute(&mut *conn)
            .await
            .expect("wipe leftover demo");
        conn.close().await.expect("close wipe conn");
        pool.close().await;

        let host_config_before = fs::read(&config_dest).expect("read host config");
        let cfg = Config::load(&config_dest).expect("load host config");
        reset_prepared_bundle_at_url(&cfg, &bundle, DEMO_ACCOUNT_ID, &url)
            .await
            .expect("reset at url");
        assert!(
            !unused_db.exists(),
            "reset-demo --db-url must not create or replace paths.db"
        );
        assert_eq!(
            fs::read(&config_dest).expect("reread host config"),
            host_config_before,
            "reset-demo --db-url must leave the host config file unchanged"
        );

        let pool = engine::open_pool_from_url(&url)
            .await
            .expect("reopen postgres");
        let mut conn = pool.acquire().await.expect("acquire");
        let username: Option<String> =
            sqlx::query_scalar("SELECT username FROM accounts WHERE id = $1")
                .bind(DEMO_ACCOUNT_ID)
                .fetch_optional(&mut *conn)
                .await
                .expect("username");
        assert_eq!(username.as_deref(), Some("demo"));
        let hash: Option<String> =
            sqlx::query_scalar("SELECT password_hash FROM accounts WHERE id = $1")
                .bind(DEMO_ACCOUNT_ID)
                .fetch_one(&mut *conn)
                .await
                .expect("password hash");
        assert!(hash.is_none(), "demo account must have no password hash");
        let conversations: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM conversations WHERE account_id = $1")
                .bind(DEMO_ACCOUNT_ID)
                .fetch_one(&mut *conn)
                .await
                .expect("conversations");
        assert!(conversations >= 1, "expected imported conversations");
        sqlx::query("DELETE FROM accounts WHERE id = $1")
            .bind(DEMO_ACCOUNT_ID)
            .execute(&mut *conn)
            .await
            .expect("cleanup demo");
        conn.close().await.expect("close");
        pool.close().await;
    }

    /// Open `db` with the vault schema applied and one connection checked out.
    async fn test_db_conn(db: &Path) -> sqlx::pool::PoolConnection<sqlx::Any> {
        let (_pool, conn) = test_db(db).await;
        conn
    }

    /// Open `db` with the vault schema applied; returns the pool alongside the
    /// connection so the caller can close the pool deterministically before
    /// copying or replacing the database file.
    async fn test_db(db: &Path) -> (sqlx::AnyPool, sqlx::pool::PoolConnection<sqlx::Any>) {
        sqlx::any::install_default_drivers();
        let pool = engine::open_pool_for_path(db)
            .await
            .expect("open test database");
        let mut conn = pool.acquire().await.expect("acquire test connection");
        schema::ensure_vault_schema(&mut conn)
            .await
            .expect("create vault schema");
        (pool, conn)
    }

    /// Close the pool so no connection stays attached to the database file.
    async fn close_test_db(pool: sqlx::AnyPool, conn: sqlx::pool::PoolConnection<sqlx::Any>) {
        // Await the real close: `pool.close()` alone only waits for the
        // connection to be returned, and the sqlx worker thread closes it
        // later — racing the checkpoint/copy that follows can SIGBUS.
        conn.close().await.expect("close test connection");
        pool.close().await;
    }

    /// The committed demo bundle ships a `seed.toml`; it must parse with the
    /// current `DemoOwner` (handle_specs) format or `reset-demo` fails on
    /// release images that skip bundle regeneration.
    #[test]
    fn committed_demo_seed_toml_parses() {
        let text = include_str!("../../demo-seed/config/seed.toml");
        let seed: DemoSeed = toml::from_str(text).expect("committed demo seed.toml must parse");
        assert_eq!(seed.owner.display_name, "Demo User");
        assert_eq!(seed.owner.handle_specs.len(), 1);
        let (raw, handle_type) = &seed.owner.handle_specs[0];
        assert_eq!(raw, "+14155559000");
        assert_eq!(*handle_type, HandleType::Phone);
        assert_eq!(seed.owner.emails, vec!["demo.ingest@example.com"]);
        assert_eq!(seed.account.username, "demo");
    }

    #[tokio::test]
    async fn the_demo_account_may_import_export_and_delete() {
        let temp = tempfile::tempdir().expect("create test directory");
        let db = temp.path().join("vault.db");
        let (pool, mut conn) = test_db(&db).await;
        let seed = DemoSeed {
            owner: DemoOwner {
                display_name: "Demo User".into(),
                handle_specs: Vec::new(),
                emails: Vec::new(),
            },
            account: DemoAccount {
                username: "demo".into(),
            },
        };

        seed_demo_account_on_conn(&mut conn, DEMO_ACCOUNT_ID, &seed)
            .await
            .expect("seed the demo account");

        let (import, export, delete): (i64, i64, i64) =
            sqlx::query_as("SELECT can_import, can_export, can_delete FROM accounts WHERE id = $1")
                .bind(DEMO_ACCOUNT_ID)
                .fetch_one(&mut *conn)
                .await
                .expect("read the demo account");
        assert_eq!(
            (import, export, delete),
            (1, 1, 1),
            "the demo account is there to try the whole vault, so it may import, export, and delete"
        );

        close_test_db(pool, conn).await;
    }

    #[tokio::test]
    async fn failed_reset_preserves_existing_demo_account() {
        let temp = tempfile::tempdir().expect("create test directory");
        let db = temp.path().join("vault.db");
        let data_dir = temp.path().join("data");
        let account_root = data_dir.join(DEMO_ACCOUNT_ID);
        fs::create_dir_all(&account_root).expect("create account data directory");
        let sentinel = account_root.join("existing.bin");
        let original_data = b"existing account data\n";
        fs::write(&sentinel, original_data).expect("write account data sentinel");

        {
            let (pool, mut conn) = test_db(&db).await;
            account_profile::ensure_account_row(&mut conn, DEMO_ACCOUNT_ID)
                .await
                .expect("seed account");
            let handle_id: i64 = sqlx::query_scalar(
                "INSERT INTO handles (
                    account_id, raw, normalized, handle_type, service
                 ) VALUES ($1, '+15555550100', '+15555550100', 'phone', 'phone')
                 RETURNING id",
            )
            .bind(DEMO_ACCOUNT_ID)
            .fetch_one(&mut *conn)
            .await
            .expect("insert handle");
            let conversation_id: i64 = sqlx::query_scalar(
                "INSERT INTO conversations (
                    account_id, chat_handle_id, conversation_type, source_file
                 ) VALUES ($1, $2, 'individual', 'existing.jsonl')
                 RETURNING id",
            )
            .bind(DEMO_ACCOUNT_ID)
            .bind(handle_id)
            .fetch_one(&mut *conn)
            .await
            .expect("insert conversation");
            sqlx::query(
                "INSERT INTO messages (
                    conversation_id, account_id, source, guid, timestamp,
                    is_from_me, body, sort_order
                 ) VALUES ($1, $2, 'imessage', 'existing-message',
                           '2026-01-01T00:00:00Z', 0, 'keep me', 0)",
            )
            .bind(conversation_id)
            .bind(DEMO_ACCOUNT_ID)
            .execute(&mut *conn)
            .await
            .expect("insert message");
            close_test_db(pool, conn).await;
        }

        let cfg = Config {
            paths: PathsConfig {
                db: db.clone(),
                data_dir,
                assets_dir: "assets".into(),
                assets_converted_dir: "assets_converted".into(),
            },
            server: None,
            database: crate::config::DatabaseConfig::default(),
        };
        let invalid_bundle = temp.path().join("invalid-bundle");
        fs::create_dir_all(invalid_bundle.join("staging").join(IMESSAGE_SOURCE))
            .expect("create iMessage tree");
        fs::create_dir_all(invalid_bundle.join("staging").join(SBR_SOURCE))
            .expect("create Android tree");

        let result = reset_prepared_bundle(
            &cfg,
            &invalid_bundle,
            DEMO_ACCOUNT_ID,
            &temp.path().join("config/config.toml"),
            &temp.path().join("prepared-config.toml"),
        )
        .await;

        assert!(result.is_err());
        let mut conn = test_db_conn(&db).await;
        let account_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM accounts WHERE id = $1")
            .bind(DEMO_ACCOUNT_ID)
            .fetch_one(&mut *conn)
            .await
            .expect("count account");
        let message_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM messages WHERE guid = 'existing-message'")
                .fetch_one(&mut *conn)
                .await
                .expect("count message");
        assert_eq!(account_count, 1);
        assert_eq!(message_count, 1);
        assert_eq!(
            fs::read(&sentinel).expect("read account sentinel"),
            original_data
        );
    }

    #[tokio::test]
    async fn failed_preparation_preserves_active_config() {
        let temp = tempfile::tempdir().expect("create test directory");
        let config_dest = temp.path().join("config/config.toml");
        fs::create_dir_all(config_dest.parent().expect("config parent"))
            .expect("create config parent");
        let original = b"active configuration\n";
        fs::write(&config_dest, original).expect("write active config");
        let invalid_bundle = temp.path().join("invalid-bundle");
        fs::create_dir_all(&invalid_bundle).expect("create invalid bundle");

        let result =
            prepare_config_and_reset(&invalid_bundle, &config_dest, DEMO_ACCOUNT_ID, None).await;

        assert!(result.is_err());
        assert_eq!(
            fs::read(&config_dest).expect("read active config"),
            original
        );
    }

    #[tokio::test]
    async fn vault_db_without_accounts_table_does_not_block_reset_check() {
        let temp = tempfile::tempdir().expect("create test directory");
        let active = temp.path().join("vault.db");
        fs::write(&active, []).expect("create empty sqlite file");
        let prepared = temp.path().join("prepared.db");
        drop(test_db_conn(&prepared).await);

        verify_non_demo_state_preserved(&active, &prepared, DEMO_ACCOUNT_ID)
            .await
            .expect("a vault.db with no accounts table must not block reset-demo");
    }

    #[test]
    fn reset_refuses_while_server_holds_database_lock() {
        let temp = tempfile::tempdir().expect("create test directory");
        let db = temp.path().join("vault.db");
        let _serve_lock =
            crate::operation_lock::acquire_for_serve(&db).expect("acquire server lock");

        let error = crate::operation_lock::acquire_for_reset(&db)
            .expect_err("reset lock must conflict with active server")
            .to_string();

        assert!(error.contains("serve is active"), "{error}");
        assert!(error.contains("offline"), "{error}");
    }

    #[tokio::test]
    async fn failures_after_database_and_account_install_restore_all_active_state() {
        for failure_point in [
            ResetInstallFailure::AfterDatabase,
            ResetInstallFailure::AfterAccount,
        ] {
            let temp = tempfile::tempdir().expect("create test directory");
            let active_db = temp.path().join("active/vault.db");
            fs::create_dir_all(active_db.parent().expect("database parent"))
                .expect("create database parent");
            seed_reset_test_database(&active_db).await;
            let prepared_db = temp.path().join("prepared/vault.db");
            fs::create_dir_all(prepared_db.parent().expect("prepared database parent"))
                .expect("create prepared database parent");
            fs::copy(&active_db, &prepared_db).expect("copy prepared database");
            make_prepared_reset_database_observably_different(&prepared_db).await;

            let active_account = temp.path().join("data").join(DEMO_ACCOUNT_ID);
            let prepared_account = temp.path().join("prepared-data").join(DEMO_ACCOUNT_ID);
            fs::create_dir_all(&active_account).expect("create active account");
            fs::create_dir_all(&prepared_account).expect("create prepared account");
            fs::write(active_account.join("sentinel"), b"old data").expect("write old data");
            fs::write(prepared_account.join("sentinel"), b"new data").expect("write new data");

            let active_config = temp.path().join("config/config.toml");
            let prepared_config = temp.path().join("prepared-config/config.toml");
            fs::create_dir_all(active_config.parent().expect("active config parent"))
                .expect("create active config parent");
            fs::create_dir_all(prepared_config.parent().expect("prepared config parent"))
                .expect("create prepared config parent");
            fs::write(&active_config, b"old config").expect("write old config");
            fs::write(&prepared_config, b"new config").expect("write new config");

            let result = replace_reset_state_with(
                &ResetPaths {
                    active_db: &active_db,
                    prepared_db: &prepared_db,
                    active_account: &active_account,
                    prepared_account: &prepared_account,
                    active_config: &active_config,
                    prepared_config: &prepared_config,
                },
                |source, destination| {
                    if failure_point == ResetInstallFailure::AfterDatabase
                        && source == prepared_account
                    {
                        bail!("injected failure after database rename");
                    }
                    if failure_point == ResetInstallFailure::AfterAccount
                        && source == prepared_config
                    {
                        bail!("injected failure after account-directory rename");
                    }
                    fs::rename(source, destination).map_err(Into::into)
                },
            );

            assert!(result.is_err());
            assert_reset_test_database(&active_db).await;
            assert_eq!(
                fs::read(active_account.join("sentinel")).expect("read data sentinel"),
                b"old data"
            );
            assert_eq!(
                fs::read(&active_config).expect("read active config"),
                b"old config"
            );
        }
    }

    #[tokio::test]
    async fn active_sidecars_are_cleaned_immediately_before_database_rename() {
        let temp = tempfile::tempdir().expect("create test directory");
        let active_db = temp.path().join("active/vault.db");
        fs::create_dir_all(active_db.parent().expect("database parent"))
            .expect("create database parent");
        seed_reset_test_database(&active_db).await;
        let prepared_db = temp.path().join("prepared/vault.db");
        fs::create_dir_all(prepared_db.parent().expect("prepared database parent"))
            .expect("create prepared database parent");
        fs::copy(&active_db, &prepared_db).expect("copy prepared database");

        let active_account = temp.path().join("data").join(DEMO_ACCOUNT_ID);
        let prepared_account = temp.path().join("prepared-data").join(DEMO_ACCOUNT_ID);
        fs::create_dir_all(&active_account).expect("create active account");
        fs::create_dir_all(&prepared_account).expect("create prepared account");
        let active_config = temp.path().join("config/config.toml");
        let prepared_config = temp.path().join("prepared-config/config.toml");
        fs::create_dir_all(active_config.parent().expect("active config parent"))
            .expect("create active config parent");
        fs::create_dir_all(prepared_config.parent().expect("prepared config parent"))
            .expect("create prepared config parent");
        fs::write(&active_config, b"old config").expect("write active config");
        fs::write(&prepared_config, b"new config").expect("write prepared config");

        {
            let (pool, mut conn) = test_db(&active_db).await;
            sqlx::query("UPDATE accounts SET preferred_name = 'reopened' WHERE id = $1")
                .bind(DEMO_ACCOUNT_ID)
                .execute(&mut *conn)
                .await
                .expect("write through reopened active database");
            close_test_db(pool, conn).await;
        }
        let active_wal = sqlite_sidecar(&active_db, "-wal");
        let active_shm = sqlite_sidecar(&active_db, "-shm");
        fs::write(&active_wal, b"").expect("create empty WAL sidecar");
        fs::write(&active_shm, b"").expect("create empty shared-memory sidecar");
        let mut observed_clean_boundary = false;

        let result = install_reset_state_with(
            &ResetPaths {
                active_db: &active_db,
                prepared_db: &prepared_db,
                active_account: &active_account,
                prepared_account: &prepared_account,
                active_config: &active_config,
                prepared_config: &prepared_config,
            },
            |source, destination| {
                if source == active_db {
                    observed_clean_boundary = !active_wal.exists() && !active_shm.exists();
                }
                if source == prepared_db {
                    bail!("stop after observing active database rename boundary");
                }
                fs::rename(source, destination).map_err(Into::into)
            },
        )
        .await;

        assert!(result.is_err());
        assert!(
            observed_clean_boundary,
            "active WAL and shared-memory sidecars must be absent at rename"
        );
    }

    #[test]
    fn reset_rollback_attempts_remaining_restorations_after_one_fails() {
        let temp = tempfile::tempdir().expect("create test directory");
        let active_db = temp.path().join("active/vault.db");
        let prepared_db = temp.path().join("prepared/vault.db");
        let active_account = temp.path().join("data/demo");
        let prepared_account = temp.path().join("prepared-data/demo");
        let active_config = temp.path().join("config/config.toml");
        let prepared_config = temp.path().join("prepared-config/config.toml");
        for parent in [
            active_db.parent().expect("active db parent"),
            prepared_db.parent().expect("prepared db parent"),
            &active_account,
            &prepared_account,
            active_config.parent().expect("active config parent"),
            prepared_config.parent().expect("prepared config parent"),
        ] {
            fs::create_dir_all(parent).expect("create replacement fixture directory");
        }
        fs::write(&active_db, b"old db").expect("write active db");
        fs::write(&prepared_db, b"new db").expect("write prepared db");
        fs::write(active_account.join("sentinel"), b"old").expect("write active account");
        fs::write(prepared_account.join("sentinel"), b"new").expect("write prepared account");
        fs::write(&active_config, b"old config").expect("write active config");
        fs::write(&prepared_config, b"new config").expect("write prepared config");
        let mut database_restore_attempted = false;

        let result = replace_reset_state_with(
            &ResetPaths {
                active_db: &active_db,
                prepared_db: &prepared_db,
                active_account: &active_account,
                prepared_account: &prepared_account,
                active_config: &active_config,
                prepared_config: &prepared_config,
            },
            |source, destination| {
                if source == prepared_config {
                    bail!("injected config install failure");
                }
                if source.ends_with("previous-account") {
                    bail!("injected account restore failure");
                }
                if source.ends_with("previous-vault.db") {
                    database_restore_attempted = true;
                }
                fs::rename(source, destination).map_err(Into::into)
            },
        );

        let error = result.expect_err("replacement must fail").to_string();
        assert!(
            database_restore_attempted,
            "database restoration must be attempted after account restoration fails"
        );
        assert!(error.contains("injected account restore failure"));
        assert!(
            prepared_account
                .parent()
                .unwrap()
                .join("previous-account")
                .exists()
        );
    }

    #[derive(Clone, Copy, PartialEq, Eq)]
    enum ResetInstallFailure {
        AfterDatabase,
        AfterAccount,
    }

    async fn seed_reset_test_database(path: &Path) {
        let (pool, mut conn) = test_db(path).await;
        schema::ensure_vault_schema(&mut conn)
            .await
            .expect("create reset test schema");
        seed_reset_test_account(&mut conn, DEMO_ACCOUNT_ID, "demo-existing").await;
        seed_reset_test_account(&mut conn, "non-demo-account", "non-demo-existing").await;
        close_test_db(pool, conn).await;
        // Pool close does not reliably checkpoint WAL sidecars, so an
        // fs::copy of this file would miss everything written to the -wal.
        // Checkpoint explicitly so copies see the seeded rows.
        checkpoint_and_clean_sidecars(path, "while seeding reset test database")
            .await
            .expect("checkpoint seeded reset test database");
    }

    async fn make_prepared_reset_database_observably_different(path: &Path) {
        let (pool, mut conn) = test_db(path).await;
        sqlx::query("UPDATE accounts SET username = 'prepared-demo' WHERE id = $1")
            .bind(DEMO_ACCOUNT_ID)
            .execute(&mut *conn)
            .await
            .expect("change prepared demo account");
        sqlx::query("DELETE FROM messages WHERE account_id = $1")
            .bind(DEMO_ACCOUNT_ID)
            .execute(&mut *conn)
            .await
            .expect("delete prepared demo message");
        sqlx::query("DELETE FROM accounts WHERE id = 'non-demo-account'")
            .execute(&mut *conn)
            .await
            .expect("delete prepared non-demo marker");
        close_test_db(pool, conn).await;
        checkpoint_and_clean_sidecars(path, "while preparing reset test database")
            .await
            .expect("checkpoint prepared reset test database");

        let (pool, mut conn) = test_db(path).await;
        let demo_username: String =
            sqlx::query_scalar("SELECT username FROM accounts WHERE id = $1")
                .bind(DEMO_ACCOUNT_ID)
                .fetch_one(&mut *conn)
                .await
                .expect("read changed prepared demo account");
        let demo_messages: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM messages WHERE account_id = $1")
                .bind(DEMO_ACCOUNT_ID)
                .fetch_one(&mut *conn)
                .await
                .expect("count prepared demo messages");
        let non_demo_accounts: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM accounts WHERE id = 'non-demo-account'")
                .fetch_one(&mut *conn)
                .await
                .expect("count prepared non-demo marker");
        assert_eq!(demo_username, "prepared-demo");
        assert_eq!(demo_messages, 0);
        assert_eq!(non_demo_accounts, 0);
        close_test_db(pool, conn).await;
    }

    async fn seed_reset_test_account(conn: &mut AnyConnection, account_id: &str, guid: &str) {
        account_profile::ensure_account_row(conn, account_id)
            .await
            .expect("seed reset test account");
        let handle_id: i64 = sqlx::query_scalar(
            "INSERT INTO handles (
                account_id, raw, normalized, handle_type, service
             ) VALUES ($1, $2, $2, 'username', 'phone')
             RETURNING id",
        )
        .bind(account_id)
        .bind(format!("{account_id}-handle"))
        .fetch_one(&mut *conn)
        .await
        .expect("insert reset test handle");
        let conversation_id: i64 = sqlx::query_scalar(
            "INSERT INTO conversations (
                account_id, chat_handle_id, conversation_type, source_file
             ) VALUES ($1, $2, 'individual', 'existing.jsonl')
             RETURNING id",
        )
        .bind(account_id)
        .bind(handle_id)
        .fetch_one(&mut *conn)
        .await
        .expect("insert reset test conversation");
        sqlx::query(
            "INSERT INTO messages (
                conversation_id, account_id, source, guid, timestamp,
                is_from_me, body, sort_order
             ) VALUES ($1, $2, 'imessage', $3,
                       '2026-01-01T00:00:00Z', 0, 'keep me', 0)",
        )
        .bind(conversation_id)
        .bind(account_id)
        .bind(guid)
        .execute(&mut *conn)
        .await
        .expect("insert reset test message");
    }

    async fn assert_reset_test_database(path: &Path) {
        let (pool, mut conn) = test_db(path).await;
        for (account_id, guid) in [
            (DEMO_ACCOUNT_ID, "demo-existing"),
            ("non-demo-account", "non-demo-existing"),
        ] {
            let account_count: i64 =
                sqlx::query_scalar("SELECT COUNT(*) FROM accounts WHERE id = $1")
                    .bind(account_id)
                    .fetch_one(&mut *conn)
                    .await
                    .expect("count restored account");
            let username: String =
                sqlx::query_scalar("SELECT username FROM accounts WHERE id = $1")
                    .bind(account_id)
                    .fetch_one(&mut *conn)
                    .await
                    .expect("read restored username");
            let (message_count, body): (i64, String) = sqlx::query_as(
                "SELECT COUNT(*), MIN(body)
                 FROM messages WHERE account_id = $1 AND guid = $2",
            )
            .bind(account_id)
            .bind(guid)
            .fetch_one(&mut *conn)
            .await
            .expect("count restored message");
            assert_eq!(account_count, 1, "account {account_id}");
            assert_eq!(username, account_id, "username {account_id}");
            assert_eq!(message_count, 1, "message {guid}");
            assert_eq!(body, "keep me", "message body {guid}");
        }
        close_test_db(pool, conn).await;
    }
}
