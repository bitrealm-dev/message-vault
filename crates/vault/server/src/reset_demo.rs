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
use crate::db::engine;
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
    read_only: bool,
}

struct PreparedBundle {
    seed: DemoSeed,
    imessage_dir: PathBuf,
    sbr_dir: PathBuf,
    whatsapp_dir: PathBuf,
    contacts_vcf: PathBuf,
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
    run_reset_demo_for_account(bundle, config_dest, DEMO_ACCOUNT_ID, db_url).await
}

async fn run_reset_demo_for_account(
    bundle: &Path,
    config_dest: &Path,
    account_id: &str,
    db_url: Option<&str>,
) -> Result<ResetDemoStats> {
    let bundle = if bundle.is_absolute() {
        bundle.to_path_buf()
    } else {
        std::env::current_dir()?.join(bundle)
    };

    println!("  bundle:       {}", bundle.display());
    let seed_stats = maybe_regenerate_bundle(&bundle)?;
    let reset_stats = prepare_config_and_reset(&bundle, config_dest, account_id, db_url).await?;

    Ok(ResetDemoStats {
        seed: seed_stats,
        import: reset_stats.import,
        dedupe_keys_filled: reset_stats.dedupe_keys_filled,
        process_assets: reset_stats.process_assets,
    })
}

/// Refuse a config that serves the database from a URL unless `--db-url` was passed.
fn refuse_url_config_without_flag(cfg: &Config, db_url: Option<&str>) -> Result<()> {
    if db_url.is_some() {
        return Ok(());
    }
    if let Some(url) = cfg.database.url.as_deref() {
        bail!(
            "reset-demo replaces the on-disk vault at paths.db, but this config serves the database from {}; URL-served databases cannot be reset this way — run reset-demo on the host that owns the database file",
            crate::import_cli::redact_db_url(url)
        );
    }
    Ok(())
}

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
    refuse_url_config_without_flag(&cfg, None)?;
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

