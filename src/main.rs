mod asset_uploads;
mod assets;
mod auth;
mod config;
mod contacts_api;
mod profile;
mod db;
mod dedupe;
mod export_api;
mod import;
mod import_cli;
mod import_media;
mod jsonl;
mod models;
mod process_assets;
mod reset_demo;
mod search_query;
mod server;

use crate::db::{account_profile, contacts as contacts_db};

use std::path::PathBuf;

use anyhow::{Result, bail};
use clap::{Parser, Subcommand};

use crate::config::{Config, validate_source_id};

#[derive(Parser)]
#[command(name = "message-vault-rs")]
#[command(about = "Import and view messages in SQLite")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Import a message-ir JSONL folder (source from export.source unless --source)
    Import {
        /// Optional source override (forces one source; skips IR export.source)
        #[arg(long)]
        source: Option<String>,

        /// Path to config.toml
        #[arg(long, default_value = "config/config.toml")]
        config: PathBuf,

        /// Folder of `*.jsonl` conversation files (+ attachments)
        #[arg(long = "input", visible_aliases = ["dir", "staging-dir", "export-dir"])]
        input: PathBuf,

        /// Output SQLite database path (overrides config)
        #[arg(long)]
        db: Option<PathBuf>,

        /// Originals asset store directory (overrides account/source default; fixed-source only)
        #[arg(long)]
        assets_dir: Option<PathBuf>,

        /// Address book to load: VCF or vCard CSV export
        #[arg(long = "contacts", alias = "contacts-csv")]
        contacts: Option<PathBuf>,

        /// Reload contacts from --contacts even if the table is non-empty
        #[arg(long)]
        overwrite_contacts: bool,

        /// Attachment handling: copy (default), none, convert, compress
        #[arg(long, default_value = "copy")]
        media: String,

        /// Import mode: replace (wipe sources found in input) or append
        #[arg(long, default_value = "replace")]
        mode: String,

        /// Skip the cross-source soft-dedupe pass after import
        #[arg(long)]
        skip_dedupe: bool,

        /// Near-time window in seconds for dedupe Pass B (default 2)
        #[arg(long, default_value_t = 2)]
        window_secs: i64,

        /// Account username or UUID (scopes import to this vault tenant)
        #[arg(long)]
        account: String,
    },

    /// Soft-hide the same SMS when it appears under more than one import source
    DedupeCrossSource {
        /// Path to config.toml
        #[arg(long, default_value = "config/config.toml")]
        config: PathBuf,

        /// Output SQLite database path (overrides config)
        #[arg(long)]
        db: Option<PathBuf>,

        /// Near-time window in seconds for Pass B (default 2)
        #[arg(long, default_value_t = 2)]
        window_secs: i64,

        /// Account username or UUID (scopes dedupe to this vault tenant)
        #[arg(long)]
        account: String,
    },
    ImportContacts {
        /// Path to config.toml
        #[arg(long, default_value = "config/config.toml")]
        config: PathBuf,

        /// Address book: VCF, or vCard CSV (First Name, Last Name, Phone columns)
        #[arg(long = "contacts", alias = "contacts-csv")]
        contacts: PathBuf,

        /// Output SQLite database path (overrides config)
        #[arg(long)]
        db: Option<PathBuf>,

        /// Account username or UUID (scopes contacts to this vault tenant)
        #[arg(long)]
        account: String,
    },
    /// Regenerate demo bundle, clear demo account data, import, and process assets
    ResetDemo {
        /// Demo bundle directory (rewritten by demo-seed, then imported)
        #[arg(long, default_value = "demo")]
        bundle: PathBuf,

        /// Active config path to overwrite (default config/config.toml)
        #[arg(long, default_value = "config/config.toml")]
        config: PathBuf,
    },

    /// Run HTTP ingest API (`POST /v1/import` with message-ir JSONL)
    Serve {
        /// Path to config.toml (must include `[server]` with `bind`)
        #[arg(long, default_value = "config/config.toml")]
        config: PathBuf,
    },

    /// Generate browser-friendly derived media under assets_converted/
    ProcessAssets {
        /// Path to config.toml
        #[arg(long, default_value = "config/config.toml")]
        config: PathBuf,

        /// Re-derive even when a derived file already exists
        #[arg(long)]
        force: bool,

        /// Convert and log without writing files or updating the DB
        #[arg(long)]
        dry_run: bool,

        /// Skip image conversion
        #[arg(long)]
        skip_image: bool,

        /// Skip video conversion
        #[arg(long)]
        skip_video: bool,

        /// Skip audio conversion
        #[arg(long)]
        skip_audio: bool,

        /// Override SQLite database path from config
        #[arg(long)]
        db: Option<PathBuf>,

        /// Only process this source id
        #[arg(long)]
        source: Option<String>,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Import {
            source,
            config,
            input,
            db,
            assets_dir,
            contacts,
            overwrite_contacts,
            media,
            mode,
            skip_dedupe,
            window_secs,
            account,
        } => {
            let cfg = Config::load(&config)?;
            if window_secs < 0 {
                bail!("--window-secs must be >= 0");
            }
            if let Some(ref source) = source {
                validate_source_id(source)?;
            }
            let mode = import::ImportMode::parse(&mode)?;
            let media = import_media::MediaMode::parse(&media)?;
            let db_path = db.clone().unwrap_or_else(|| cfg.paths.db.clone());
            let account = account_profile::resolve_account_ref_at(&db_path, &account)?;

            let stats = import_cli::run(
                &cfg,
                &import_cli::CliImportOptions {
                    account_id: account,
                    input_dir: input,
                    db_path: db,
                    assets_dir,
                    source_override: source,
                    mode,
                    media,
                    contacts,
                    overwrite_contacts,
                    skip_dedupe,
                    window_secs,
                },
            )?;

            println!();
            println!("Import into {}", db_path.display());
            println!("  input:         {}", stats.input_dir.display());
            println!("  sources:       {}", stats.sources.join(", "));
            if stats.import.contacts_skipped {
                println!(
                    "  contacts:      (skipped — already loaded or no --contacts; use --overwrite-contacts)"
                );
            } else {
                println!("  contacts:      {}", stats.import.contacts);
                println!("  contact handles:{}", stats.import.contact_handles);
                println!("  contact labels:{}", stats.import.contact_label_links);
            }
            println!("  files:         {}", stats.import.files);
            println!("  conversations: {}", stats.import.conversations);
            println!("  participants:  {}", stats.import.participants);
            println!("  messages:      {}", stats.import.messages);
            println!("  messages deduped: {}", stats.import.messages_deduped);
            if stats.import.mode == "append" {
                println!("  messages appended: {}", stats.import.messages_appended);
            }
            println!("  attachments:   {}", stats.import.attachments);
            println!("  tapbacks:      {}", stats.import.tapbacks);
            println!("  assets copied: {}", stats.import.assets_copied);
            println!("  assets deduped:{}", stats.import.assets_deduped);
            println!("  assets missing:{}", stats.import.assets_missing);
            if stats.import.unknown_contacts > 0 {
                println!(
                    "  unknown contacts created: {}",
                    stats.import.unknown_contacts
                );
            }
            if let Some(d) = stats.dedupe {
                println!("Cross-source dedupe");
                println!("  keys filled:   {}", d.keys_filled);
                println!("  exact groups:  {}", d.exact_groups);
                println!("  exact flagged: {}", d.exact_flagged);
                println!("  near flagged:  {}", d.near_flagged);
            } else {
                println!("Cross-source dedupe skipped (--skip-dedupe)");
            }
        }

        Commands::DedupeCrossSource {
            config,
            db,
            window_secs,
            account,
        } => {
            let cfg = Config::load(&config)?;
            let db = db.unwrap_or_else(|| cfg.paths.db.clone());
            let account = account_profile::resolve_account_ref_at(&db, &account)?;
            if window_secs < 0 {
                bail!("--window-secs must be >= 0");
            }

            let priority = {
                let conn = rusqlite::Connection::open(&db)?;
                conn.execute_batch("PRAGMA foreign_keys = ON;")?;
                dedupe::source_priority_from_db(&conn, &account)?
            };

            println!("Cross-source dedupe on {}", db.display());
            println!("  config:       {}", config.display());
            println!("  account:      {}", account);
            println!("  window_secs:  {}", window_secs);
            println!(
                "  priority:     {}",
                if priority.is_empty() {
                    "(none)".to_string()
                } else {
                    priority.join(", ")
                }
            );

            let stats = dedupe::run_dedupe(&db, &account, window_secs)?;
            println!("  keys filled:  {}", stats.keys_filled);
            println!("  exact groups: {}", stats.exact_groups);
            println!("  exact flagged:{}", stats.exact_flagged);
            println!("  near flagged: {}", stats.near_flagged);
        }

        Commands::ImportContacts {
            config,
            contacts,
            db,
            account,
        } => {
            let cfg = Config::load(&config)?;
            let db = db.unwrap_or_else(|| cfg.paths.db.clone());
            let account = account_profile::resolve_account_ref_at(&db, &account)?;

            if let Some(parent) = db.parent()
                && !parent.as_os_str().is_empty()
            {
                std::fs::create_dir_all(parent)?;
            }

            let mut conn = rusqlite::Connection::open(&db)?;
            conn.execute_batch("PRAGMA foreign_keys = ON;")?;
            let stats = contacts_db::load_contacts_if_needed(
                &mut conn,
                Some(&contacts),
                true,
                &account,
            )?;

            println!("Imported contacts into {}", db.display());
            println!("  config:       {}", config.display());
            println!("  account:      {}", account);
            println!("  contacts:     {}", contacts.display());
            println!("  rows:         {}", stats.contacts);
            println!("  phones:       {}", stats.phones);
            println!("  label links:  {}", stats.labels);
        }

        Commands::ResetDemo { bundle, config } => {
            let stats = reset_demo::run_reset_demo(&bundle, &config)?;
            println!();
            println!("Demo reset complete");
            if stats.seed.messages > 0 {
                println!("  generated msgs:  {}", stats.seed.messages);
            }
            println!("  conversations:   {}", stats.import.conversations);
            println!("  messages:        {}", stats.import.messages);
            println!("  attachments:     {}", stats.import.attachments);
            println!("  tapbacks:        {}", stats.import.tapbacks);
            println!("  contacts:        {}", stats.import.contacts);
            println!("  assets copied:   {}", stats.import.assets_copied);
            println!("  assets missing:  {}", stats.import.assets_missing);
            println!("  dedupe keys:     {}", stats.dedupe_keys_filled);
            println!("  derived media:   {}", stats.process_assets.derived);
            println!("  derive skipped:  {}", stats.process_assets.skipped);
            println!("  derive errors:   {}", stats.process_assets.errors);
        }

        Commands::Serve { config } => {
            let cfg = Config::load(&config)?;
            let _ = cfg.require_server()?;
            let rt = tokio::runtime::Runtime::new()?;
            rt.block_on(server::run(cfg))?;
        }

        Commands::ProcessAssets {
            config,
            force,
            dry_run,
            skip_image,
            skip_video,
            skip_audio,
            db,
            source,
        } => {
            let cfg = Config::load(&config)?;
            if let Some(ref source) = source {
                validate_source_id(source)?;
            }
            let _stats = process_assets::run(
                &cfg,
                &process_assets::ProcessAssetsOptions {
                    force,
                    dry_run,
                    skip_image,
                    skip_video,
                    skip_audio,
                    db,
                    source,
                },
            )?;
        }
    }

    Ok(())
}
