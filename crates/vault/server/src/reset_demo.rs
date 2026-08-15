//! Regenerate the demo bundle, clear the demo account's data, re-import, and process media.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use message_ir::HandleType;
use rusqlite::params;
use serde::Deserialize;

use crate::config::{Config, GuestDemoSettings};
use crate::db::account_profile;
use crate::db::schema;
use crate::dedupe;
use crate::guest_pool;
use crate::import::{self, ImportMode};
use crate::process_assets::{self, ProcessAssetsOptions};

/// Stable demo account id used when `reset-demo` runs without `--account`.
pub use crate::db::account_profile::DEMO_ACCOUNT_ID;

const IMESSAGE_SOURCE: &str = "imessage";
const SBR_SOURCE: &str = "sms-backup-restore";
const WHATSAPP_SOURCE: &str = "whatsapp";

#[derive(Debug)]
pub struct ResetDemoStats {
    pub seed: demo_seed::GenStats,
    pub import: import::ImportStats,
    pub dedupe_keys_filled: u64,
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

/// Rebuild the demo vault from the bundle at `bundle` and write the active
/// config to `config_dest`.
///
/// # Errors
///
/// Returns an error when the bundle is incomplete, the database cannot be
/// replaced, or import / media processing fails.
pub fn run_reset_demo(bundle: &Path, config_dest: &Path) -> Result<ResetDemoStats> {
    run_reset_demo_for_account(bundle, config_dest, DEMO_ACCOUNT_ID)
}

fn run_reset_demo_for_account(
    bundle: &Path,
    config_dest: &Path,
    account_id: &str,
) -> Result<ResetDemoStats> {
    let bundle = if bundle.is_absolute() {
        bundle.to_path_buf()
    } else {
        std::env::current_dir()?.join(bundle)
    };

    println!("  bundle:       {}", bundle.display());
    let seed_stats = maybe_regenerate_bundle(&bundle)?;
    let reset_stats = prepare_config_and_reset(&bundle, config_dest, account_id)?;

    Ok(ResetDemoStats {
        seed: seed_stats,
        import: reset_stats.import,
        dedupe_keys_filled: reset_stats.dedupe_keys_filled,
        process_assets: reset_stats.process_assets,
    })
}

fn prepare_config_and_reset(
    bundle: &Path,
    config_dest: &Path,
    account_id: &str,
) -> Result<ResetPreparedStats> {
    validate_prepared_bundle(bundle)?;
    let demo_config = bundle.join("config/config.toml");
    if !demo_config.is_file() {
        bail!(
            "incomplete demo bundle under {} (need config/config.toml)",
            bundle.display()
        );
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
    let temporary_config = temporary_config.into_temp_path();
    reset_prepared_bundle(
        &cfg,
        bundle,
        account_id,
        config_dest,
        temporary_config.as_ref(),
    )
}

fn reset_prepared_bundle(
    cfg: &Config,
    bundle: &Path,
    account_id: &str,
    config_dest: &Path,
    prepared_config: &Path,
) -> Result<ResetPreparedStats> {
    let prepared = validate_prepared_bundle(bundle)?;
    let _operation_lock = crate::operation_lock::acquire_for_reset(&cfg.paths.db)?;
    let db_parent = parent_dir_or_cwd(&cfg.paths.db);
    let data_parent = parent_dir_or_cwd(&cfg.paths.data_dir);
    fs::create_dir_all(db_parent)
        .with_context(|| format!("create database parent {}", db_parent.display()))?;
    fs::create_dir_all(data_parent)
        .with_context(|| format!("create data parent {}", data_parent.display()))?;
    let db_work = tempfile::Builder::new()
        .prefix(".reset-demo-db-")
        .tempdir_in(db_parent)
        .context("create temporary demo database directory")?;
    let data_work = tempfile::Builder::new()
        .prefix(".reset-demo-data-")
        .tempdir_in(data_parent)
        .context("create temporary demo account directory")?;
    let prepared_db = db_work.path().join("vault.db");
    checkpoint_and_clean_sidecars(&cfg.paths.db, "before creating the reset snapshot")?;
    prepare_database_snapshot(&cfg.paths.db, &prepared_db)?;

    let mut temporary_cfg = cfg.clone();
    temporary_cfg.paths.db = prepared_db.clone();
    temporary_cfg.paths.data_dir = data_work.path().to_path_buf();
    wipe_demo_account(&temporary_cfg, account_id)?;

    println!("Reset demo — preparing replacement");
    println!("  account:      {account_id}");
    println!("  imessage:     {}", prepared.imessage_dir.display());
    println!("  android:      {}", prepared.sbr_dir.display());
    println!("  whatsapp:     {}", prepared.whatsapp_dir.display());
    println!("  db:           {}", cfg.paths.db.display());

    seed_demo_account(&prepared_db, account_id, &prepared.seed)?;
    let imessage_assets = temporary_cfg
        .paths
        .assets_dir_for_account(account_id, IMESSAGE_SOURCE);
    let sbr_assets = temporary_cfg
        .paths
        .assets_dir_for_account(account_id, SBR_SOURCE);
    let whatsapp_assets = temporary_cfg
        .paths
        .assets_dir_for_account(account_id, WHATSAPP_SOURCE);
    let mut import_stats = import::import_export(
        &prepared.imessage_dir,
        &prepared_db,
        &imessage_assets,
        Some(&prepared.contacts_vcf),
        true,
        ImportMode::Replace,
        IMESSAGE_SOURCE,
        account_id,
    )?;
    let sbr_stats = import::import_export(
        &prepared.sbr_dir,
        &prepared_db,
        &sbr_assets,
        None,
        false,
        ImportMode::Append,
        SBR_SOURCE,
        account_id,
    )?;
    merge_import_stats(&mut import_stats, &sbr_stats);
    let whatsapp_stats = import::import_export(
        &prepared.whatsapp_dir,
        &prepared_db,
        &whatsapp_assets,
        None,
        false,
        ImportMode::Append,
        WHATSAPP_SOURCE,
        account_id,
    )?;
    merge_import_stats(&mut import_stats, &whatsapp_stats);

    let dedupe_stats = dedupe::run_dedupe(&prepared_db, account_id, 2)?;
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
        },
    )
    .context("process-assets after prepared demo import")?;
    if process_stats.errors > 0 {
        bail!(
            "prepared demo asset processing reported {} conversion failures",
            process_stats.errors
        );
    }

