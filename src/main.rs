mod asset_uploads;
mod assets;
mod config;
mod db;
mod dedupe;
mod import;
mod jsonl;
mod models;
mod process_assets;
mod reset_demo;
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
    /// Import a message-ir JSONL folder for one source (soft-dedupe afterward by default)
    Import {
        /// Source id (lowercase slug; becomes messages.source and asset folder name)
        source: String,

        /// Path to config.toml
        #[arg(long, default_value = "config/config.toml")]
        config: PathBuf,

        /// Folder of `*.jsonl` conversation files (+ attachments)
        #[arg(long, visible_aliases = ["staging-dir", "export-dir"])]
        dir: PathBuf,

        /// Output SQLite database path (overrides config)
        #[arg(long)]
        db: Option<PathBuf>,

        /// Originals asset store directory (overrides account/source default)
        #[arg(long)]
        assets_dir: Option<PathBuf>,

        /// Address book to load: VCF or vCard CSV export
        #[arg(long = "contacts", alias = "contacts-csv")]
        contacts: Option<PathBuf>,

        /// Reload contacts from --contacts even if the table is non-empty
        #[arg(long)]
        overwrite_contacts: bool,

        /// Import mode: replace (wipe this source's messages) or append (dedupe by source+guid)
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
            dir,
            db,
            assets_dir,
            contacts,
            overwrite_contacts,
            mode,
            skip_dedupe,
            window_secs,
            account,
        } => {
            let cfg = Config::load(&config)?;
            if window_secs < 0 {
                bail!("--window-secs must be >= 0");
            }
            validate_source_id(&source)?;
            let db = db.unwrap_or_else(|| cfg.paths.db.clone());
            let account = account_profile::resolve_account_ref_at(&db, &account)?;
            let mode = import::ImportMode::parse(&mode)?;
            let assets =
                assets_dir.unwrap_or_else(|| cfg.paths.assets_dir_for_account(&account, &source));

            if !dir.is_dir() {
                bail!("import directory does not exist: {}", dir.display());
            }
            if !import_dir_has_jsonl(&dir)? {
                bail!(
                    "import directory {} has no .jsonl files (message-ir JSONL expected)",
                    dir.display()
                );
            }

            println!("Importing into {}", db.display());
            println!("  config:        {}", config.display());
            println!("  account:       {}", account);
            println!("  source:        {}", source);
            println!("  dir:           {}", dir.display());
            println!("  assets:        {}", assets.display());
            println!("  mode:          {}", mode.as_str());
            match &contacts {
                Some(path) => println!("  contacts:      {}", path.display()),
                None => println!("  contacts:      (none — use --contacts for VCF or vCard CSV)"),
            }

            let stats = import::import_export(
                &dir,
                &db,
                &assets,
                contacts.as_deref(),
                overwrite_contacts,
                mode,
                &source,
                &account,
            )?;

            println!();
            println!("Source '{}'", source);
            if stats.contacts_skipped {
                println!(
                    "  contacts:      (skipped — already loaded or no --contacts; use --overwrite-contacts)"
                );
            } else {
                println!("  contacts:      {}", stats.contacts);
                println!("  contact handles:{}", stats.contact_handles);
                println!("  contact labels:{}", stats.contact_label_links);
            }
            println!("  files:         {}", stats.files);
            println!("  conversations: {}", stats.conversations);
            println!("  participants:  {}", stats.participants);
            println!("  messages:      {}", stats.messages);
            println!("  messages deduped: {}", stats.messages_deduped);
            if stats.mode == "append" {
                println!("  messages appended: {}", stats.messages_appended);
            }
            println!("  attachments:   {}", stats.attachments);
            println!("  tapbacks:      {}", stats.tapbacks);
            println!("  assets copied: {}", stats.assets_copied);
            println!("  assets deduped:{}", stats.assets_deduped);
            println!("  assets missing:{}", stats.assets_missing);

            if skip_dedupe {
                println!("Cross-source dedupe skipped (--skip-dedupe)");
            } else {
                let d = dedupe::run_dedupe(&db, &account, window_secs)?;
                println!("Cross-source dedupe");
                println!("  keys filled:   {}", d.keys_filled);
                println!("  exact groups:  {}", d.exact_groups);
                println!("  exact flagged: {}", d.exact_flagged);
                println!("  near flagged:  {}", d.near_flagged);
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

fn import_dir_has_jsonl(dir: &std::path::Path) -> Result<bool> {
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("jsonl"))
        {
            return Ok(true);
        }
    }
    Ok(false)
}
