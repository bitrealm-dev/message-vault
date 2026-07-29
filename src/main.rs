mod api_tokens;
mod assets;
mod config;
mod contacts;
mod dedupe;
mod exclude;
mod export_markdown;
mod import;
mod ingest;
mod models;
mod ndjson;
mod phone;
mod reset_demo;
mod schema;
mod server;
mod vault_owner;
mod vcf;
mod vcf_to_contacts;

use std::path::PathBuf;

use anyhow::{bail, Result};
use clap::{Parser, Subcommand};

use crate::config::Config;

#[derive(Parser)]
#[command(name = "message-vault-rs")]
#[command(about = "Import and view messages in SQLite")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Import staging for a source (optional CSV→vault NDJSON), then soft-dedupe.
    /// Fill `export_dir` / staging via message-exporters first.
    Ingest {
        /// Configured source id (imessage, go-sms-pro, sms-backup-plus, sms-backup-restore, …)
        source: String,

        /// Path to config.toml
        #[arg(long, default_value = "config/config.toml")]
        config: PathBuf,

        /// Override staging dir (defaults to the source's export_dir)
        #[arg(long)]
        staging_dir: Option<PathBuf>,

        /// Import mode: replace (wipe this source's messages) or append
        #[arg(long, default_value = "replace")]
        mode: String,

        /// Reload contacts CSV even if the table is non-empty
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

    /// Import NDJSON export(s) into SQLite
    Import {
        /// Path to config.toml
        #[arg(long, default_value = "config/config.toml")]
        config: PathBuf,

        /// Import one configured source by id
        #[arg(long)]
        source: Option<String>,

        /// Import every configured source
        #[arg(long)]
        all: bool,

        /// Directory containing NDJSON conversation files (overrides selected source export_dir)
        #[arg(long)]
        export_dir: Option<PathBuf>,

        /// Output SQLite database path (overrides config)
        #[arg(long)]
        db: Option<PathBuf>,

        /// Originals asset store directory (overrides selected source assets dir)
        #[arg(long)]
        assets_dir: Option<PathBuf>,

        /// Contacts CSV path (overrides config; default config/contacts.csv)
        #[arg(long)]
        contacts_csv: Option<PathBuf>,

        /// Exclude CSV path (overrides config; handles skipped on import)
        #[arg(long = "exclude-csv")]
        exclude_csv: Option<PathBuf>,

        /// Delete and reload contacts from CSV even if the table is non-empty
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

        /// Contacts CSV path (overrides config; default config/contacts.csv)
        #[arg(long)]
        contacts_csv: Option<PathBuf>,

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
        /// Path to config.toml (used for default --out / --exclude)
        #[arg(long, default_value = "config/config.toml")]
        config: PathBuf,

        /// Input contacts.vcf
        #[arg(long)]
        vcf: PathBuf,

        /// Output contacts.csv (defaults to paths.contacts_csv from config)
        #[arg(long)]
        out: Option<PathBuf>,

        /// Optional exclude.csv (sets exclude=true; defaults to paths.exclude_csv)
        #[arg(long = "exclude")]
        exclude: Option<PathBuf>,

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

    /// Run HTTP ingest API (`POST /v1/import` with vault NDJSON)
    Serve {
        /// Path to config.toml (must include `[server]` with `api_token`)
        #[arg(long, default_value = "config/config.toml")]
        config: PathBuf,
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
            let _ = cfg.source(&source)?;
            let account = vault_owner::resolve_account_ref_at(&cfg.paths.db, &account)?;

            let stats = ingest::ingest(
                &cfg,
                &ingest::IngestOptions {
                    source_id: source,
                    account_id: account,
                    staging_dir,
                    mode,
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
            all,
            export_dir,
            db,
            assets_dir,
            contacts_csv,
            exclude_csv,
            overwrite_contacts,
            mode,
            account,
        } => {
            let cfg = Config::load(&config)?;
            let db = db.unwrap_or_else(|| cfg.paths.db.clone());
            let account = vault_owner::resolve_account_ref_at(&db, &account)?;
            let (default_contacts, default_exclude) = cfg.paths.ensure_account_csvs(&account)?;
            let contacts_csv = contacts_csv.unwrap_or(default_contacts);
            let exclude_csv = exclude_csv.unwrap_or(default_exclude);
            let mode = import::ImportMode::parse(&mode)?;

            if all && source.is_some() {
                bail!("use either --source <id> or --all, not both");
            }

            let sources: Vec<&config::SourceConfig> = if all {
                cfg.sources.iter().collect()
            } else if let Some(id) = source.as_deref() {
                vec![cfg.source(id)?]
            } else if cfg.sources.len() == 1 {
                vec![&cfg.sources[0]]
            } else {
                bail!(
                    "multiple sources configured; pass --source <id> or --all (ids: {})",
                    cfg.sources
                        .iter()
                        .map(|s| s.id.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                );
            };

            if sources.len() > 1 && (export_dir.is_some() || assets_dir.is_some()) {
                bail!("--export-dir / --assets-dir only apply when importing a single source");
            }

            println!("Importing into {}", db.display());
            println!("  config:        {}", config.display());
            println!("  account:       {}", account);
            println!("  mode:          {}", mode.as_str());
            if let Some(owner) = &cfg.owner {
                println!(
                    "  owner (legacy):{} ({})",
                    owner.display_name,
                    owner.phones.join(", ")
                );
            }
            println!("  contacts csv:  {}", contacts_csv.display());
            println!("  exclude csv:   {}", exclude_csv.display());

            let mut overwrite = overwrite_contacts;
            for src in sources {
                let export = export_dir
                    .clone()
                    .unwrap_or_else(|| src.export_dir.clone());
                let assets = assets_dir.clone().unwrap_or_else(|| {
                    src.resolved_assets_dir_for_account(&cfg.paths, &account)
                });

                let stats = import::import_export(
                    &export,
                    &db,
                    &assets,
                    &contacts_csv,
                    &exclude_csv,
                    overwrite,
                    mode,
                    &src.id,
                    &account,
                )?;
                // Only overwrite contacts on the first source of a batch.
                overwrite = false;

                println!();
                println!("Source '{}'", src.id);
                println!("  export_dir:    {}", export.display());
                println!("  assets:        {}", assets.display());
                if stats.contacts_skipped {
                    println!("  contacts:      (skipped — already loaded; use --overwrite-contacts)");
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
            let priority: Vec<String> = cfg.sources.iter().map(|s| s.id.clone()).collect();
            if window_secs < 0 {
                bail!("--window-secs must be >= 0");
            }

            println!("Cross-source dedupe on {}", db.display());
            println!("  config:       {}", config.display());
            println!("  account:      {}", account);
            println!("  window_secs:  {}", window_secs);
            println!("  priority:     {}", priority.join(", "));

            let stats = dedupe::run_dedupe(&db, &account, &priority, window_secs)?;
            println!("  keys filled:  {}", stats.keys_filled);
            println!("  exact groups: {}", stats.exact_groups);
            println!("  exact flagged:{}", stats.exact_flagged);
            println!("  near flagged: {}", stats.near_flagged);
        }

        Commands::ImportContacts {
            config,
            contacts_csv,
            db,
            account,
        } => {
            let cfg = Config::load(&config)?;
            let db = db.unwrap_or_else(|| cfg.paths.db.clone());
            let account = vault_owner::resolve_account_ref_at(&db, &account)?;
            let (default_contacts, _) = cfg.paths.ensure_account_csvs(&account)?;
            let contacts_csv = contacts_csv.unwrap_or(default_contacts);

            if let Some(parent) = db.parent() {
                if !parent.as_os_str().is_empty() {
                    std::fs::create_dir_all(parent)?;
                }
            }

            let mut conn = rusqlite::Connection::open(&db)?;
            conn.execute_batch("PRAGMA foreign_keys = ON;")?;
            let stats =
                contacts::load_contacts_if_needed(&mut conn, &contacts_csv, true, &account)?;

            println!("Imported contacts into {}", db.display());
            println!("  config:       {}", config.display());
            println!("  account:      {}", account);
            println!("  contacts csv: {}", contacts_csv.display());
            println!("  contacts:     {}", stats.contacts);
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
            let snippet_css = snippet_css.unwrap_or_else(|| {
                PathBuf::from("config/obsidian-message-vault.css")
            });
            if !snippet_css.is_file() {
                bail!(
                    "CSS snippet not found at {} (pass --snippet-css)",
                    snippet_css.display()
                );
            }

            let mut assets_by_source = std::collections::HashMap::new();
            for src in &cfg.sources {
                assets_by_source.insert(
                    src.id.clone(),
                    src.resolved_assets_dir_for_account(&cfg.paths, &account),
                );
            }

            println!("Export markdown → {}", out.display());
            println!("  config:  {}", config.display());
            println!("  account: {}", account);
            println!("  db:      {}", db.display());
            println!("  snippet: {}", snippet_css.display());

            let stats = export_markdown::run_export(
                &db,
                &account,
                &assets_by_source,
                &out,
                &snippet_css,
            )?;
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
            force,
        } => {
            let cfg = Config::load(&config)?;
            let out = out.unwrap_or(cfg.paths.contacts_csv);
            let exclude = exclude.or(Some(cfg.paths.exclude_csv));

            let stats = vcf_to_contacts::convert(
                &vcf,
                &out,
                exclude.as_deref(),
                force,
            )?;
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
    }

    Ok(())
}
