//! Local JSONL import for one account + source folder.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use crate::config::{Config, validate_source_id};
use crate::dedupe::{self, DedupeStats};
use crate::import::{self, ImportMode};

#[derive(Debug, Clone)]
pub struct IngestOptions {
    pub source_id: String,
    pub account_id: String,
    /// Required folder of `*.jsonl` conversation files (+ attachments).
    pub staging_dir: PathBuf,
    pub mode: ImportMode,
    /// Optional address book: iMazing Contacts CSV or VCF.
    pub contacts: Option<PathBuf>,
    pub overwrite_contacts: bool,
    pub skip_dedupe: bool,
    pub window_secs: i64,
}

#[derive(Debug)]
pub struct IngestStats {
    pub staging_dir: PathBuf,
    pub import: import::ImportStats,
    pub dedupe: Option<DedupeStats>,
}

pub fn ingest(cfg: &Config, opts: &IngestOptions) -> Result<IngestStats> {
    validate_source_id(&opts.source_id)?;
    let staging = &opts.staging_dir;
    if !staging.is_dir() {
        bail!("staging directory does not exist: {}", staging.display());
    }

    let has_jsonl = staging_has_ext(staging, "jsonl")?;
    if !has_jsonl {
        bail!(
            "staging {} has no .jsonl files (Message Exporters JSONL export expected)",
            staging.display()
        );
    }

    let assets_dir = cfg
        .paths
        .assets_dir_for_account(&opts.account_id, &opts.source_id);

    println!("Ingest");
    println!("  source:       {}", opts.source_id);
    println!("  account:      {}", opts.account_id);
    println!("  staging:      {}", staging.display());
    println!("  db:           {}", cfg.paths.db.display());
    println!("  assets_dir:   {}", assets_dir.display());
    println!("  mode:         {}", opts.mode.as_str());
    match &opts.contacts {
        Some(path) => println!("  contacts:     {}", path.display()),
        None => println!("  contacts:     (none)"),
    }

    let import_stats = import::import_export(
        staging,
        &cfg.paths.db,
        &assets_dir,
        opts.contacts.as_deref(),
        opts.overwrite_contacts,
        opts.mode,
        &opts.source_id,
        &opts.account_id,
    )?;

    let dedupe = if opts.skip_dedupe {
        None
    } else {
        let dedupe_stats = dedupe::run_dedupe(&cfg.paths.db, &opts.account_id, opts.window_secs)?;
        println!(
            "  dedupe:       keys_filled={} exact_flagged={} near_flagged={}",
            dedupe_stats.keys_filled, dedupe_stats.exact_flagged, dedupe_stats.near_flagged
        );
        Some(dedupe_stats)
    };

    Ok(IngestStats {
        staging_dir: staging.clone(),
        import: import_stats,
        dedupe,
    })
}

fn staging_has_ext(staging: &Path, ext: &str) -> Result<bool> {
    for entry in fs::read_dir(staging)
        .with_context(|| format!("failed to read staging {}", staging.display()))?
    {
        let path = entry?.path();
        if path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case(ext))
        {
            return Ok(true);
        }
    }
    Ok(false)
}
