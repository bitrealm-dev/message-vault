//! CLI directory import: any JSONL folder; source from IR `export.source` unless overridden.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use crate::config::{Config, validate_source_id};
use crate::db::account_profile;
use crate::db::schema;
use crate::db::vault_imports;
use crate::dedupe::{self, DedupeStats};
use crate::import::{self, ImportMode, ImportOptions, ImportStats};
use crate::import_media::MediaMode;
use crate::jsonl;
use crate::models::ExportRecord;

#[derive(Debug, Clone)]
pub struct CliImportOptions {
    pub account_id: String,
    pub input_dir: PathBuf,
    pub db_path: Option<PathBuf>,
    pub assets_dir: Option<PathBuf>,
    /// When set, force this source for every conversation (ignore IR export.source).
    pub source_override: Option<String>,
    pub mode: ImportMode,
    pub media: MediaMode,
    pub contacts: Option<PathBuf>,
    pub overwrite_contacts: bool,
    pub skip_dedupe: bool,
    pub window_secs: i64,
}

#[derive(Debug)]
pub struct CliImportStats {
    pub input_dir: PathBuf,
    pub sources: Vec<String>,
    pub import: ImportStats,
    pub dedupe: Option<DedupeStats>,
}

pub fn run(cfg: &Config, opts: &CliImportOptions) -> Result<CliImportStats> {
    let input = &opts.input_dir;
    if !input.is_dir() {
        bail!("input directory does not exist: {}", input.display());
    }

    let paths = list_jsonl_files(input)?;
    if paths.is_empty() {
        bail!("input {} has no .jsonl files", input.display());
    }

    let db_path = opts.db_path.clone().unwrap_or_else(|| cfg.paths.db.clone());
    let account_id = opts.account_id.clone();

    let (sources, source_from_jsonl, wipe_sources) =
        if let Some(ref override_source) = opts.source_override {
            validate_source_id(override_source)?;
            (
                vec![override_source.clone()],
                false,
                Some(vec![override_source.clone()]),
            )
        } else {
            let discovered = discover_sources(&paths)?;
            if discovered.is_empty() {
                bail!(
                    "no conversation export.source found in {}; each conversation needs \
                 export.source in the message-ir header (or pass --source)",
                    input.display()
                );
            }
            for source in &discovered {
                validate_source_id(source)?;
            }
            (discovered.clone(), true, Some(discovered))
        };

    println!("Import");
    println!("  account:      {}", account_id);
    println!("  input:        {}", input.display());
    println!("  db:           {}", db_path.display());
    println!("  sources:      {}", sources.join(", "));
    if source_from_jsonl {
        println!("  source mode:  from JSONL export.source");
    } else {
        println!("  source mode:  --source override");
    }
    println!("  mode:         {}", opts.mode.as_str());
    println!("  media:        {}", opts.media.as_str());
    match &opts.contacts {
        Some(path) => println!("  contacts:     {}", path.display()),
        None => println!("  contacts:     (none — use --contacts for VCF or vCard CSV)"),
    }

    let placeholder_assets = opts.assets_dir.clone().unwrap_or_else(|| {
        cfg.paths
            .assets_dir_for_account(&account_id, sources.first().expect("sources non-empty"))
    });

    let session_source = sources.join(",");
    let mut conn = schema::open_configured(&db_path)
        .with_context(|| format!("failed to open database {}", db_path.display()))?;
    schema::ensure_vault_schema(&conn)?;
    account_profile::ensure_account_row(&conn, &account_id)?;

    let import_id = vault_imports::start_import(
        &conn,
        &account_id,
        &session_source,
        opts.mode.as_str(),
        Some("message-vault-server"),
    )?;

    let import_opts = ImportOptions {
        db_path: &db_path,
        assets_dir: &placeholder_assets,
        asset_root: input,
        contacts: opts.contacts.as_deref(),
        overwrite_contacts: opts.overwrite_contacts,
        mode: opts.mode,
        source: opts.source_override.as_deref().unwrap_or(""),
        account_id: &account_id,
        fill_content_keys: true,
        import_id: Some(import_id),
        source_from_jsonl,
        paths: source_from_jsonl.then_some(&cfg.paths),
        media: opts.media,
        wipe_sources: wipe_sources.clone(),
        contact_name_mode: import::ContactNameMode::default(),
    };

    let result = import::import_jsonl_files_on_conn(
        &mut conn,
        &paths,
        &import_opts,
        import::ImportSchemaMode::AssumeReady,
    );

    let complete_args = match &result {
        Ok(stats) => {
            vault_imports::CompleteImportArgs::succeeded(stats.messages, stats.attachments)
        }
        Err(_) => vault_imports::CompleteImportArgs::failed(),
    };
    vault_imports::complete_import_or_warn(&conn, &account_id, import_id, &complete_args);
    let import_stats = result?;
    drop(conn);

    let dedupe = if opts.skip_dedupe {
        None
    } else {
        let dedupe_stats = dedupe::run_dedupe(&db_path, &account_id, opts.window_secs)?;
        println!(
            "  dedupe:       fingerprints_set={} exact_hidden={} near_flagged={} (fingerprints are one per message, not duplicates)",
            dedupe_stats.keys_filled, dedupe_stats.exact_flagged, dedupe_stats.near_flagged
        );
        Some(dedupe_stats)
    };

    Ok(CliImportStats {
        input_dir: input.clone(),
        sources,
        import: import_stats,
        dedupe,
    })
}