    verify_non_demo_state_preserved(&cfg.paths.db, &prepared_db, account_id)?;
    let replacement = install_reset_state(
        &cfg.paths.db,
        &prepared_db,
        &cfg.paths.data_dir.join(account_id),
        &temporary_cfg.paths.data_dir.join(account_id),
        config_dest,
        prepared_config,
    );
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

    after_reset_refresh_guest_pool(cfg, GuestDemoSettings::from_env())?;

    Ok(ResetPreparedStats {
        import: import_stats,
        dedupe_keys_filled: dedupe_stats.keys_filled,
        process_assets: process_stats,
    })
}

/// After a new demo template is live, drop unused ready guests and refill.
///
/// Assigned guests are left alone so a visitor who already has a copy keeps it.
/// When the hosted pool is off this is a no-op.
///
/// # Errors
///
/// Returns an error when the live database cannot be opened, unused ready
/// guests cannot be deleted, or a refill clone fails.
pub fn after_reset_refresh_guest_pool(cfg: &Config, settings: GuestDemoSettings) -> Result<()> {
    if !settings.enabled {
        return Ok(());
    }
    let mut conn = schema::open_configured(&cfg.paths.db)?;
    guest_pool::drop_ready_guests(&conn, &cfg.paths.data_dir)?;
    guest_pool::refill_pool(&mut conn, cfg, settings, 0)?;
    Ok(())
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

fn prepare_database_snapshot(active: &Path, prepared: &Path) -> Result<()> {
    if active.is_file() {
        let conn = rusqlite::Connection::open(active)
            .with_context(|| format!("open {} for reset snapshot", active.display()))?;
        conn.execute(
            "VACUUM INTO ?1",
            params![prepared.to_string_lossy().as_ref()],
        )
        .with_context(|| {
            format!(
                "copy database snapshot {} to {}",
                active.display(),
                prepared.display()
            )
        })?;
    } else {
        let conn = schema::open_configured(prepared)
            .with_context(|| format!("create prepared database {}", prepared.display()))?;
        schema::ensure_vault_schema(&conn)?;
    }
    Ok(())
}

/// Copy pending SQLite writes into the main database file, then remove the
/// write-ahead log (`-wal`) and shared-memory (`-shm`) sidecar files so the
/// database can be renamed safely.
fn checkpoint_and_clean_sidecars(db: &Path, operation: &str) -> Result<()> {
    if !db.is_file() {
        return Ok(());
    }
    let conn = rusqlite::Connection::open(db)
        .with_context(|| format!("open {} {operation}", db.display()))?;
    let (busy, _, _): (i64, i64, i64) = conn
        .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })
        .with_context(|| {
            format!(
                "checkpoint SQLite write-ahead log for {} {operation}",
                db.display()
            )
        })?;
    conn.close()
        .map_err(|(_, error)| error)
        .with_context(|| format!("close {} {operation}", db.display()))?;
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

