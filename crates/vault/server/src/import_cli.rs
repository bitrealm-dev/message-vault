//! CLI directory import: any JSONL folder; source from IR `export.source` unless overridden.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use crate::config::{Config, validate_source_id};
use crate::db::account_profile;
use crate::db::engine;
use crate::db::schema;
use crate::db::vault_imports;
use crate::dedupe::{self, DedupeStats};
use crate::import::{self, ImportMode, ImportOptions, ImportStats};
use crate::import_media::MediaMode;
use crate::jsonl;
use crate::models::ExportRecord;

/// Options for a CLI directory import.
#[derive(Debug, Clone)]
pub struct CliImportOptions {
    /// Vault account the import writes into.
    pub account_id: String,
    /// Folder of `*.jsonl` conversation files (+ attachments).
    pub input_dir: PathBuf,
    /// Database path override; falls back to config when `None`.
    pub db_path: Option<PathBuf>,
    /// Database URL override (`sqlite:...` / `postgres://...`); wins over `db_path`.
    pub db_url: Option<String>,
    /// Originals asset store override; per-account default when `None`.
    pub assets_dir: Option<PathBuf>,
    /// When set, force this source for every conversation (ignore IR export.source).
    pub source_override: Option<String>,
    /// Import mode: replace or append.
    pub mode: ImportMode,
    /// Attachment handling mode: copy, none, convert, compress.
    pub media: MediaMode,
    /// Optional address book to load: VCF or vCard CSV export.
    pub contacts: Option<PathBuf>,
    /// Reload contacts even when the table is non-empty.
    pub overwrite_contacts: bool,
    /// Skip the cross-source soft-dedupe pass after import.
    pub skip_dedupe: bool,
    /// Near-time window in seconds for dedupe Pass B.
    pub window_secs: i64,
}

/// Counts and inputs reported by a CLI directory import.
#[derive(Debug)]
pub struct CliImportStats {
    /// Input folder that was imported.
    pub input_dir: PathBuf,
    /// Source ids written (one per conversation unless overridden).
    pub sources: Vec<String>,
    /// Import stage counts.
    pub import: ImportStats,
    /// Dedupe counts when the pass ran, `None` when skipped.
    pub dedupe: Option<DedupeStats>,
}

/// Import a folder of JSON Lines files into the vault, then optionally run
/// cross-source duplicate hiding.
///
/// # Errors
///
/// Returns an error when the input directory is missing, has no `.jsonl`
/// files, or import / duplicate detection fails.
pub async fn run(cfg: &Config, opts: &CliImportOptions) -> Result<CliImportStats> {
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
    match opts.db_url.as_deref() {
        Some(url) => println!("  db:           {}", redact_db_url(url)),
        None => println!("  db:           {}", db_path.display()),
    }
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
    let pool = match opts.db_url.as_deref() {
        Some(url) => engine::open_pool_from_url(url)
            .await
            .with_context(|| format!("failed to open database at {}", redact_db_url(url)))?,
        None => engine::open_pool_for_path(&db_path)
            .await
            .with_context(|| format!("failed to open database {}", db_path.display()))?,
    };
    let mut conn = pool.acquire().await?;
    schema::ensure_vault_schema(&mut conn).await?;
    account_profile::ensure_account_row(&mut conn, &account_id).await?;

    let import_id = vault_imports::start_import(
        &mut conn,
        &vault_imports::StartImportArgs {
            account_id: &account_id,
            source: &session_source,
            mode: opts.mode.as_str(),
            tool: Some("message-vault-server"),
            stage: vault_imports::ImportStage::Parse,
            staging_dir: None,
            device_id: None,
            form_json: None,
            source_fingerprint: None,
            source_identities: None,
        },
    )
    .await?;

    let import_opts = ImportOptions {
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
    )
    .await;

    let complete_args = match &result {
        Ok(stats) => {
            vault_imports::CompleteImportArgs::succeeded(stats.messages, stats.attachments)
        }
        Err(_) => vault_imports::CompleteImportArgs::failed(),
    };
    vault_imports::complete_import_or_warn(&mut conn, &account_id, import_id, &complete_args).await;
    let import_stats = result?;

    let dedupe = if opts.skip_dedupe {
        None
    } else {
        let dedupe_stats =
            dedupe::dedupe_cross_source(&mut conn, &account_id, None, opts.window_secs).await?;
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

/// A database URL with credentials stripped, safe for status and error
/// output: `postgres://user:secret@host:5432/db` prints as
/// `postgres://host:5432/db`. Query parameters (which can carry secrets of
/// their own) are dropped too. Inputs that are not `scheme://…` URLs print
/// as a placeholder instead of being echoed raw.
///
/// Best effort: a malformed URL — a `/` or `#` inside the password, for
/// instance — can defeat the splits and leak credentials into the error
/// context. sqlx rejects such URLs before any output is produced, so this
/// only ever prints URLs that failed to open for other reasons.
pub(crate) fn redact_db_url(url: &str) -> String {
    let Some((scheme, rest)) = url.split_once("://") else {
        return "<db url>".to_string();
    };
    let rest = rest.split_once('?').map_or(rest, |(r, _)| r);
    let (authority, path) = match rest.split_once('/') {
        Some((authority, path)) => (authority, format!("/{path}")),
        None => (rest, String::new()),
    };
    let host = authority
        .rsplit_once('@')
        .map_or(authority, |(_, host)| host);
    format!("{scheme}://{host}{path}")
}

/// Every JSON Lines file (`.jsonl`, one JSON object per line) directly inside
/// `dir`, sorted by path.
///
/// # Errors
///
/// Returns an error when `dir` cannot be read.
pub fn list_jsonl_files(dir: &Path) -> Result<Vec<PathBuf>> {
    let entries = fs::read_dir(dir).with_context(|| format!("failed to read {}", dir.display()))?;
    let mut paths = Vec::new();
    for entry in entries {
        let Ok(entry) = entry else {
            continue;
        };
        let path = entry.path();
        let Some(ext) = path.extension().and_then(|ext| ext.to_str()) else {
            continue;
        };
        if ext.eq_ignore_ascii_case("jsonl") {
            paths.push(path);
        }
    }
    paths.sort();
    Ok(paths)
}

/// Collect distinct IR `export.source` values from conversation headers.
///
/// # Errors
///
/// Returns an error when a JSON Lines file cannot be read.
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
    fn redacts_credentials_from_db_url() {
        assert_eq!(
            redact_db_url("postgres://vault:vault@127.0.0.1:5432/vault"),
            "postgres://127.0.0.1:5432/vault"
        );
        assert_eq!(
            redact_db_url("postgres://user:pa:ss@host:5432/db?sslmode=require"),
            "postgres://host:5432/db"
        );
        assert_eq!(
            redact_db_url("postgres://user@host/db"),
            "postgres://host/db"
        );
        assert_eq!(redact_db_url("postgres://user:pw@host"), "postgres://host");
        assert_eq!(
            redact_db_url("sqlite://data/vault.db"),
            "sqlite://data/vault.db"
        );
        assert_eq!(
            redact_db_url("sqlite:///tmp/vault.db?mode=rwc"),
            "sqlite:///tmp/vault.db"
        );
        assert_eq!(redact_db_url("not-a-url"), "<db url>");
    }

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