async fn reset_prepared_bundle_at_url(
    cfg: &Config,
    bundle: &Path,
    account_id: &str,
    db_url: &str,
) -> Result<ResetPreparedStats> {
    let prepared = validate_prepared_bundle(bundle)?;
    wipe_demo_account_at_url(cfg, account_id, db_url).await?;

    println!("Reset demo — preparing replacement");
    println!("  account:      {account_id}");
    println!("  imessage:     {}", prepared.imessage_dir.display());
    println!("  android:      {}", prepared.sbr_dir.display());
    println!("  whatsapp:     {}", prepared.whatsapp_dir.display());
    println!(
        "  db:           {}",
        crate::import_cli::redact_db_url(db_url)
    );

    seed_demo_account_at_url(db_url, account_id, &prepared.seed).await?;
    let mut import_stats = crate::import_cli::run(
        cfg,
        &crate::import_cli::CliImportOptions {
            account_id: account_id.to_string(),
            input_dir: prepared.imessage_dir.clone(),
            db_path: None,
            db_url: Some(db_url.to_string()),
            assets_dir: None,
            source_override: Some(IMESSAGE_SOURCE.to_string()),
            mode: ImportMode::Replace,
            media: crate::import_media::MediaMode::Copy,
            contacts: Some(prepared.contacts_vcf.clone()),
            overwrite_contacts: true,
            skip_dedupe: true,
            window_secs: 2,
        },
    )
    .await?
    .import;
    let sbr_stats = crate::import_cli::run(
        cfg,
        &crate::import_cli::CliImportOptions {
            account_id: account_id.to_string(),
            input_dir: prepared.sbr_dir.clone(),
            db_path: None,
            db_url: Some(db_url.to_string()),
            assets_dir: None,
            source_override: Some(SBR_SOURCE.to_string()),
            mode: ImportMode::Append,
            media: crate::import_media::MediaMode::Copy,
            contacts: None,
            overwrite_contacts: false,
            skip_dedupe: true,
            window_secs: 2,
        },
    )
    .await?
    .import;
    merge_import_stats(&mut import_stats, &sbr_stats);
    let whatsapp_stats = crate::import_cli::run(
        cfg,
        &crate::import_cli::CliImportOptions {
            account_id: account_id.to_string(),
            input_dir: prepared.whatsapp_dir.clone(),
            db_path: None,
            db_url: Some(db_url.to_string()),
            assets_dir: None,
            source_override: Some(WHATSAPP_SOURCE.to_string()),
            mode: ImportMode::Append,
            media: crate::import_media::MediaMode::Copy,
            contacts: None,
            overwrite_contacts: false,
            skip_dedupe: true,
            window_secs: 2,
        },
    )
    .await?
    .import;
    merge_import_stats(&mut import_stats, &whatsapp_stats);

    let dedupe_stats = dedupe::run_dedupe(&cfg.paths.db, account_id, 2, Some(db_url)).await?;
    println!("Reset demo — processing prepared assets");
    let process_stats = process_assets::run(
        cfg,
        &ProcessAssetsOptions {
            force: false,
            dry_run: false,
            skip_image: false,
            skip_video: false,
            skip_audio: false,
            db: None,
            source: None,
            db_url: Some(db_url.to_string()),
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

    vacuum_after_demo_url(db_url).await;

    Ok(ResetPreparedStats {
        import: import_stats,
        dedupe_keys_filled: dedupe_stats.keys_filled,
        process_assets: process_stats,
    })
}

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
    wipe_demo_account(&temporary_cfg, account_id).await?;

    println!("Reset demo — preparing replacement");
    println!("  account:      {account_id}");
    println!("  imessage:     {}", prepared.imessage_dir.display());
    println!("  android:      {}", prepared.sbr_dir.display());
    println!("  whatsapp:     {}", prepared.whatsapp_dir.display());
    println!("  db:           {}", cfg.paths.db.display());

    seed_demo_account(&prepared_db, account_id, &prepared.seed).await?;
    let imessage_assets = temporary_cfg
        .paths
        .assets_dir_for_account(account_id, IMESSAGE_SOURCE);
    let sbr_assets = temporary_cfg
        .paths
        .assets_dir_for_account(account_id, SBR_SOURCE);
    let whatsapp_assets = temporary_cfg
        .paths
        .assets_dir_for_account(account_id, WHATSAPP_SOURCE);
    let mut import_stats = import::import_export(&ImportExportArgs {
        export_dir: &prepared.imessage_dir,
        db_path: &prepared_db,
        assets_dir: &imessage_assets,
        contacts: Some(&prepared.contacts_vcf),
        overwrite_contacts: true,
        mode: ImportMode::Replace,
        source: IMESSAGE_SOURCE,
        account_id,
    })
    .await?;
    let sbr_stats = import::import_export(&ImportExportArgs {
        export_dir: &prepared.sbr_dir,
        db_path: &prepared_db,
        assets_dir: &sbr_assets,
        contacts: None,
        overwrite_contacts: false,
        mode: ImportMode::Append,
        source: SBR_SOURCE,
        account_id,
    })
    .await?;
    merge_import_stats(&mut import_stats, &sbr_stats);
    let whatsapp_stats = import::import_export(&ImportExportArgs {
        export_dir: &prepared.whatsapp_dir,
        db_path: &prepared_db,
        assets_dir: &whatsapp_assets,
        contacts: None,
        overwrite_contacts: false,
        mode: ImportMode::Append,
        source: WHATSAPP_SOURCE,
        account_id,
    })
    .await?;
    merge_import_stats(&mut import_stats, &whatsapp_stats);

    let dedupe_stats = dedupe::run_dedupe(&prepared_db, account_id, 2, None).await?;
    println!("Reset demo — processing prepared assets");
    let process_stats = process_assets::run(
        &temporary_cfg,
        &ProcessAssetsOptions {
            force: false,
            dry_run: false,
            skip_image: false,
            skip_video: false,
            skip_audio: false,
            db: None,
            source: None,
            db_url: None,
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

    vacuum_after_demo_path(&prepared_db).await;

    verify_non_demo_state_preserved(&cfg.paths.db, &prepared_db, account_id).await?;
    let active_account = cfg.paths.data_dir.join(account_id);
    let prepared_account = temporary_cfg.paths.data_dir.join(account_id);
    let replacement = install_reset_state(&ResetPaths {
        active_db: &cfg.paths.db,
        prepared_db: &prepared_db,
        active_account: &active_account,
        prepared_account: &prepared_account,
        active_config: config_dest,
        prepared_config,
    })
    .await;
    if let Err(error) = replacement {
        let config_backup = sqlite_sidecar(prepared_config, ".previous-active");
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
        return Err(error);
    }

    crate::operation_lock::mark_ready(&cfg.paths.db)?;

    Ok(ResetPreparedStats {
        import: import_stats,
        dedupe_keys_filled: dedupe_stats.keys_filled,
        process_assets: process_stats,
    })
}

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

fn sqlite_sidecar(db: &Path, suffix: &str) -> PathBuf {
    let mut path: OsString = db.as_os_str().to_owned();
    path.push(suffix);
    PathBuf::from(path)
}

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

async fn install_reset_state(paths: &ResetPaths<'_>) -> Result<()> {
    install_reset_state_with(paths, rename_prepared_path).await
}

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

/// Compact import tables after the sample inbox is fully loaded. Failure is a
/// warning; the demo rows are already committed.
async fn vacuum_after_demo_url(db_url: &str) {
    let pool = match engine::open_pool_from_url(db_url).await {
        Ok(pool) => pool,
        Err(err) => {
            eprintln!("  sql:      warning: vacuum after demo failed to open the database: {err}");
            return;
        }
    };
    vacuum_after_demo_on_pool(pool).await;
}

async fn vacuum_after_demo_path(db_path: &Path) {
    let pool = match engine::open_pool_for_path(db_path).await {
        Ok(pool) => pool,
        Err(err) => {
            eprintln!("  sql:      warning: vacuum after demo failed to open the database: {err}");
            return;
        }
    };
    vacuum_after_demo_on_pool(pool).await;
}

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

fn merge_import_stats(into: &mut import::ImportStats, other: &import::ImportStats) {
    into.conversations += other.conversations;
    into.participants += other.participants;
    into.messages += other.messages;
    into.attachments += other.attachments;
    into.tapbacks += other.tapbacks;
    into.files += other.files;
    into.assets_copied += other.assets_copied;
    into.assets_deduped += other.assets_deduped;
    into.assets_missing += other.assets_missing;
    into.messages_deduped += other.messages_deduped;
    into.messages_appended += other.messages_appended;
    into.phones_needing_review += other.phones_needing_review;
}

fn load_demo_seed(path: &Path) -> Result<DemoSeed> {
    let text = fs::read_to_string(path)
        .with_context(|| format!("failed to read demo seed {}", path.display()))?;
    toml::from_str(&text).with_context(|| format!("failed to parse demo seed {}", path.display()))
}

async fn seed_demo_account(db_path: &Path, account_id: &str, seed: &DemoSeed) -> Result<()> {
    let pool = engine::open_pool_for_path(db_path).await?;
    let mut conn = pool.acquire().await?;
    schema::ensure_vault_schema(&mut conn).await?;
    seed_demo_account_on_conn(&mut conn, account_id, seed).await?;
    conn.close().await?;
    pool.close().await;
    Ok(())
}

async fn seed_demo_account_at_url(db_url: &str, account_id: &str, seed: &DemoSeed) -> Result<()> {
    let pool = engine::open_pool_from_url(db_url).await?;
    let mut conn = pool.acquire().await?;
    schema::ensure_vault_schema(&mut conn).await?;
    seed_demo_account_on_conn(&mut conn, account_id, seed).await?;
    conn.close().await?;
    pool.close().await;
    Ok(())
}

async fn seed_demo_account_on_conn(
    conn: &mut sqlx::AnyConnection,
    account_id: &str,
    seed: &DemoSeed,
) -> Result<()> {
    account_profile::ensure_account_row(conn, account_id).await?;

    sqlx::query(
        r#"
        INSERT INTO accounts (
            id, username, read_only, password_hash, preferred_name
        )
        VALUES ($1, $2, $3, NULL, $4)
        ON CONFLICT(id) DO UPDATE SET
            username = excluded.username,
            read_only = excluded.read_only,
            password_hash = NULL,
            preferred_name = excluded.preferred_name
        "#,
    )
    .bind(account_id)
    .bind(&seed.account.username)
    .bind(seed.account.read_only as i64)
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
/// on-disk attachments. Leaves `vault.db` and other accounts intact.
async fn wipe_demo_account(cfg: &Config, account_id: &str) -> Result<()> {
    let db = &cfg.paths.db;
    if db.is_file() {
        println!("Reset demo — clearing account data in {}", db.display());
        let pool = engine::open_pool_for_path(db)
            .await
            .with_context(|| format!("open {} for demo account wipe", db.display()))?;
        let mut conn = pool
            .acquire()
            .await
            .with_context(|| format!("open {} for demo account wipe", db.display()))?;
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
    } else {
        println!(
            "Reset demo — no existing db at {}; will create on import",
            db.display()
        );
    }

    let account_root = cfg.paths.data_dir.join(account_id);
    remove_tree_if_exists(&account_root)?;
    Ok(())
}

async fn wipe_demo_account_at_url(cfg: &Config, account_id: &str, db_url: &str) -> Result<()> {
    println!(
        "Reset demo — clearing account data in {}",
        crate::import_cli::redact_db_url(db_url)
    );
    let pool = engine::open_pool_from_url(db_url).await.with_context(|| {
        format!(
            "open {} for demo account wipe",
            crate::import_cli::redact_db_url(db_url)
        )
    })?;
    let mut conn = pool.acquire().await.with_context(|| {
        format!(
            "open {} for demo account wipe",
            crate::import_cli::redact_db_url(db_url)
        )
    })?;
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
    fn refuse_url_config_without_flag_errors_when_config_has_url() {
        let err = refuse_url_config_without_flag(&url_config_for_refuse_tests(), None)
            .expect_err("config URL without --db-url must fail");
        assert!(
            err.to_string()
                .contains("URL-served databases cannot be reset"),
            "{err}"
        );
    }

    #[test]
    fn refuse_url_config_without_flag_ok_when_db_url_flag_set() {
        refuse_url_config_without_flag(
            &url_config_for_refuse_tests(),
            Some("postgres://vault:vault@127.0.0.1:5432/vault"),
        )
        .expect("flag allows a URL-served config");
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
read_only = true
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
                r#"{{"schema_version":3,"export":{{"source":"{source}","tool":"t","tool_version":"0","owner_handle":null,"owner_display_name":null}},"conversation":{{"chat_identifier":"{chat}","conversation_type":"individual","group_title":null,"participants":[{{"handle":"{chat}","display_name":null}}],"stats":{{"message_count":1,"attachment_count":0,"first_timestamp_unix_ms":1426183462000,"last_timestamp_unix_ms":1426183462000}}}}}}
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
        assert!(seed.account.read_only);
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