/// Every `.jsonl` file directly inside `dir`, sorted by path.
pub fn list_jsonl_files(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut paths: Vec<PathBuf> = fs::read_dir(dir)
        .with_context(|| format!("failed to read {}", dir.display()))?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .and_then(|ext| ext.to_str())
                .is_some_and(|ext| ext.eq_ignore_ascii_case("jsonl"))
        })
        .collect();
    paths.sort();
    Ok(paths)
}

/// Collect distinct IR `export.source` values from conversation headers.
pub fn discover_sources(paths: &[PathBuf]) -> Result<Vec<String>> {
    let mut set = std::collections::BTreeSet::new();
    for path in paths {
        let records = jsonl::read_records(path)?;
        let mut saw_conversation = false;
        for record in records {
            if let ExportRecord::Conversation(c) = record {
                saw_conversation = true;
                let Some(source) = c
                    .export_source
                    .as_deref()
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                else {
                    bail!(
                        "{}: conversation '{}' is missing export.source \
                         (required for CLI directory import; or pass --source)",
                        path.display(),
                        c.chat_identifier
                    );
                };
                set.insert(source.to_string());
            }
        }
        let is_orphaned = import::is_orphaned_export(path);
        if !saw_conversation && !is_orphaned {
            bail!(
                "{}: no conversation header (cannot determine export.source)",
                path.display()
            );
        }
        if !saw_conversation && is_orphaned {
            bail!(
                "{}: orphaned.jsonl without a conversation header cannot supply export.source; \
                 pass --source, or add a conversation header with export.source",
                path.display()
            );
        }
    }
    Ok(set.into_iter().collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn discover_sources_from_ir_headers() {
        let tmp = TempDir::new().unwrap();
        fs::write(
            tmp.path().join("a.jsonl"),
            r#"{"schema_version":3,"export":{"source":"imessage","tool":"t","tool_version":"0","owner_handle":null,"owner_display_name":null},"conversation":{"chat_identifier":"+1","conversation_type":"individual","group_title":null,"participants":[],"stats":{"message_count":0,"attachment_count":0,"first_timestamp_unix_ms":null,"last_timestamp_unix_ms":null}}}
"#,
        )
        .unwrap();
        fs::write(
            tmp.path().join("b.jsonl"),
            r#"{"schema_version":3,"export":{"source":"go-sms-pro","tool":"t","tool_version":"0","owner_handle":null,"owner_display_name":null},"conversation":{"chat_identifier":"+2","conversation_type":"individual","group_title":null,"participants":[],"stats":{"message_count":0,"attachment_count":0,"first_timestamp_unix_ms":null,"last_timestamp_unix_ms":null}}}
"#,
        )
        .unwrap();
        let paths = list_jsonl_files(tmp.path()).unwrap();
        let sources = discover_sources(&paths).unwrap();
        assert_eq!(
            sources,
            vec!["go-sms-pro".to_string(), "imessage".to_string()]
        );
    }
}
