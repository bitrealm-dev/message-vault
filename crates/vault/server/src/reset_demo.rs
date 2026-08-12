//! Regenerate the demo bundle, clear the demo account's data, re-import, and process media.

use std::fs;
use std::path::Path;

use anyhow::{Context, Result, bail};
use message_ir::HandleType;
use rusqlite::params;
use serde::Deserialize;

use crate::config::Config;
use crate::db::account_profile;
use crate::db::schema;
use crate::dedupe;
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

    let demo_config = bundle.join("config/config.toml");
    let demo_seed = bundle.join("config/seed.toml");
    let imessage_dir = bundle.join("staging").join(IMESSAGE_SOURCE);
    let sbr_dir = bundle.join("staging").join(SBR_SOURCE);
    let whatsapp_dir = bundle.join("staging").join(WHATSAPP_SOURCE);
    let contacts_vcf = bundle.join("config/contacts.vcf");
    if !demo_config.is_file()
        || !demo_seed.is_file()
        || !imessage_dir.is_dir()
        || !sbr_dir.is_dir()
        || !whatsapp_dir.is_dir()
        || !contacts_vcf.is_file()
    {
        bail!(
            "incomplete demo bundle under {} (need config/config.toml, config/seed.toml, \
             staging/{IMESSAGE_SOURCE}/, staging/{SBR_SOURCE}/, staging/{WHATSAPP_SOURCE}/, config/contacts.vcf)",
            bundle.display()
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

    wipe_demo_account(&cfg, account_id)?;

    let imessage_assets = cfg
        .paths
        .assets_dir_for_account(account_id, IMESSAGE_SOURCE);
    let sbr_assets = cfg.paths.assets_dir_for_account(account_id, SBR_SOURCE);
    let whatsapp_assets = cfg
        .paths
        .assets_dir_for_account(account_id, WHATSAPP_SOURCE);
    let db = cfg.paths.db.clone();

    println!("Reset demo — importing");
    println!("  config:       {}", config_dest.display());
    println!("  account:      {}", account_id);
    println!("  imessage:     {}", imessage_dir.display());
    println!("  android:      {}", sbr_dir.display());
    println!("  whatsapp:     {}", whatsapp_dir.display());
    println!("  db:           {}", db.display());

    seed_demo_account(&db, account_id, &seed)?;

    let mut import_stats = import::import_export(
        &imessage_dir,
        &db,
        &imessage_assets,
        Some(&contacts_vcf),
        true,
        ImportMode::Replace,
        IMESSAGE_SOURCE,
        account_id,
    )?;

    let sbr_stats = import::import_export(
        &sbr_dir,
        &db,
        &sbr_assets,
        None,
        false,
        ImportMode::Append,
        SBR_SOURCE,
        account_id,
    )?;
    merge_import_stats(&mut import_stats, &sbr_stats);

    let wa_stats = import::import_export(
        &whatsapp_dir,
        &db,
        &whatsapp_assets,
        None,
        false,
        ImportMode::Append,
        WHATSAPP_SOURCE,
        account_id,
    )?;
    merge_import_stats(&mut import_stats, &wa_stats);

    let dedupe_stats = dedupe::run_dedupe(&db, account_id, 2)?;

    println!("Reset demo — processing assets");
    let process_stats = process_assets::run(
        &cfg,
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
    .context("process-assets after demo import")?;

    Ok(ResetDemoStats {
        seed: seed_stats,
        import: import_stats,
        dedupe_keys_filled: dedupe_stats.keys_filled,
        process_assets: process_stats,
    })
}

/// Regenerate from `demo_seed.toml` when present (dev checkout).
/// Release images only ship the committed `demo/` tree — skip regen there.
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
    // Optional email handles for recognizing “you” in messages — not for login.
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

    // Owner identity handles ("you" matching) live in `handles`, linked via `account_handles`.
    conn.execute(
        "DELETE FROM account_handles WHERE account_id = ?1",
        params![account_id],
    )?;
    for (raw, handle_type) in &seed.owner.handle_specs {
        account_profile::link_account_handle(&conn, account_id, raw, *handle_type)?;
    }

    Ok(())
}

/// Clear the demo account's vault rows (CASCADE) and on-disk assets.
/// Leaves `vault.db` and other accounts intact.
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

    /// The committed demo bundle ships a `seed.toml`; it must parse with the
    /// current `DemoOwner` (handle_specs) format or `reset-demo` fails on
    /// release images that skip bundle regeneration.
    #[test]
    fn committed_demo_seed_toml_parses() {
        let text = include_str!("../../../../demo/config/seed.toml");
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
}
