//! Restore the committed demo bundle: config, wipe DB/assets, re-import.

use std::fs;
use std::path::Path;

use anyhow::{Context, Result, bail};
use rusqlite::{Connection, params};
use serde::Deserialize;

use crate::config::Config;
use crate::dedupe;
use crate::import::{self, ImportMode};
use crate::schema;
use crate::vault_owner;

/// Stable demo account id used when `reset-demo` runs without `--account`.
pub const DEMO_ACCOUNT_ID: &str = "00000000-0000-0000-0000-00000000d001";

const DEMO_SOURCE: &str = "imessage";

#[derive(Debug)]
pub struct ResetDemoStats {
    pub import: import::ImportStats,
    pub dedupe_keys_filled: u64,
}

#[derive(Debug, Deserialize)]
struct DemoSeed {
    owner: DemoOwner,
    account: DemoAccount,
}

#[derive(Debug, Deserialize)]
struct DemoOwner {
    display_name: String,
    phones: Vec<String>,
    #[serde(default)]
    emails: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct DemoAccount {
    username: String,
    login_email: String,
    read_only: bool,
}

pub fn run_reset_demo(bundle: &Path, config_dest: &Path) -> Result<ResetDemoStats> {
    run_reset_demo_for_account(bundle, config_dest, DEMO_ACCOUNT_ID)
}

pub fn run_reset_demo_for_account(
    bundle: &Path,
    config_dest: &Path,
    account_id: &str,
) -> Result<ResetDemoStats> {
    let bundle = if bundle.is_absolute() {
        bundle.to_path_buf()
    } else {
        std::env::current_dir()?.join(bundle)
    };

    let demo_config = bundle.join("config/config.toml");
    let demo_seed = bundle.join("config/seed.toml");
    if !demo_config.is_file() {
        bail!(
            "demo bundle missing {} (run: cargo run -p demo-seed)",
            demo_config.display()
        );
    }
    if !demo_seed.is_file() {
        bail!(
            "demo bundle missing {} (run: cargo run -p demo-seed)",
            demo_seed.display()
        );
    }

    fs::create_dir_all(
        config_dest
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("config")),
    )?;
    fs::copy(&demo_config, config_dest)
        .with_context(|| format!("copy {} → {}", demo_config.display(), config_dest.display()))?;

    let cfg = Config::load(config_dest)?;
    let seed = load_demo_seed(&demo_seed)?;
    let export_dir = bundle.join("staging/imessage");
    if !export_dir.is_dir() {
        bail!(
            "demo staging missing {} (run: cargo run -p demo-seed)",
            export_dir.display()
        );
    }

    wipe_vault(&cfg, account_id)?;
    restore_demo_csvs(&bundle, &cfg, account_id)?;

    let assets_dir = cfg.paths.assets_dir_for_account(account_id, DEMO_SOURCE);
    let db = cfg.paths.db.clone();
    let (contacts_csv, exclude_csv) = cfg.paths.ensure_account_csvs(account_id)?;

    println!("Reset demo");
    println!("  bundle:       {}", bundle.display());
    println!("  config:       {}", config_dest.display());
    println!("  account:      {}", account_id);
    println!("  export_dir:   {}", export_dir.display());
    println!("  db:           {}", db.display());

    seed_demo_account(&db, account_id, &seed)?;

    let import_stats = import::import_export(
        &export_dir,
        &db,
        &assets_dir,
        &contacts_csv,
        &exclude_csv,
        true,
        ImportMode::Replace,
        DEMO_SOURCE,
        account_id,
    )?;

    let dedupe_stats = dedupe::run_dedupe(&db, account_id, 2)?;

    Ok(ResetDemoStats {
        import: import_stats,
        dedupe_keys_filled: dedupe_stats.keys_filled,
    })
}

fn load_demo_seed(path: &Path) -> Result<DemoSeed> {
    let text = fs::read_to_string(path)
        .with_context(|| format!("failed to read demo seed {}", path.display()))?;
    toml::from_str(&text).with_context(|| format!("failed to parse demo seed {}", path.display()))
}

fn seed_demo_account(db_path: &Path, account_id: &str, seed: &DemoSeed) -> Result<()> {
    let conn = Connection::open(db_path)?;
    conn.execute_batch("PRAGMA foreign_keys = ON;")?;
    schema::ensure_vault_schema(&conn)?;
    vault_owner::ensure_account_row(&conn, account_id)?;

    conn.execute(
        r#"
        INSERT INTO accounts (
            id, username, read_only, password_hash,
            first_name, last_name, preferred_name
        )
        VALUES (?1, ?2, ?3, NULL, ?4, '', ?4)
        ON CONFLICT(id) DO UPDATE SET
            username = excluded.username,
            read_only = excluded.read_only,
            password_hash = NULL,
            first_name = excluded.first_name,
            last_name = excluded.last_name,
            preferred_name = excluded.preferred_name
        "#,
        params![
            account_id,
            seed.account.username,
            seed.account.read_only as i64,
            seed.owner.display_name
        ],
    )?;
    conn.execute(
        "DELETE FROM account_emails WHERE account_id = ?1",
        params![account_id],
    )?;
    conn.execute(
        r#"
        INSERT INTO account_emails (account_id, email, is_primary)
        VALUES (?1, ?2, 1)
        "#,
        params![account_id, seed.account.login_email],
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

    conn.execute(
        "DELETE FROM account_phones WHERE account_id = ?1",
        params![account_id],
    )?;
    for phone in &seed.owner.phones {
        conn.execute(
            "INSERT INTO account_phones (account_id, phone) VALUES (?1, ?2)",
            params![account_id, phone],
        )?;
    }

    Ok(())
}

fn restore_demo_csvs(bundle: &Path, cfg: &Config, account_id: &str) -> Result<()> {
    let demo_contacts = bundle.join("config/contacts.csv");
    let demo_exclude = bundle.join("config/exclude.csv");
    let (contacts, exclude) = cfg.paths.ensure_account_csvs(account_id)?;
    copy_if_exists(&demo_contacts, &contacts)?;
    copy_if_exists(&demo_exclude, &exclude)?;
    Ok(())
}

fn copy_if_exists(from: &Path, to: &Path) -> Result<()> {
    if !from.is_file() {
        return Ok(());
    }
    if from == to {
        return Ok(());
    }
    if fs::canonicalize(from).ok() == fs::canonicalize(to).ok() {
        return Ok(());
    }
    if let Some(parent) = to.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)?;
    }
    fs::copy(from, to).with_context(|| format!("copy {} → {}", from.display(), to.display()))?;
    Ok(())
}

fn wipe_vault(cfg: &Config, account_id: &str) -> Result<()> {
    remove_db_files(&cfg.paths.db)?;
    let account_root = cfg.paths.data_dir.join(account_id);
    remove_tree_if_exists(&account_root)?;
    Ok(())
}

fn remove_db_files(db: &Path) -> Result<()> {
    for path in [
        db.to_path_buf(),
        db.with_extension("db-wal"),
        db.with_extension("db-shm"),
    ] {
        if path.is_file() {
            fs::remove_file(&path).with_context(|| format!("remove {}", path.display()))?;
        }
    }
    Ok(())
}

fn remove_tree_if_exists(path: &Path) -> Result<()> {
    if path.exists() {
        fs::remove_dir_all(path).with_context(|| format!("remove {}", path.display()))?;
    }
    Ok(())
}
