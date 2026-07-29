//! Restore the committed demo bundle: config, wipe DB/assets, re-import.

use std::fs;
use std::path::Path;

use anyhow::{bail, Context, Result};
use rusqlite::{params, Connection};

use crate::config::Config;
use crate::dedupe;
use crate::import::{self, ImportMode};
use crate::schema;
use crate::vault_owner;

/// Stable demo account id used when `reset-demo` runs without `--account`.
pub const DEMO_ACCOUNT_ID: &str = "00000000-0000-0000-0000-00000000d001";

#[derive(Debug)]
pub struct ResetDemoStats {
    pub import: import::ImportStats,
    pub dedupe_keys_filled: u64,
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
    if !demo_config.is_file() {
        bail!(
            "demo bundle missing {} (run: cargo run -p demo-seed)",
            demo_config.display()
        );
    }

    fs::create_dir_all(
        config_dest
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("config")),
    )?;
    fs::copy(&demo_config, config_dest).with_context(|| {
        format!(
            "copy {} → {}",
            demo_config.display(),
            config_dest.display()
        )
    })?;

    let cfg = Config::load(config_dest)?;
    wipe_vault(&cfg, account_id)?;
    restore_demo_csvs(&bundle, &cfg, account_id)?;

    let src = cfg.source("imessage")?;
    let export_dir = src.export_dir.clone();
    let assets_dir = src.resolved_assets_dir_for_account(&cfg.paths, account_id);
    let db = cfg.paths.db.clone();
    let (contacts_csv, exclude_csv) = cfg.paths.ensure_account_csvs(account_id)?;

    println!("Reset demo");
    println!("  bundle:       {}", bundle.display());
    println!("  config:       {}", config_dest.display());
    println!("  account:      {}", account_id);
    println!("  export_dir:   {}", export_dir.display());
    println!("  db:           {}", db.display());

    seed_demo_account(&db, account_id, &cfg)?;

    let import_stats = import::import_export(
        &export_dir,
        &db,
        &assets_dir,
        &contacts_csv,
        &exclude_csv,
        true,
        ImportMode::Replace,
        "imessage",
        account_id,
    )?;

    let priority: Vec<String> = cfg.sources.iter().map(|s| s.id.clone()).collect();
    let dedupe_stats = dedupe::run_dedupe(&db, account_id, &priority, 2)?;

    Ok(ResetDemoStats {
        import: import_stats,
        dedupe_keys_filled: dedupe_stats.keys_filled,
    })
}

fn seed_demo_account(db_path: &Path, account_id: &str, cfg: &Config) -> Result<()> {
    let conn = Connection::open(db_path)?;
    conn.execute_batch("PRAGMA foreign_keys = ON;")?;
    schema::ensure_vault_schema(&conn)?;
    vault_owner::ensure_account_row(&conn, account_id)?;

    if let Some(acct) = &cfg.account {
        conn.execute(
            r#"
            INSERT INTO accounts (id, username, read_only)
            VALUES (?1, ?2, ?3)
            ON CONFLICT(id) DO UPDATE SET
                username = excluded.username,
                read_only = excluded.read_only
            "#,
            params![account_id, acct.username, acct.read_only as i64],
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
            params![account_id, acct.login_email],
        )?;
        crate::api_tokens::ensure_account_api_token(&conn, account_id)?;
    }

    if let Some(owner) = &cfg.owner {
        conn.execute(
            r#"
            INSERT INTO vault_owners (account_id, first_name, last_name, display_name)
            VALUES (?1, ?2, '', ?2)
            ON CONFLICT(account_id) DO UPDATE SET
              first_name = excluded.first_name,
              last_name = excluded.last_name,
              display_name = excluded.display_name
            "#,
            params![account_id, owner.display_name],
        )?;
        conn.execute(
            "DELETE FROM vault_owner_phones WHERE account_id = ?1",
            params![account_id],
        )?;
        for phone in &owner.phones {
            conn.execute(
                "INSERT INTO vault_owner_phones (account_id, phone) VALUES (?1, ?2)",
                params![account_id, phone],
            )?;
        }
        conn.execute(
            "DELETE FROM vault_owner_emails WHERE account_id = ?1",
            params![account_id],
        )?;
        for email in &owner.emails {
            conn.execute(
                "INSERT INTO vault_owner_emails (account_id, email) VALUES (?1, ?2)",
                params![account_id, email],
            )?;
        }
    }

    Ok(())
}

fn restore_demo_csvs(bundle: &Path, cfg: &Config, account_id: &str) -> Result<()> {
    let demo_contacts = bundle.join("config/contacts.csv");
    let demo_exclude = bundle.join("config/exclude.csv");
    // Keep legacy template paths for seed fallback, and write per-account copies.
    copy_if_exists(&demo_contacts, &cfg.paths.contacts_csv)?;
    copy_if_exists(&demo_exclude, &cfg.paths.exclude_csv)?;
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
    if let Some(parent) = to.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }
    fs::copy(from, to).with_context(|| format!("copy {} → {}", from.display(), to.display()))?;
    Ok(())
}

fn wipe_vault(cfg: &Config, account_id: &str) -> Result<()> {
    remove_db_files(&cfg.paths.db)?;
    for src in &cfg.sources {
        let assets = src.resolved_assets_dir_for_account(&cfg.paths, account_id);
        let converted = src.resolved_assets_converted_dir_for_account(&cfg.paths, account_id);
        remove_tree_if_exists(&assets)?;
        remove_tree_if_exists(&converted)?;
    }
    Ok(())
}

fn remove_db_files(db: &Path) -> Result<()> {
    for path in [db.to_path_buf(), db.with_extension("db-wal"), db.with_extension("db-shm")] {
        if path.is_file() {
            fs::remove_file(&path)
                .with_context(|| format!("remove {}", path.display()))?;
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