fn verify_non_demo_state_preserved(active: &Path, prepared: &Path, demo_id: &str) -> Result<()> {
    if !active.is_file() {
        return Ok(());
    }
    let active_state = non_demo_state(active, demo_id)?;
    let prepared_state = non_demo_state(prepared, demo_id)?;
    if active_state != prepared_state {
        bail!(
            "prepared reset database changed non-demo account state; active={active_state:?}, prepared={prepared_state:?}"
        );
    }
    Ok(())
}

fn non_demo_state(db: &Path, demo_id: &str) -> Result<BTreeMap<String, i64>> {
    let conn = rusqlite::Connection::open(db)
        .with_context(|| format!("open {} to verify non-demo accounts", db.display()))?;
    let mut statement = conn.prepare(
        "SELECT a.id, COUNT(m.id)
         FROM accounts a
         LEFT JOIN messages m ON m.account_id = a.id
         WHERE a.id != ?1
         GROUP BY a.id
         ORDER BY a.id",
    )?;
    let rows = statement.query_map(params![demo_id], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
    })?;
    let mut state = BTreeMap::new();
    for row in rows {
        let (account_id, message_count) = row?;
        state.insert(account_id, message_count);
    }
    Ok(state)
}

fn install_reset_state(
    active_db: &Path,
    prepared_db: &Path,
    active_account: &Path,
    prepared_account: &Path,
    active_config: &Path,
    prepared_config: &Path,
) -> Result<()> {
    install_reset_state_with(
        active_db,
        prepared_db,
        active_account,
        prepared_account,
        active_config,
        prepared_config,
        |source, destination| {
            fs::rename(source, destination).with_context(|| {
                format!("rename {} to {}", source.display(), destination.display())
            })
        },
    )
}

#[allow(clippy::too_many_arguments)]
fn install_reset_state_with<F>(
    active_db: &Path,
    prepared_db: &Path,
    active_account: &Path,
    prepared_account: &Path,
    active_config: &Path,
    prepared_config: &Path,
    rename: F,
) -> Result<()>
where
    F: FnMut(&Path, &Path) -> Result<()>,
{
    checkpoint_and_clean_sidecars(prepared_db, "before installing the prepared database")?;
    checkpoint_and_clean_sidecars(
        active_db,
        "immediately before replacing the active database",
    )?;
    replace_reset_state_with(
        active_db,
        prepared_db,
        active_account,
        prepared_account,
        active_config,
        prepared_config,
        rename,
    )
}

