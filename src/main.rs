mod api_tokens;
mod assets;
mod config;
mod contacts;
mod dedupe;
mod exclude;
mod export_markdown;
mod import;
mod ingest;
mod jsonl;
mod models;
mod phone;
mod process_assets;
mod reset_demo;
mod schema;
mod server;
mod vault_owner;
mod vcf;
mod vcf_to_contacts;

use std::collections::HashMap;
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
    /// Import a Message Exporters JSONL staging folder for one source, then soft-dedupe.
    Ingest {
        /// Source id (lowercase slug; becomes messages.source and asset folder name)
        source: String,

        /// Path to config.toml
        #[arg(long, default_value = "config/config.toml")]
        config: PathBuf,

        /// Folder of `*.jsonl` conversation files (+ attachments)
        #[arg(long)]
        staging_dir: PathBuf,

        /// Import mode: replace (wipe this source's messages) or append
        #[arg(long, default_value = "replace")]
        mode: String,

        /// Address book to load: iMazing Contacts CSV or VCF
        #[arg(long = "contacts", alias = "contacts-csv")]
        contacts: Option<PathBuf>,

        /// Reload contacts from --contacts even if the table is non-empty
        #[arg(long)]
        overwrite_contacts: bool,

        /// Skip the cross-source soft-dedupe pass
        #[arg(long)]
        skip_dedupe: bool,

        /// Near-time window in seconds for Pass B (default 2)
        #[arg(long, default_value_t = 2)]
        window_secs: i64,

        /// Account username or UUID (scopes ingest to this vault tenant)
        #[arg(long)]
        account: String,
    },

    /// Import JSONL export(s) into SQLite
    Import {
        /// Path to config.toml
        #[arg(long, default_value = "config/config.toml")]
        config: PathBuf,

        /// Source id (lowercase slug)
        #[arg(long)]
        source: String,

        /// Directory containing `*.jsonl` conversation files
        #[arg(long)]
        export_dir: PathBuf,

        /// Output SQLite database path (overrides config)
        #[arg(long)]
        db: Option<PathBuf>,

        /// Originals asset store directory (overrides account/source default)
        #[arg(long)]
        assets_dir: Option<PathBuf>,

        /// Address book to load: iMazing Contacts CSV or VCF
        #[arg(long = "contacts", alias = "contacts-csv")]
        contacts: Option<PathBuf>,

        /// Exclude CSV path (overrides per-account default)
        #[arg(long = "exclude-csv")]
        exclude_csv: Option<PathBuf>,

        /// Delete and reload contacts from --contacts even if the table is non-empty
        #[arg(long)]
        overwrite_contacts: bool,

        /// Import mode: replace (wipe this source's messages) or append (dedupe by source+guid)
        #[arg(long, default_value = "replace")]
        mode: String,

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

        /// Address book: iMazing Contacts CSV (First Name, Last Name, Phone columns) or VCF
        #[arg(long = "contacts", alias = "contacts-csv")]
        contacts: PathBuf,

        /// Output SQLite database path (overrides config)
        #[arg(long)]
        db: Option<PathBuf>,

        /// Account username or UUID (scopes contacts to this vault tenant)
        #[arg(long)]
        account: String,
    },
    ExportMarkdown {
        /// Output directory (required; written fresh under this path)
        #[arg(long)]
        out: PathBuf,

        /// Path to config.toml
        #[arg(long, default_value = "config/config.toml")]
        config: PathBuf,

        /// SQLite database path (overrides config)
        #[arg(long)]
        db: Option<PathBuf>,

        /// Path to Obsidian bubble CSS snippet (default config/obsidian-message-vault.css)
        #[arg(long)]
        snippet_css: Option<PathBuf>,

        /// Account username or UUID (scopes export to this vault tenant)
        #[arg(long)]
        account: String,
    },
    VcfToContacts {
        /// Path to config.toml
        #[arg(long, default_value = "config/config.toml")]
        config: PathBuf,

        /// Input contacts.vcf
        #[arg(long)]
        vcf: PathBuf,

        /// Output contacts.csv (defaults to data/<account>/contacts.csv when --account is set)
        #[arg(long)]
        out: Option<PathBuf>,

        /// Optional exclude.csv (defaults to data/<account>/exclude.csv when --account is set)
        #[arg(long = "exclude")]
        exclude: Option<PathBuf>,

        /// Account username or UUID (used for default --out / --exclude paths)
        #[arg(long)]
        account: Option<String>,

        /// Overwrite --out if it already exists
        #[arg(long)]
        force: bool,
    },

    /// Restore committed demo bundle: copy config, wipe DB, re-import iMessage staging
    ResetDemo {
        /// Demo bundle directory
        #[arg(long, default_value = "demo")]
        bundle: PathBuf,

        /// Active config path to overwrite (default config/config.toml)
        #[arg(long, default_value = "config/config.toml")]
        config: PathBuf,
    },

    /// Run HTTP ingest API (`POST /v1/import` with vault JSONL)
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
        Commands::Ingest {
            source,
            config,
            staging_dir,
            mode,
            contacts,
            overwrite_contacts,
            skip_dedupe,
            window_secs,
            account,
        } => {
            let cfg = Config::load(&config)?;
            if window_secs < 0 {
                bail!("--window-secs must be >= 0");
            }
            let mode = import::ImportMode::parse(&mode)?;
            validate_source_id(&source)?;
            let account = vault_owner::resolve_account_ref_at(&cfg.paths.db, &account)?;

            let stats = ingest::ingest(
                &cfg,
                &ingest::IngestOptions {
                    source_id: source,
                    account_id: account,
                    staging_dir,
                    mode,
                    contacts,
                    overwrite_contacts,
                    skip_dedupe,
                    window_secs,
                },
            )?;

            println!();
            println!("Import into {}", cfg.paths.db.display());
            println!("  staging:       {}", stats.staging_dir.display());
            println!("  files:         {}", stats.import.files);
            println!("  conversations: {}", stats.import.conversations);
            println!("  messages:      {}", stats.import.messages);
            println!("  messages deduped: {}", stats.import.messages_deduped);
            if stats.import.mode == "append" {
                println!("  messages appended: {}", stats.import.messages_appended);
            }
            println!("  attachments:   {}", stats.import.attachments);
            println!("  assets copied: {}", stats.import.assets_copied);
            println!("  assets missing:{}", stats.import.assets_missing);
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

        Commands::Import {
            config,
            source,
            export_dir,
            db,
            assets_dir,
            contacts,
            exclude_csv,
            overwrite_contacts,
            mode,
            account,
        } => {
            let cfg = Config::load(&config)?;
            validate_source_id(&source)?;
            let db = db.unwrap_or_else(|| cfg.paths.db.clone());
            let account = vault_owner::resolve_account_ref_at(&db, &account)?;
            let (mirror_csv, default_exclude) = cfg.paths.ensure_account_csvs(&account)?;
            let exclude_csv = exclude_csv.unwrap_or(default_exclude);
            let mode = import::ImportMode::parse(&mode)?;
            let assets =
                assets_dir.unwrap_or_else(|| cfg.paths.assets_dir_for_account(&account, &source));

            println!("Importing into {}", db.display());
            println!("  config:        {}", config.display());
            println!("  account:       {}", account);
            println!("  source:        {}", source);
            println!("  mode:          {}", mode.as_str());
            match &contacts {
                Some(path) => println!("  contacts:      {}", path.display()),
                None => println!("  contacts:      (none — use --contacts for iMazing CSV or VCF)"),
            }
            println!("  exclude csv:   {}", exclude_csv.display());

            let stats = import::import_export(
                &export_dir,
                &db,
                &assets,
                contacts.as_deref(),
                &mirror_csv,
                &exclude_csv,
                overwrite_contacts,
                mode,
                &source,
                &account,
            )?;

            println!();
            println!("Source '{}'", source);
            println!("  export_dir:    {}", export_dir.display());
            println!("  assets:        {}", assets.display());
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
            println!("  excl. convos:  {}", stats.conversations_excluded);
            println!("  excl. msgs:    {}", stats.messages_excluded);
            println!("  excl. parts:   {}", stats.participants_excluded);
            println!("  assets copied: {}", stats.assets_copied);
            println!("  assets deduped:{}", stats.assets_deduped);
            println!("  assets missing:{}", stats.assets_missing);
        }

        Commands::DedupeCrossSource {
            config,
            db,
            window_secs,
            account,
        } => {
            let cfg = Config::load(&config)?;
            let db = db.unwrap_or_else(|| cfg.paths.db.clone());
            let account = vault_owner::resolve_account_ref_at(&db, &account)?;
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
            let account = vault_owner::resolve_account_ref_at(&db, &account)?;

            if let Some(parent) = db.parent()
                && !parent.as_os_str().is_empty()
            {
                std::fs::create_dir_all(parent)?;
            }

            let mut conn = rusqlite::Connection::open(&db)?;
            conn.execute_batch("PRAGMA foreign_keys = ON;")?;
            let stats =
                contacts::load_contacts_if_needed(&mut conn, Some(&contacts), true, &account)?;

            println!("Imported contacts into {}", db.display());
            println!("  config:       {}", config.display());
            println!("  account:      {}", account);
            println!("  contacts:     {}", contacts.display());
            println!("  rows:         {}", stats.contacts);
            println!("  phones:       {}", stats.phones);
            println!("  label links:  {}", stats.labels);
        }

        Commands::ExportMarkdown {
            out,
            config,
            db,
            snippet_css,
            account,
        } => {
            let cfg = Config::load(&config)?;
            let db = db.unwrap_or_else(|| cfg.paths.db.clone());
            let account = vault_owner::resolve_account_ref_at(&db, &account)?;
            let snippet_css =
                snippet_css.unwrap_or_else(|| PathBuf::from("config/obsidian-message-vault.css"));
            if !snippet_css.is_file() {
                bail!(
                    "CSS snippet not found at {} (pass --snippet-css)",
                    snippet_css.display()
                );
            }

            let sources = {
                let conn = rusqlite::Connection::open(&db)?;
                conn.execute_batch("PRAGMA foreign_keys = ON;")?;
                list_message_sources(&conn, &account)?
            };
            let mut assets_by_source = HashMap::new();
            for src in &sources {
                assets_by_source
                    .insert(src.clone(), cfg.paths.assets_dir_for_account(&account, src));
            }

            println!("Export markdown → {}", out.display());
            println!("  config:  {}", config.display());
            println!("  account: {}", account);
            println!("  db:      {}", db.display());
            println!("  snippet: {}", snippet_css.display());

            let stats =
                export_markdown::run_export(&db, &account, &assets_by_source, &out, &snippet_css)?;
            println!("  people:        {}", stats.people);
            println!("  year pages:    {}", stats.year_pages);
            println!("  messages:      {}", stats.messages);
            println!("  assets copied: {}", stats.assets_copied);
            println!("  assets missing:{}", stats.assets_missing);
            println!(
                "Enable CSS snippet message-vault-bubbles in Obsidian (Settings → Appearance)."
            );
        }

        Commands::VcfToContacts {
            config,
            vcf,
            out,
            exclude,
            account,
            force,
        } => {
            let cfg = Config::load(&config)?;
            let (out, exclude) = match (out, account.as_deref()) {
                (Some(out), _) => (out, exclude),
                (None, Some(account_ref)) => {
                    let account_id =
                        vault_owner::resolve_account_ref_at(&cfg.paths.db, account_ref)?;
                    let (contacts, excl) = cfg.paths.ensure_account_csvs(&account_id)?;
                    (contacts, exclude.or(Some(excl)))
                }
                (None, None) => {
                    bail!("pass --out <contacts.csv> or --account <username|uuid>");
                }
            };

            let stats = vcf_to_contacts::convert(&vcf, &out, exclude.as_deref(), force)?;
            println!("Wrote {}", out.display());
            println!("  vcf cards:        {}", stats.cards_total);
            println!("  skipped (no TEL): {}", stats.cards_skipped_no_tel);
            println!("  exclude-only:     {}", stats.exclude_only);
            println!("  contacts written: {}", stats.contacts_written);
        }

        Commands::ResetDemo { bundle, config } => {
            let stats = reset_demo::run_reset_demo(&bundle, &config)?;
            println!();
            println!("Demo reset complete");
            println!("  conversations: {}", stats.import.conversations);
            println!("  messages:      {}", stats.import.messages);
            println!("  attachments:   {}", stats.import.attachments);
            println!("  tapbacks:      {}", stats.import.tapbacks);
            println!("  contacts:      {}", stats.import.contacts);
            println!("  assets copied: {}", stats.import.assets_copied);
            println!("  assets missing:{}", stats.import.assets_missing);
            println!("  dedupe keys:   {}", stats.dedupe_keys_filled);
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

fn list_message_sources(conn: &rusqlite::Connection, account_id: &str) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(
        r#"
        SELECT DISTINCT m.source
        FROM messages m
        JOIN conversations c ON c.id = m.conversation_id
        WHERE c.account_id = ?1
          AND m.source IS NOT NULL
          AND TRIM(m.source) != ''
        ORDER BY m.source
        "#,
    )?;
    let rows = stmt.query_map(rusqlite::params![account_id], |row| row.get::<_, String>(0))?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}