#[allow(clippy::too_many_arguments)]
fn replace_reset_state_with<F>(
    active_db: &Path,
    prepared_db: &Path,
    active_account: &Path,
    prepared_account: &Path,
    active_config: &Path,
    prepared_config: &Path,
    mut rename: F,
) -> Result<()>
where
    F: FnMut(&Path, &Path) -> Result<()>,
{
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
        if installed_account {
            if let Err(rollback_error) = remove_any_if_exists(active_account) {
                rollback_errors.push(format!(
                    "remove installed account directory {}: {rollback_error:#}",
                    active_account.display()
                ));
            }
        }
        if installed_db && active_db.exists() {
            if let Err(rollback_error) = remove_any_if_exists(active_db) {
                rollback_errors.push(format!(
                    "remove installed database {}: {rollback_error:#}",
                    active_db.display()
                ));
            }
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
        if had_account && account_backup.exists() {
            if let Err(rollback_error) = rename(&account_backup, active_account) {
                rollback_errors.push(format!(
                    "restore previous account directory {}: {rollback_error:#}",
                    active_account.display()
                ));
            }
        }
        if had_db && db_backup.exists() {
            if let Err(rollback_error) = rename(&db_backup, active_db) {
                rollback_errors.push(format!(
                    "restore previous database {}: {rollback_error:#}",
                    active_db.display()
                ));
            }
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
/// (a development checkout). Release images only ship the committed
/// staging/config tree — skip regeneration there.
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
            "Reset demo — using committed bundle (no {} in this image)",
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
        "cannot reset demo: {} is missing and {} is not a complete committed bundle \
         (need staging/{IMESSAGE_SOURCE}/, staging/{SBR_SOURCE}/, and staging/{WHATSAPP_SOURCE}/)",
        seed_toml.display(),
        bundle.display()
    );
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

fn seed_demo_account(db_path: &Path, account_id: &str, seed: &DemoSeed) -> Result<()> {
    let conn = schema::open_configured(db_path)?;
    schema::ensure_vault_schema(&conn)?;
    account_profile::ensure_account_row(&conn, account_id)?;

    conn.execute(
        r#"
        INSERT INTO accounts (
            id, username, read_only, password_hash, preferred_name
        )
        VALUES (?1, ?2, ?3, NULL, ?4)
        ON CONFLICT(id) DO UPDATE SET
            username = excluded.username,
            read_only = excluded.read_only,
            password_hash = NULL,
            preferred_name = excluded.preferred_name
        "#,
        params![
            account_id,
            seed.account.username,
            seed.account.read_only as i64,
            seed.owner.display_name
        ],
    )?;
    // Extra email addresses used only to recognize "you" in messages, not for login.
    conn.execute(
        "DELETE FROM account_emails WHERE account_id = ?1",
        params![account_id],
    )?;
    for email in &seed.owner.emails {
        conn.execute(
            r#"
            INSERT OR IGNORE INTO account_emails (account_id, email, is_primary)
            VALUES (?1, ?2, 0)
            "#,
            params![account_id, email],
        )?;
    }
    // Demo has no Import API token until the user generates one in Settings.

    // Phone and email identities that mark messages as from "you" live in
    // `handles`, linked through `account_handles`.
    conn.execute(
        "DELETE FROM account_handles WHERE account_id = ?1",
        params![account_id],
    )?;
    for (raw, handle_type) in &seed.owner.handle_specs {
        account_profile::link_account_handle(&conn, account_id, raw, *handle_type)?;
    }

    Ok(())
}

/// Delete the demo account's vault rows (child rows follow via CASCADE) and
/// on-disk attachments. Leaves `vault.db` and other accounts intact.
fn wipe_demo_account(cfg: &Config, account_id: &str) -> Result<()> {
    let db = &cfg.paths.db;
    if db.is_file() {
        println!("Reset demo — clearing account data in {}", db.display());
        let conn = schema::open_configured(db)
            .with_context(|| format!("open {} for demo account wipe", db.display()))?;
        schema::ensure_vault_schema(&conn)?;
        let deleted = conn
            .execute("DELETE FROM accounts WHERE id = ?1", params![account_id])
            .with_context(|| format!("delete account {account_id}"))?;
        println!("  sql:      demo account rows removed (accounts matched={deleted})");
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

fn remove_tree_if_exists(path: &Path) -> Result<()> {
    if path.exists() {
        fs::remove_dir_all(path).with_context(|| format!("remove {}", path.display()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{GuestDemoSettings, PathsConfig};
    use crate::guest_pool;

    /// A template refresh must delete unused ready guests (they still point at
    /// the old sample set) and grow the pool back to `pool_min` from the new
    /// template. Assigned guests keep the snapshot they already received.
    #[test]
    fn after_reset_refresh_guest_pool_drops_ready_and_refills_to_min() {
        let (_temp, cfg, stale_id, assigned_id) = guest_pool_refresh_fixture();
        let settings = GuestDemoSettings {
            enabled: true,
            pool_min: 2,
            pool_max: 20,
            session_secs: 60,
        };

        after_reset_refresh_guest_pool(&cfg, settings).expect("refresh guest pool");

        let conn = schema::open_configured(&cfg.paths.db).expect("reopen");
        assert!(
            account_profile::username_for_account(&conn, &stale_id)
                .unwrap()
                .is_none(),
            "unused ready guest must be deleted so it cannot hand out the old dataset"
        );
        assert_eq!(
            account_profile::guest_status(&conn, &assigned_id)
                .unwrap()
                .as_deref(),
            Some("assigned"),
            "assigned guests stay after a template refresh"
        );
        assert_eq!(guest_pool::count_ready(&conn).unwrap(), settings.pool_min);
        assert!(!cfg.paths.data_dir.join(&stale_id).exists());
        assert!(cfg.paths.data_dir.join(&assigned_id).is_dir());
    }

    #[test]
    fn after_reset_refresh_guest_pool_skips_when_disabled() {
        let (_temp, cfg, stale_id, assigned_id) = guest_pool_refresh_fixture();
        let settings = GuestDemoSettings {
            enabled: false,
            pool_min: 2,
            pool_max: 20,
            session_secs: 60,
        };

        after_reset_refresh_guest_pool(&cfg, settings).expect("disabled refresh is a no-op");

        let conn = schema::open_configured(&cfg.paths.db).expect("reopen");
        assert_eq!(
            account_profile::guest_status(&conn, &stale_id)
                .unwrap()
                .as_deref(),
            Some("ready")
        );
        assert_eq!(
            account_profile::guest_status(&conn, &assigned_id)
                .unwrap()
                .as_deref(),
            Some("assigned")
        );
        assert_eq!(guest_pool::count_ready(&conn).unwrap(), 1);
    }

    fn guest_pool_refresh_fixture() -> (tempfile::TempDir, Config, String, String) {
        let temp = tempfile::tempdir().expect("create test directory");
        let db = temp.path().join("vault.db");
        let data_dir = temp.path().join("data");
        fs::create_dir_all(&data_dir).expect("create data dir");

        let conn = schema::open_configured(&db).expect("open test database");
        schema::ensure_vault_schema(&conn).expect("create vault schema");
        conn.execute(
            "INSERT INTO accounts (id, username, read_only, preferred_name)
             VALUES (?1, 'demo', 1, 'Alex Demo')",
            params![DEMO_ACCOUNT_ID],
        )
        .expect("insert template account");
        conn.execute(
            "INSERT INTO handles (account_id, raw, normalized, handle_type, service)
             VALUES (?1, '+15555550100', '+15555550100', 'phone', 'phone')",
            params![DEMO_ACCOUNT_ID],
        )
        .expect("insert template handle");
        let hid = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO account_handles (account_id, handle_id) VALUES (?1, ?2)",
            params![DEMO_ACCOUNT_ID, hid],
        )
        .expect("link template handle");
        conn.execute(
            "INSERT INTO conversations (account_id, chat_handle_id, conversation_type, source_file)
             VALUES (?1, ?2, 'individual', 'a.jsonl')",
            params![DEMO_ACCOUNT_ID, hid],
        )
        .expect("insert template conversation");
        let cid = conn.last_insert_rowid();
        conn.execute(
            r#"INSERT INTO messages (
                conversation_id, account_id, source, guid, timestamp, is_from_me, sort_order, body
            ) VALUES (?1, ?2, 'imessage', 'g1', '2020-01-01T00:00:00Z', 1, 0, 'hello')"#,
            params![cid, DEMO_ACCOUNT_ID],
        )
        .expect("insert template message");

        let stale_id = "eeeeeeee-eeee-4eee-8eee-eeeeeeeeeee1".to_string();
        let assigned_id = "eeeeeeee-eeee-4eee-8eee-eeeeeeeeeee2".to_string();
        account_profile::insert_guest_account(&conn, &stale_id, "guest-stale", Some("Guest"))
            .expect("insert stale ready guest");
        account_profile::insert_guest_account(&conn, &assigned_id, "guest-keep", Some("Guest"))
            .expect("insert assigned guest");
        account_profile::set_guest_status(&conn, &assigned_id, "assigned").expect("mark assigned");
        fs::create_dir_all(data_dir.join(&stale_id)).expect("stale guest dir");
        fs::create_dir_all(data_dir.join(&assigned_id)).expect("assigned guest dir");
        drop(conn);

        let cfg = Config {
            paths: PathsConfig {
                db,
                data_dir,
                assets_dir: "assets".into(),
                assets_converted_dir: "assets_converted".into(),
            },
            server: None,
        };
        (temp, cfg, stale_id, assigned_id)
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

    #[test]
    fn failed_reset_preserves_existing_demo_account() {
        let temp = tempfile::tempdir().expect("create test directory");
        let db = temp.path().join("vault.db");
        let data_dir = temp.path().join("data");
        let account_root = data_dir.join(DEMO_ACCOUNT_ID);
        fs::create_dir_all(&account_root).expect("create account data directory");
        let sentinel = account_root.join("existing.bin");
        let original_data = b"existing account data\n";
        fs::write(&sentinel, original_data).expect("write account data sentinel");

        let conn = schema::open_configured(&db).expect("open test database");
        schema::ensure_vault_schema(&conn).expect("create vault schema");
        account_profile::ensure_account_row(&conn, DEMO_ACCOUNT_ID).expect("seed account");
        conn.execute(
            "INSERT INTO handles (
                account_id, raw, normalized, handle_type, service
             ) VALUES (?1, '+15555550100', '+15555550100', 'phone', 'phone')",
            params![DEMO_ACCOUNT_ID],
        )
        .expect("insert handle");
        let handle_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO conversations (
                account_id, chat_handle_id, conversation_type, source_file
             ) VALUES (?1, ?2, 'individual', 'existing.jsonl')",
            params![DEMO_ACCOUNT_ID, handle_id],
        )
        .expect("insert conversation");
        let conversation_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO messages (
                conversation_id, account_id, source, guid, timestamp,
                is_from_me, body, sort_order
             ) VALUES (?1, ?2, 'imessage', 'existing-message',
                       '2026-01-01T00:00:00Z', 0, 'keep me', 0)",
            params![conversation_id, DEMO_ACCOUNT_ID],
        )
        .expect("insert message");
        drop(conn);

        let cfg = Config {
            paths: PathsConfig {
                db: db.clone(),
                data_dir,
                assets_dir: "assets".into(),
                assets_converted_dir: "assets_converted".into(),
            },
            server: None,
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
        );

        assert!(result.is_err());
        let conn = schema::open_configured(&db).expect("reopen test database");
        let account_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM accounts WHERE id = ?1",
                params![DEMO_ACCOUNT_ID],
                |row| row.get(0),
            )
            .expect("count account");
        let message_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM messages WHERE guid = 'existing-message'",
                [],
                |row| row.get(0),
            )
            .expect("count message");
        assert_eq!(account_count, 1);
        assert_eq!(message_count, 1);
        assert_eq!(
            fs::read(&sentinel).expect("read account sentinel"),
            original_data
        );
    }

    #[test]
    fn failed_preparation_preserves_active_config() {
        let temp = tempfile::tempdir().expect("create test directory");
        let config_dest = temp.path().join("config/config.toml");
        fs::create_dir_all(config_dest.parent().expect("config parent"))
            .expect("create config parent");
        let original = b"active configuration\n";
        fs::write(&config_dest, original).expect("write active config");
        let invalid_bundle = temp.path().join("invalid-bundle");
        fs::create_dir_all(&invalid_bundle).expect("create invalid bundle");

        let result = prepare_config_and_reset(&invalid_bundle, &config_dest, DEMO_ACCOUNT_ID);

        assert!(result.is_err());
        assert_eq!(
            fs::read(&config_dest).expect("read active config"),
            original
        );
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

    #[test]
    fn failures_after_database_and_account_install_restore_all_active_state() {
        for failure_point in [
            ResetInstallFailure::AfterDatabase,
            ResetInstallFailure::AfterAccount,
        ] {
            let temp = tempfile::tempdir().expect("create test directory");
            let active_db = temp.path().join("active/vault.db");
            fs::create_dir_all(active_db.parent().expect("database parent"))
                .expect("create database parent");
            seed_reset_test_database(&active_db);
            let prepared_db = temp.path().join("prepared/vault.db");
            fs::create_dir_all(prepared_db.parent().expect("prepared database parent"))
                .expect("create prepared database parent");
            fs::copy(&active_db, &prepared_db).expect("copy prepared database");
            make_prepared_reset_database_observably_different(&prepared_db);

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
                &active_db,
                &prepared_db,
                &active_account,
                &prepared_account,
                &active_config,
                &prepared_config,
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
            assert_reset_test_database(&active_db);
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

    #[test]
    fn active_sidecars_are_cleaned_immediately_before_database_rename() {
        let temp = tempfile::tempdir().expect("create test directory");
        let active_db = temp.path().join("active/vault.db");
        fs::create_dir_all(active_db.parent().expect("database parent"))
            .expect("create database parent");
        seed_reset_test_database(&active_db);
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
            let conn = schema::open_configured(&active_db).expect("reopen active database");
            conn.execute(
                "UPDATE accounts SET preferred_name = 'reopened' WHERE id = ?1",
                params![DEMO_ACCOUNT_ID],
            )
            .expect("write through reopened active database");
        }
        let active_wal = sqlite_sidecar(&active_db, "-wal");
        let active_shm = sqlite_sidecar(&active_db, "-shm");
        fs::write(&active_wal, b"").expect("create empty WAL sidecar");
        fs::write(&active_shm, b"").expect("create empty shared-memory sidecar");
        let mut observed_clean_boundary = false;

        let result = install_reset_state_with(
            &active_db,
            &prepared_db,
            &active_account,
            &prepared_account,
            &active_config,
            &prepared_config,
            |source, destination| {
                if source == active_db {
                    observed_clean_boundary = !active_wal.exists() && !active_shm.exists();
                }
                if source == prepared_db {
                    bail!("stop after observing active database rename boundary");
                }
                fs::rename(source, destination).map_err(Into::into)
            },
        );

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
            &active_db,
            &prepared_db,
            &active_account,
            &prepared_account,
            &active_config,
            &prepared_config,
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

    fn seed_reset_test_database(path: &Path) {
        let conn = schema::open_configured(path).expect("open reset test database");
        schema::ensure_vault_schema(&conn).expect("create reset test schema");
        seed_reset_test_account(&conn, DEMO_ACCOUNT_ID, "demo-existing");
        seed_reset_test_account(&conn, "non-demo-account", "non-demo-existing");
    }

    fn make_prepared_reset_database_observably_different(path: &Path) {
        let conn = schema::open_configured(path).expect("open prepared reset test database");
        conn.execute(
            "UPDATE accounts SET username = 'prepared-demo' WHERE id = ?1",
            params![DEMO_ACCOUNT_ID],
        )
        .expect("change prepared demo account");
        conn.execute(
            "DELETE FROM messages WHERE account_id = ?1",
            params![DEMO_ACCOUNT_ID],
        )
        .expect("delete prepared demo message");
        conn.execute("DELETE FROM accounts WHERE id = 'non-demo-account'", [])
            .expect("delete prepared non-demo marker");
        drop(conn);
        checkpoint_and_clean_sidecars(path, "while preparing reset test database")
            .expect("checkpoint prepared reset test database");

        let conn = schema::open_configured(path).expect("verify prepared reset test database");
        let demo_username: String = conn
            .query_row(
                "SELECT username FROM accounts WHERE id = ?1",
                params![DEMO_ACCOUNT_ID],
                |row| row.get(0),
            )
            .expect("read changed prepared demo account");
        let demo_messages: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM messages WHERE account_id = ?1",
                params![DEMO_ACCOUNT_ID],
                |row| row.get(0),
            )
            .expect("count prepared demo messages");
        let non_demo_accounts: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM accounts WHERE id = 'non-demo-account'",
                [],
                |row| row.get(0),
            )
            .expect("count prepared non-demo marker");
        assert_eq!(demo_username, "prepared-demo");
        assert_eq!(demo_messages, 0);
        assert_eq!(non_demo_accounts, 0);
    }

    fn seed_reset_test_account(conn: &rusqlite::Connection, account_id: &str, guid: &str) {
        account_profile::ensure_account_row(conn, account_id).expect("seed reset test account");
        conn.execute(
            "INSERT INTO handles (
                account_id, raw, normalized, handle_type, service
             ) VALUES (?1, ?2, ?2, 'username', 'phone')",
            params![account_id, format!("{account_id}-handle")],
        )
        .expect("insert reset test handle");
        let handle_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO conversations (
                account_id, chat_handle_id, conversation_type, source_file
             ) VALUES (?1, ?2, 'individual', 'existing.jsonl')",
            params![account_id, handle_id],
        )
        .expect("insert reset test conversation");
        let conversation_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO messages (
                conversation_id, account_id, source, guid, timestamp,
                is_from_me, body, sort_order
             ) VALUES (?1, ?2, 'imessage', ?3,
                       '2026-01-01T00:00:00Z', 0, 'keep me', 0)",
            params![conversation_id, account_id, guid],
        )
        .expect("insert reset test message");
    }

    fn assert_reset_test_database(path: &Path) {
        let conn = schema::open_configured(path).expect("open restored reset test database");
        for (account_id, guid) in [
            (DEMO_ACCOUNT_ID, "demo-existing"),
            ("non-demo-account", "non-demo-existing"),
        ] {
            let account_count: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM accounts WHERE id = ?1",
                    params![account_id],
                    |row| row.get(0),
                )
                .expect("count restored account");
            let username: String = conn
                .query_row(
                    "SELECT username FROM accounts WHERE id = ?1",
                    params![account_id],
                    |row| row.get(0),
                )
                .expect("read restored username");
            let (message_count, body): (i64, String) = conn
                .query_row(
                    "SELECT COUNT(*), MIN(body)
                     FROM messages WHERE account_id = ?1 AND guid = ?2",
                    params![account_id, guid],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .expect("count restored message");
            assert_eq!(account_count, 1, "account {account_id}");
            assert_eq!(username, account_id, "username {account_id}");
            assert_eq!(message_count, 1, "message {guid}");
            assert_eq!(body, "keep me", "message body {guid}");
        }
    }
}
