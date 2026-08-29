//! Import message-ir JSONL into the vault.
//!
//! The pipeline runs in three stages: `staging` parses JSONL files and writes
//! staging rows, `promote` copies staging rows into the production tables, and
//! `contact_name` links handles to vault contacts and merges display names.
//! The HTTP handlers for `POST /v1/import` and the `/v1/imports` session
//! routes live at the end of this module.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use sqlx::AnyConnection;
use sqlx::Connection;
use tempfile::TempDir;

use axum::Json;
use axum::extract::{FromRequest, Multipart, Path as AxumPath, Query, Request, State};
use axum::http::HeaderMap;
use tokio::sync::Mutex;

use crate::assets::AssetStats;
use crate::config::{PathsConfig, validate_source_id};
use crate::db::contacts;
use crate::db::dialect;
use crate::db::engine;
use crate::db::schema;
use crate::db::vault_imports::{self, CompleteImportArgs};
use crate::import_media::MediaMode;

pub mod contact_name;
pub mod promote;
pub mod staging;

pub use contact_name::ContactNameMode;
pub use staging::is_orphaned_export;

use staging::StagingInserts;

use crate::dedupe;
use crate::import::{self};
use crate::server::{
    ApiError, AppState, content_type_base, is_jsonl_content_type, is_multipart_content_type,
    require_import_access, resolve_auth, resolve_import_account, safe_rel_path,
    stream_body_to_file, stream_field_to_file,
};

/// What happens to a source's messages that were imported before.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportMode {
    /// Wipe the source's existing messages before importing.
    Replace,
    /// Keep existing messages and add only new ones.
    Append,
}

impl ImportMode {
    /// Parse `replace` or `append`.
    ///
    /// # Errors
    ///
    /// Returns an error when `s` is not one of those values.
    pub fn parse(s: &str) -> Result<Self> {
        match s.to_ascii_lowercase().as_str() {
            "replace" => Ok(Self::Replace),
            "append" => Ok(Self::Append),
            other => bail!("invalid import mode '{other}' (expected replace or append)"),
        }
    }

    /// Canonical flag value (`replace` or `append`).
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Replace => "replace",
            Self::Append => "append",
        }
    }
}

/// Full import settings: paths, mode, media handling, and contact naming.
#[derive(Debug, Clone)]
pub struct ImportOptions<'a> {
    /// Content-addressed asset store when [`Self::source_from_jsonl`] is false.
    pub assets_dir: &'a Path,
    /// Root for resolving relative attachment paths in JSONL.
    pub asset_root: &'a Path,
    /// Optional address book to load: VCF or vCard CSV export.
    pub contacts: Option<&'a Path>,
    /// Reload the address book even when contacts already exist.
    pub overwrite_contacts: bool,
    /// Import mode: replace or append.
    pub mode: ImportMode,
    /// Fixed source id (HTTP / `--source` override). Ignored when `source_from_jsonl`.
    pub source: &'a str,
    /// Vault account the import writes into.
    pub account_id: &'a str,
    /// Fill missing `content_key` values during promote (needed before cross-source dedupe).
    pub fill_content_keys: bool,
    /// Optional vault import session id (messages stamped on promote).
    pub import_id: Option<i64>,
    /// When true, stamp `messages.source` from each conversation's IR `export.source`.
    pub source_from_jsonl: bool,
    /// Required when `source_from_jsonl` to resolve per-source asset dirs.
    pub paths: Option<&'a PathsConfig>,
    /// Attachment handling mode: copy, none, convert, compress.
    pub media: MediaMode,
    /// When `source_from_jsonl` + Replace: wipe these sources before import.
    pub wipe_sources: Option<Vec<String>>,
    /// Apply vault contact preferred names to import `name_alias` values.
    pub contact_name_mode: ContactNameMode,
}

/// Path/mode fields for [`ImportOptions::fixed`].
#[derive(Debug, Clone, Copy)]
pub struct FixedImportArgs<'a> {
    /// Content-addressed asset store directory.
    pub assets_dir: &'a Path,
    /// Root for resolving relative attachment paths in JSONL.
    pub asset_root: &'a Path,
    /// Optional address book to load: VCF or vCard CSV export.
    pub contacts: Option<&'a Path>,
    /// Reload the address book even when contacts already exist.
    pub overwrite_contacts: bool,
    /// Import mode: replace or append.
    pub mode: ImportMode,
    /// Fixed source id applied to every conversation.
    pub source: &'a str,
    /// Vault account the import writes into.
    pub account_id: &'a str,
    /// Fill missing `content_key` values during promote.
    pub fill_content_keys: bool,
    /// Optional vault import session id (messages stamped on promote).
    pub import_id: Option<i64>,
}

impl<'a> ImportOptions<'a> {
    /// HTTP / tests / reset-demo: fixed source + assets dir, copy media.
    pub fn fixed(args: FixedImportArgs<'a>) -> Self {
        Self {
            assets_dir: args.assets_dir,
            asset_root: args.asset_root,
            contacts: args.contacts,
            overwrite_contacts: args.overwrite_contacts,
            mode: args.mode,
            source: args.source,
            account_id: args.account_id,
            fill_content_keys: args.fill_content_keys,
            import_id: args.import_id,
            source_from_jsonl: false,
            paths: None,
            media: MediaMode::Copy,
            wipe_sources: None,
            contact_name_mode: ContactNameMode::default(),
        }
    }
}

/// Counters for one import run (staging and promote results).
#[derive(Debug, Default, Clone, Serialize, utoipa::ToSchema)]
pub struct ImportStats {
    /// Conversations imported.
    pub conversations: u64,
    /// Participant rows imported.
    pub participants: u64,
    /// Messages imported.
    pub messages: u64,
    /// Attachment records (message–media links) imported.
    pub attachments: u64,
    /// Tapback reactions imported.
    pub tapbacks: u64,
    /// JSONL files imported.
    pub files: u64,
    /// Unique media files written to the asset store.
    pub assets_copied: u64,
    /// Media files already present under the same fingerprint, skipped.
    pub assets_deduped: u64,
    /// Attachment files referenced but not found on disk.
    pub assets_missing: u64,
    /// Contacts loaded from the address book.
    pub contacts: u64,
    /// Contact–handle links created.
    pub contact_handles: u64,
    /// Contact–group links created.
    pub contact_group_links: u64,
    /// True when the address book was not loaded (already present or no file).
    pub contacts_skipped: bool,
    /// Messages hidden as duplicates within this import.
    pub messages_deduped: u64,
    /// Messages added by an append-mode import.
    pub messages_appended: u64,
    /// Import mode string (`replace` or `append`).
    pub mode: String,
    /// Flagged phone handles (ambiguous; review note set) inserted by this import.
    pub phones_needing_review: u64,
}

impl ImportStats {
    fn merge_file(&mut self, other: &ImportStats) {
        self.conversations += other.conversations;
        self.participants += other.participants;
        self.messages += other.messages;
        self.attachments += other.attachments;
        self.tapbacks += other.tapbacks;
        self.messages_deduped += other.messages_deduped;
        self.phones_needing_review += other.phones_needing_review;
    }
}

/// Arguments for [`import_export`].
#[derive(Debug, Clone, Copy)]
pub struct ImportExportArgs<'a> {
    /// Folder of `*.jsonl` conversation files to import.
    pub export_dir: &'a Path,
    /// Database path.
    pub db_path: &'a Path,
    /// Content-addressed asset store directory.
    pub assets_dir: &'a Path,
    /// Optional address book to load: VCF or vCard CSV export.
    pub contacts: Option<&'a Path>,
    /// Reload the address book even when contacts already exist.
    pub overwrite_contacts: bool,
    /// Import mode: replace or append.
    pub mode: ImportMode,
    /// Fixed source id applied to every conversation.
    pub source: &'a str,
    /// Vault account the import writes into.
    pub account_id: &'a str,
}

/// Import every JSON Lines file (`*.jsonl`, one JSON object per line) under
/// `args.export_dir` (CLI staging path — the temporary import area).
///
/// # Errors
///
/// Returns an error when the export directory is missing, a file cannot be
/// parsed, or a database write fails.
pub async fn import_export(args: &ImportExportArgs<'_>) -> Result<ImportStats> {
    if !args.export_dir.is_dir() {
        bail!(
            "export directory does not exist: {}",
            args.export_dir.display()
        );
    }

    let paths = crate::import_cli::list_jsonl_files(args.export_dir)?;

    let pool = engine::open_pool_for_path(args.db_path)
        .await
        .with_context(|| format!("failed to open database {}", args.db_path.display()))?;
    let mut conn = pool.acquire().await?;
    schema::ensure_vault_schema(&mut conn).await?;
    crate::db::account_profile::ensure_account_row(&mut conn, args.account_id).await?;

    let import_id = vault_imports::start_import(
        &mut conn,
        args.account_id,
        args.source,
        args.mode.as_str(),
        Some("message-vault-server"),
    )
    .await?;

    let result = import_jsonl_files_on_conn(
        &mut conn,
        &paths,
        &ImportOptions::fixed(FixedImportArgs {
            assets_dir: args.assets_dir,
            asset_root: args.export_dir,
            contacts: args.contacts,
            overwrite_contacts: args.overwrite_contacts,
            mode: args.mode,
            source: args.source,
            account_id: args.account_id,
            fill_content_keys: true,
            import_id: Some(import_id),
        }),
        ImportSchemaMode::AssumeReady,
    )
    .await;

    let complete_args = match &result {
        Ok(stats) => CompleteImportArgs::succeeded(stats.messages, stats.attachments),
        Err(_) => CompleteImportArgs::failed(),
    };
    vault_imports::complete_import_or_warn(&mut conn, args.account_id, import_id, &complete_args)
        .await;

    result
}

/// Whether import should run DDL/schema ensure on the connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportSchemaMode {
    /// CLI / one-shot: ensure vault + messages schema.
    Ensure,
    /// HTTP serve hot path: schema already ensured on the warm connection.
    AssumeReady,
}

/// Test helper: open a configured database and run one import.
///
/// Production paths use [`import_jsonl_files_on_conn`] on their own
/// connection (HTTP serve) or [`import_export`] (CLI directory import).
#[cfg(test)]
pub(crate) async fn import_jsonl_files(
    db_path: &Path,
    paths: &[PathBuf],
    opts: &ImportOptions<'_>,
) -> Result<ImportStats> {
    validate_import_options(opts)?;

    if let Some(parent) = db_path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }

    let pool = engine::open_pool_for_path(db_path)
        .await
        .with_context(|| format!("failed to open database {}", db_path.display()))?;
    let mut conn = pool.acquire().await?;
    println!("  sql:      opened {}", db_path.display());
    let _ = io::stdout().flush();
    import_jsonl_files_on_conn(&mut conn, paths, opts, ImportSchemaMode::Ensure).await
}

fn validate_import_options(opts: &ImportOptions<'_>) -> Result<()> {
    if opts.source_from_jsonl {
        if opts.paths.is_none() {
            bail!("source_from_jsonl requires config paths for per-source assets");
        }
    } else if opts.source.trim().is_empty() {
        bail!("import source id must not be empty");
    }
    Ok(())
}

/// Import onto an existing connection (warm serve path or tests).
///
/// # Errors
///
/// Returns an error when options are invalid or staging / promote fails.
pub async fn import_jsonl_files_on_conn(
    conn: &mut AnyConnection,
    paths: &[PathBuf],
    opts: &ImportOptions<'_>,
    schema_mode: ImportSchemaMode,
) -> Result<ImportStats> {
    validate_import_options(opts)?;
    if !opts.source_from_jsonl {
        fs::create_dir_all(opts.assets_dir)
            .with_context(|| format!("failed to create {}", opts.assets_dir.display()))?;
    }

    if schema_mode == ImportSchemaMode::Ensure {
        schema::ensure_vault_schema(conn).await?;
    }
    crate::db::account_profile::ensure_account_row(conn, opts.account_id).await?;

    if let Some(path) = opts.contacts {
        println!("  sql:      loading contacts from {}…", path.display());
    } else {
        println!("  sql:      contacts load skipped (no --contacts address book)");
    }
    let _ = io::stdout().flush();
    let contact_stats = contacts::load_contacts_if_needed(
        conn,
        opts.contacts,
        opts.overwrite_contacts,
        opts.account_id,
    )
    .await?;
    if contact_stats.skipped {
        println!("  sql:      contacts skipped (already loaded or no address book)");
    } else {
        println!(
            "  sql:      contacts={} phones={} groups={}",
            contact_stats.contacts, contact_stats.phones, contact_stats.groups
        );
    }
    if schema_mode == ImportSchemaMode::Ensure {
        println!("  sql:      ensuring schema + resetting staging for account…");
        let _ = io::stdout().flush();
    } else {
        println!("  sql:      resetting staging for account…");
        let _ = io::stdout().flush();
    }
    schema::reset_staging_for_account(conn, opts.account_id).await?;
    let wipe_sources: Vec<String> = if opts.mode == ImportMode::Replace {
        if opts.source_from_jsonl {
            opts.wipe_sources.clone().unwrap_or_default()
        } else {
            vec![opts.source.to_string()]
        }
    } else {
        Vec::new()
    };
    for source in &wipe_sources {
        validate_source_id(source)?;
    }
    let _ = io::stdout().flush();

    let total_files = paths.len();
    println!(
        "  import:   {} JSONL file{}",
        total_files,
        if total_files == 1 { "" } else { "s" }
    );
    if opts.mode == ImportMode::Replace {
        let wiped = wipe_sources.join(", ");
        println!("  import:   will wipe source(s) '{wiped}' after staging succeeds");
    }
    let _ = io::stdout().flush();

    let media_work = TempDir::new().context("temp dir for import-time media rewrite")?;

    let mut stats = ImportStats {
        contacts: contact_stats.contacts,
        contact_handles: contact_stats.phones,
        contact_group_links: contact_stats.groups,
        contacts_skipped: contact_stats.skipped,
        phones_needing_review: contact_stats.phones_needing_review,
        mode: opts.mode.as_str().to_string(),
        ..Default::default()
    };
    let mut asset_stats = AssetStats::default();
    let started = Instant::now();
    let progress_every = if total_files <= 20 {
        1usize
    } else {
        (total_files / 40).max(10)
    };
    const STAGING_COMMIT_EVERY: usize = 50;

    // Staging writes need the write lock up front on SQLite (IMMEDIATE) so
    // two imports for different accounts cannot race into SQLITE_BUSY at the
    // first INSERT; Postgres has no statement-level equivalent and uses a
    // plain BEGIN.
    let engine = dialect::engine_of(conn);
    let mut tx = conn
        .begin_with(dialect::begin_immediate_sql(engine))
        .await?;
    let mut stmts = StagingInserts::new(opts.account_id, opts.import_id);

    for (idx, path) in paths.iter().enumerate() {
        let file_stats = staging::import_file_to_staging(
            &mut tx,
            &mut stmts,
            opts,
            path,
            &mut asset_stats,
            media_work.path(),
        )
        .await?;
        stats.merge_file(&file_stats);
        stats.files += 1;

        let n = idx + 1;
        if n == 1 || n == total_files || n % progress_every == 0 {
            let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("?");
            println!(
                "  import:   [{n}/{total_files}] {name}  msgs={} attachments={} assets_copied={} missing={}  ({:.0}s)",
                stats.messages,
                stats.attachments,
                asset_stats.copied,
                asset_stats.missing,
                started.elapsed().as_secs_f64()
            );
            let _ = io::stdout().flush();
        }

        if n % STAGING_COMMIT_EVERY == 0 && n < total_files {
            tx.commit().await?;
            tx = conn
                .begin_with(dialect::begin_immediate_sql(engine))
                .await?;
        }
    }
    drop(stmts);
    tx.commit().await?;

    println!(
        "  import:   promoting staging → production ({:.0}s so far)…",
        started.elapsed().as_secs_f64()
    );
    let _ = io::stdout().flush();
    let promote_stats = promote::promote_append(
        conn,
        opts.mode,
        opts.account_id,
        opts.fill_content_keys,
        &wipe_sources,
    )
    .await?;
    stats.messages_deduped += promote_stats.messages_deduped;
    stats.messages_appended = promote_stats.messages_appended;
    if opts.mode == ImportMode::Append {
        stats.conversations = promote_stats.conversations;
        stats.participants = promote_stats.participants;
        stats.messages = promote_stats.messages;
        stats.attachments = promote_stats.attachments;
        stats.tapbacks = promote_stats.tapbacks;
    }

    schema::reset_staging_for_account(conn, opts.account_id).await?;

    stats.assets_copied = asset_stats.copied;
    stats.assets_deduped = asset_stats.deduped;
    stats.assets_missing = asset_stats.missing;

    println!(
        "  import:   finished in {:.1}s  files={} msgs={} attachments={} assets_copied={}",
        started.elapsed().as_secs_f64(),
        stats.files,
        stats.messages,
        stats.attachments,
        stats.assets_copied
    );

    Ok(stats)
}

#[derive(Debug, Deserialize)]
pub(crate) struct ImportQuery {
    source: String,
    /// Username or UUID. Optional; when set must match the Bearer token's account.
    #[serde(default)]
    account: Option<String>,
    #[serde(default = "default_import_mode")]
    mode: String,
    /// Run cross-source soft-dedupe after import.
    #[serde(default)]
    dedupe: bool,
    /// Optional vault import session id from POST /v1/imports.
    #[serde(default)]
    import_id: Option<i64>,
    /// How vault contacts supply participant names (`fill_missing`, `overwrite`, or `as_is`).
    #[serde(default = "default_contact_name_mode")]
    contact_name_mode: String,
}

fn default_contact_name_mode() -> String {
    "fill_missing".to_string()
}

fn default_import_mode() -> String {
    "append".to_string()
}

/// Import result: stats plus optional dedupe counts.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub(crate) struct ImportResponse {
    ok: bool,
    source: String,
    account: String,
    #[serde(flatten)]
    stats: ImportStats,
    #[serde(skip_serializing_if = "Option::is_none")]
    dedupe: Option<DedupeResponse>,
}

/// Cross-source dedupe outcome.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub(crate) struct DedupeResponse {
    keys_filled: u64,
    exact_groups: u64,
    exact_flagged: u64,
    near_flagged: u64,
}

/// Source, mode, tool, and optional account for a new import session.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub(crate) struct CreateImportBody {
    pub(crate) source: String,
    #[serde(default = "default_import_mode")]
    pub(crate) mode: String,
    #[serde(default)]
    pub(crate) tool: Option<String>,
    #[serde(default)]
    pub(crate) account: Option<String>,
}

/// The new import session id.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub(crate) struct CreateImportResponse {
    ok: bool,
    id: i64,
}

/// Final stats and issues for a finished import session.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub(crate) struct CompleteImportBody {
    #[serde(default = "default_true")]
    pub(crate) ok: bool,
    #[serde(default)]
    pub(crate) message_count: Option<i64>,
    #[serde(default)]
    pub(crate) attachment_count: Option<i64>,
    #[serde(default)]
    pub(crate) bytes_uploaded: Option<i64>,
    #[serde(default)]
    pub(crate) duration_ms: Option<i64>,
    #[serde(default)]
    pub(crate) parse_ms: Option<i64>,
    #[serde(default)]
    pub(crate) attachments_ms: Option<i64>,
    #[serde(default)]
    pub(crate) prepare_ms: Option<i64>,
    #[serde(default)]
    pub(crate) upload_ms: Option<i64>,
    #[serde(default)]
    pub(crate) summary: Option<serde_json::Value>,
    #[serde(default)]
    pub(crate) issues: Vec<CompleteImportIssueBody>,
}

fn default_true() -> bool {
    true
}

/// One parse/convert/upload issue from the import.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub(crate) struct CompleteImportIssueBody {
    pub(crate) kind: String,
    pub(crate) step: String,
    pub(crate) item: String,
    pub(crate) reason: String,
}

fn validate_complete_import_issues(issues: &[CompleteImportIssueBody]) -> Result<(), ApiError> {
    for issue in issues {
        match issue.kind.as_str() {
            "error" | "skip" => {}
            other => {
                return Err(ApiError::BadRequest(format!(
                    "invalid import issue kind '{other}'; expected 'error' or 'skip'"
                )));
            }
        }
    }
    Ok(())
}

/// Stored session status after completion.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub(crate) struct CompleteImportResponse {
    ok: bool,
    id: i64,
    pub(crate) status: String,
    pub(crate) message_count: i64,
    pub(crate) attachment_count: i64,
    pub(crate) bytes_uploaded: i64,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ListImportsQuery {
    #[serde(default)]
    account: Option<String>,
}

/// Past import sessions.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub(crate) struct ImportsListResponse {
    imports: Vec<crate::db::vault_imports::ImportSummary>,
}

/// One stored import issue.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub(crate) struct ImportDetailIssueResponse {
    pub(crate) kind: String,
    pub(crate) step: String,
    item: String,
    reason: String,
}

/// Full import session record.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub(crate) struct ImportDetailResponse {
    pub(crate) id: i64,
    source: String,
    tool: Option<String>,
    mode: String,
    status: String,
    started_at: String,
    finished_at: Option<String>,
    message_count: i64,
    attachment_count: i64,
    bytes_uploaded: i64,
    pub(crate) duration_ms: Option<i64>,
    pub(crate) parse_ms: Option<i64>,
    pub(crate) attachments_ms: Option<i64>,
    pub(crate) prepare_ms: Option<i64>,
    pub(crate) upload_ms: Option<i64>,
    pub(crate) summary: serde_json::Value,
    pub(crate) issues: Vec<ImportDetailIssueResponse>,
}

/// List past import sessions for the account with their stats.
#[utoipa::path(
    get,
    path = "/v1/imports",
    tag = "Import",
    security(("bearer" = [])),
    params(("account" = Option<String>, Query)),
    responses(
        (status = 200, body = ImportsListResponse),
        (status = 401, body = crate::server::ErrorBody),
        (status = 403, body = crate::server::ErrorBody)
    )
)]
pub(crate) async fn imports_list_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<ListImportsQuery>,
) -> Result<Json<ImportsListResponse>, ApiError> {
    let auth = resolve_auth(&headers, &state).await?;
    require_import_access(&auth)?;
    let account = resolve_import_account(&auth, query.account.as_deref(), &state.db).await?;

    // TODO(#148): pool acquire
    let mut conn = state.db.acquire().await?;
    let imports = crate::db::vault_imports::list_imports(&mut conn, &account)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    Ok(Json(ImportsListResponse { imports }))
}

/// Status, timings, and issues for one import session.
#[utoipa::path(
    get,
    path = "/v1/imports/{id}",
    tag = "Import",
    security(("bearer" = [])),
    params(("id" = i64, Path, description = "Import session id")),
    responses(
        (status = 200, body = ImportDetailResponse),
        (status = 401, body = crate::server::ErrorBody),
        (status = 403, body = crate::server::ErrorBody),
        (status = 404, body = crate::server::ErrorBody)
    )
)]
pub(crate) async fn imports_get_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(import_id): AxumPath<i64>,
) -> Result<Json<ImportDetailResponse>, ApiError> {
    let auth = resolve_auth(&headers, &state).await?;
    require_import_access(&auth)?;
    // TODO(#148): pool acquire
    let mut conn = state.db.acquire().await?;
    let detail =
        crate::db::vault_imports::get_import_detail(&mut conn, &auth.account_id, import_id)
            .await
            .map_err(ApiError::from)?;

    Ok(Json(import_detail_response(detail)))
}

/// Start an import session and return its id. Finish the session at
/// POST /v1/imports/{id}/complete.
#[utoipa::path(
    post,
    path = "/v1/imports",
    tag = "Import",
    security(("bearer" = [])),
    request_body = CreateImportBody,
    responses(
        (status = 200, body = CreateImportResponse),
        (status = 400, body = crate::server::ErrorBody),
        (status = 401, body = crate::server::ErrorBody),
        (status = 403, body = crate::server::ErrorBody)
    )
)]
pub(crate) async fn imports_create_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<CreateImportBody>,
) -> Result<Json<CreateImportResponse>, ApiError> {
    let auth = resolve_auth(&headers, &state).await?;
    require_import_access(&auth)?;
    if body.source.trim().is_empty() {
        return Err(ApiError::BadRequest("body field source is required".into()));
    }
    validate_source_id(&body.source).map_err(|e| ApiError::BadRequest(e.to_string()))?;
    ImportMode::parse(&body.mode).map_err(|e| ApiError::BadRequest(e.to_string()))?;
    let account = resolve_import_account(&auth, body.account.as_deref(), &state.db).await?;

    // TODO(#148): pool acquire
    let mut conn = state.db.acquire().await?;
    crate::db::account_profile::ensure_account_row(&mut conn, &account)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    let id = crate::db::vault_imports::start_import(
        &mut conn,
        &account,
        &body.source,
        &body.mode,
        body.tool.as_deref(),
    )
    .await
    .map_err(|e| ApiError::Internal(e.to_string()))?;

    Ok(Json(CreateImportResponse { ok: true, id }))
}

/// Record the outcome of an import session started with POST /v1/imports.
#[utoipa::path(
    post,
    path = "/v1/imports/{id}/complete",
    tag = "Import",
    security(("bearer" = [])),
    params(("id" = i64, Path, description = "Import session id")),
    request_body = CompleteImportBody,
    responses(
        (status = 200, body = CompleteImportResponse),
        (status = 400, body = crate::server::ErrorBody),
        (status = 401, body = crate::server::ErrorBody),
        (status = 403, body = crate::server::ErrorBody),
        (status = 404, body = crate::server::ErrorBody)
    )
)]
pub(crate) async fn imports_complete_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(import_id): AxumPath<i64>,
    Json(body): Json<CompleteImportBody>,
) -> Result<Json<CompleteImportResponse>, ApiError> {
    let auth = resolve_auth(&headers, &state).await?;
    require_import_access(&auth)?;
    let account = resolve_import_account(&auth, None, &state.db).await?;
    validate_complete_import_issues(&body.issues)?;
    let summary_json = match body.summary {
        Some(summary) => Some(
            serde_json::to_string(&summary)
                .map_err(|e| ApiError::Internal(format!("serialize import summary: {e}")))?,
        ),
        None => None,
    };
    let args = crate::db::vault_imports::CompleteImportArgs {
        ok: body.ok,
        message_count: body.message_count,
        attachment_count: body.attachment_count,
        bytes_uploaded: body.bytes_uploaded,
        duration_ms: body.duration_ms,
        parse_ms: body.parse_ms,
        attachments_ms: body.attachments_ms,
        prepare_ms: body.prepare_ms,
        upload_ms: body.upload_ms,
        summary_json,
        issues: body
            .issues
            .into_iter()
            .map(|issue| crate::db::vault_imports::ImportIssueInput {
                kind: issue.kind,
                step: issue.step,
                item: issue.item,
                reason: issue.reason,
            })
            .collect(),
    };
    // TODO(#148): pool acquire
    let mut conn = state.db.acquire().await?;
    let row = crate::db::vault_imports::complete_import(&mut conn, &account, import_id, &args)
        .await
        .map_err(
            |e| match e.downcast::<crate::db::vault_imports::ImportLookupError>() {
                Ok(lookup) => ApiError::from(lookup),
                Err(other) => ApiError::Internal(other.to_string()),
            },
        )?;

    Ok(Json(CompleteImportResponse {
        ok: true,
        id: row.id,
        status: row.status,
        message_count: row.message_count,
        attachment_count: row.attachment_count,
        bytes_uploaded: row.bytes_uploaded,
    }))
}

fn parse_summary_json(summary_json: Option<String>) -> serde_json::Value {
    match summary_json {
        Some(raw) => serde_json::from_str(&raw).unwrap_or(serde_json::Value::String(raw)),
        None => serde_json::Value::Null,
    }
}

fn import_detail_response(detail: crate::db::vault_imports::ImportDetail) -> ImportDetailResponse {
    let row = detail.row;
    let issues = detail
        .issues
        .into_iter()
        .map(|issue| ImportDetailIssueResponse {
            kind: issue.kind,
            step: issue.step,
            item: issue.item,
            reason: issue.reason,
        })
        .collect();

    ImportDetailResponse {
        id: row.id,
        source: row.source,
        tool: row.tool,
        mode: row.mode,
        status: row.status,
        started_at: row.started_at,
        finished_at: row.finished_at,
        message_count: row.message_count,
        attachment_count: row.attachment_count,
        bytes_uploaded: row.bytes_uploaded,
        duration_ms: row.duration_ms,
        parse_ms: row.parse_ms,
        attachments_ms: row.attachments_ms,
        prepare_ms: row.prepare_ms,
        upload_ms: row.upload_ms,
        summary: parse_summary_json(row.summary_json),
        issues,
    }
}

/// Import one message-ir JSONL body (raw or multipart) into the vault.
#[utoipa::path(
    post,
    path = "/v1/import",
    tag = "Import",
    security(("bearer" = [])),
    params(
        ("source" = String, Query),
        ("account" = Option<String>, Query),
        ("mode" = Option<String>, Query, description = "Default append"),
        ("dedupe" = Option<bool>, Query),
        ("import_id" = Option<i64>, Query),
        ("contact_name_mode" = Option<String>, Query)
    ),
    request_body(
        content(
            ("application/x-ndjson"),
            ("application/jsonl"),
            ("multipart/form-data")
        ),
        description = "message-ir JSONL. application/x-ndjson, application/jsonl, and multipart/form-data (field jsonl plus file parts) are accepted."
    ),
    responses(
        (status = 200, body = ImportResponse),
        (status = 400, body = crate::server::ErrorBody),
        (status = 401, body = crate::server::ErrorBody),
        (status = 403, body = crate::server::ErrorBody),
        (status = 404, body = crate::server::ErrorBody)
    )
)]
pub(crate) async fn import_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(mut query): Query<ImportQuery>,
    request: Request,
) -> Result<Json<ImportResponse>, ApiError> {
    let auth = resolve_auth(&headers, &state).await?;
    require_import_access(&auth)?;

    let Some(ct) = content_type_base(&headers) else {
        return Err(ApiError::BadRequest(
            "Content-Type required (application/x-ndjson, application/jsonl, or multipart/form-data)"
                .into(),
        ));
    };

    if query.source.trim().is_empty() {
        return Err(ApiError::BadRequest(
            "query param source is required".into(),
        ));
    }
    validate_source_id(&query.source).map_err(|e| ApiError::BadRequest(e.to_string()))?;
    let account = resolve_import_account(&auth, query.account.as_deref(), &state.db).await?;
    query.account = Some(account);

    if is_multipart_content_type(ct) {
        let multipart = Multipart::from_request(request, &state)
            .await
            .map_err(|e| ApiError::BadRequest(format!("invalid multipart body: {e}")))?;
        return import_multipart(state, query, multipart).await;
    }

    if is_jsonl_content_type(ct) {
        let temp = tempfile::tempdir().map_err(|e| ApiError::Internal(format!("temp dir: {e}")))?;
        let jsonl_path = temp.path().join("_import.jsonl");
        let n = stream_body_to_file(request.into_body(), &jsonl_path, state.max_body_bytes).await?;
        if n == 0 {
            return Err(ApiError::BadRequest("request body is empty".into()));
        }
        // The import pipeline does blocking file IO (JSONL parse, asset
        // hashing and copies) — run it off the async workers so a large
        // import cannot stall unrelated requests.
        let handle = tokio::runtime::Handle::current();
        let response = tokio::task::spawn_blocking(move || {
            handle.block_on(run_import_path(state, query, jsonl_path, None))
        })
        .await
        .map_err(|e| ApiError::Internal(format!("import task failed: {e}")))?;
        drop(temp);
        return response;
    }

    Err(ApiError::BadRequest(
        "Content-Type must be application/x-ndjson, application/jsonl, or multipart/form-data"
            .into(),
    ))
}

async fn import_multipart(
    state: AppState,
    query: ImportQuery,
    mut multipart: Multipart,
) -> Result<Json<ImportResponse>, ApiError> {
    let temp = tempfile::tempdir().map_err(|e| ApiError::Internal(format!("temp dir: {e}")))?;
    let asset_root = temp.path().to_path_buf();
    let jsonl_path = asset_root.join("_import.jsonl");
    let mut have_jsonl = false;
    let mut file_count = 0u64;

    while let Some(mut field) = multipart
        .next_field()
        .await
        .map_err(|e| ApiError::BadRequest(format!("multipart field error: {e}")))?
    {
        let name = field.name().unwrap_or("").to_string();
        match name.as_str() {
            "jsonl" => {
                let n = stream_field_to_file(field, &jsonl_path).await?;
                if n == 0 {
                    return Err(ApiError::BadRequest("jsonl part is empty".into()));
                }
                have_jsonl = true;
            }
            "file" => {
                let filename = match field.file_name() {
                    Some(name) if !name.is_empty() => name.to_string(),
                    _ => {
                        return Err(ApiError::BadRequest(
                            "file part missing filename (use relative path e.g. attachments/a.jpg)"
                                .into(),
                        ));
                    }
                };
                let rel = safe_rel_path(&filename)?;
                let dest = asset_root.join(&rel);
                stream_field_to_file(field, &dest).await?;
                file_count += 1;
            }
            other => {
                while let Some(chunk) = field
                    .chunk()
                    .await
                    .map_err(|e| ApiError::BadRequest(format!("multipart chunk: {e}")))?
                {
                    let _ = chunk;
                }
                eprintln!("import: ignoring unknown multipart field {other:?}");
            }
        }
    }

    if !have_jsonl {
        return Err(ApiError::BadRequest(
            "multipart missing required field 'jsonl'".into(),
        ));
    }
    eprintln!("import: multipart jsonl + {file_count} file(s)");

    let handle = tokio::runtime::Handle::current();
    let response = tokio::task::spawn_blocking(move || {
        handle.block_on(run_import_path(state, query, jsonl_path, Some(asset_root)))
    })
    .await
    .map_err(|e| ApiError::Internal(format!("import task failed: {e}")))?;
    drop(temp);
    response
}

/// Bound on concurrent HTTP imports: each import holds one pooled connection
/// for its whole run, so at most this many may overlap and the remaining
/// connections stay available for auth, search, and export.
const MAX_CONCURRENT_IMPORTS: usize = 2;

fn import_semaphore() -> &'static tokio::sync::Semaphore {
    static SEMAPHORE: std::sync::OnceLock<tokio::sync::Semaphore> = std::sync::OnceLock::new();
    SEMAPHORE.get_or_init(|| tokio::sync::Semaphore::new(MAX_CONCURRENT_IMPORTS))
}

async fn run_import_path(
    state: AppState,
    query: ImportQuery,
    jsonl_path: PathBuf,
    asset_root_override: Option<PathBuf>,
) -> Result<Json<ImportResponse>, ApiError> {
    // An import holds one pooled connection for its whole run (JSONL parse,
    // asset IO, promote). Bound concurrent imports here so they can never
    // drain the pool; the semaphore is taken before the per-account lock so
    // lock order (semaphore → account → pool) is consistent everywhere.
    let _import_permit = import_semaphore()
        .acquire()
        .await
        .map_err(|_| ApiError::Internal("vault is shutting down".into()))?;
    let mode = ImportMode::parse(&query.mode).map_err(|e| ApiError::BadRequest(e.to_string()))?;
    validate_source_id(&query.source).map_err(|e| ApiError::BadRequest(e.to_string()))?;
    let contact_name_mode = import::ContactNameMode::parse(&query.contact_name_mode)
        .map_err(|e| ApiError::BadRequest(e.to_string()))?;

    let cfg = Arc::clone(&state.cfg);
    let account = query
        .account
        .clone()
        .ok_or_else(|| ApiError::BadRequest("account is required".into()))?;
    let source_id = query.source.clone();
    let do_dedupe = query.dedupe;
    let query_import_id = query.import_id;

    let account_lock = {
        let mut map = state.account_import_locks.lock().await;
        map.entry(account.clone())
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone()
    };
    let _guard = account_lock.lock().await;

    // Validate client-owned sessions before staging work so bad ids return 400.
    if let Some(id) = query_import_id {
        // TODO(#148): pool acquire
        let mut conn = state.db.acquire().await?;
        crate::db::vault_imports::require_reusable_import(
            &mut conn,
            &account,
            id,
            &source_id,
            mode.as_str(),
        )
        .await
        .map_err(|e| {
            let msg = e.to_string();
            if msg.contains("not found") {
                ApiError::NotFound(msg)
            } else if msg.contains("not running") || msg.contains("mismatch") {
                ApiError::BadRequest(msg)
            } else {
                ApiError::Internal(msg)
            }
        })?;
    }

    let assets_dir = cfg.paths.assets_dir_for_account(&account, &source_id);
    // Raw body imports resolve attachment paths only via pre-uploaded sha256 assets.
    // Multipart supplies a temp asset_root for relative file parts.
    let asset_root_owned = asset_root_override.unwrap_or_else(|| assets_dir.clone());

    // Client session (vault-push): ownership/status already checked above.
    // Otherwise start a one-shot vault_imports row so Storage history works for curl / single POSTs.
    let (import_id, owns_session) = if let Some(id) = query_import_id {
        (Some(id), false)
    } else {
        // TODO(#148): pool acquire
        let mut conn = state.db.acquire().await?;
        crate::db::account_profile::ensure_account_row(&mut conn, &account)
            .await
            .map_err(|e| ApiError::Internal(e.to_string()))?;
        let id = crate::db::vault_imports::start_import(
            &mut conn,
            &account,
            &source_id,
            mode.as_str(),
            Some("http"),
        )
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
        (Some(id), true)
    };

    let mut opts = ImportOptions::fixed(FixedImportArgs {
        assets_dir: &assets_dir,
        asset_root: &asset_root_owned,
        contacts: None,
        overwrite_contacts: false,
        mode,
        source: &source_id,
        account_id: &account,
        fill_content_keys: do_dedupe,
        import_id,
    });
    opts.contact_name_mode = contact_name_mode;
    // One pooled connection held for the whole import; the import semaphore
    // taken above keeps enough of the pool free for other requests.
    let mut conn = state.db.acquire().await?;
    let import_result = import::import_jsonl_files_on_conn(
        &mut conn,
        &[jsonl_path],
        &opts,
        import::ImportSchemaMode::AssumeReady,
    )
    .await;

    if owns_session && let Some(id) = import_id {
        let complete_args = match &import_result {
            Ok(stats) => crate::db::vault_imports::CompleteImportArgs::succeeded(
                stats.messages,
                stats.attachments,
            ),
            Err(_) => crate::db::vault_imports::CompleteImportArgs::failed(),
        };
        crate::db::vault_imports::complete_import_or_warn(&mut conn, &account, id, &complete_args)
            .await;
    }
    let stats = import_result.map_err(|e| ApiError::Internal(e.to_string()))?;
    let dedupe_stats = if do_dedupe {
        Some(
            dedupe::dedupe_cross_source(&mut conn, &account, None, 2)
                .await
                .map_err(|e| ApiError::Internal(e.to_string()))?,
        )
    } else {
        None
    };

    Ok(Json(ImportResponse {
        ok: true,
        source: source_id,
        account,
        stats,
        dedupe: dedupe_stats.map(|d| DedupeResponse {
            keys_filled: d.keys_filled,
            exact_groups: d.exact_groups,
            exact_flagged: d.exact_flagged,
            near_flagged: d.near_flagged,
        }),
    }))
}

#[cfg(test)]
mod tests {
    use super::contact_name::trim_nonempty;
    use super::*;
    use crate::assets;
    use tempfile::TempDir;

    const TEST_ACCOUNT: &str = "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa";

    fn write_jsonl(dir: &Path, name: &str, body: &str) -> PathBuf {
        let path = dir.join(name);
        fs::write(&path, body).unwrap();
        path
    }

    /// Open a verify connection to an on-disk test database.
    async fn open_verify(db: &Path) -> (sqlx::AnyPool, sqlx::pool::PoolConnection<sqlx::Any>) {
        let pool = engine::open_pool_for_path(db).await.unwrap();
        let conn = pool.acquire().await.unwrap();
        (pool, conn)
    }

    #[tokio::test]
    async fn append_skips_existing_guids_and_keeps_id_map() {
        let tmp = TempDir::new().unwrap();
        let db = tmp.path().join("vault.db");
        let assets = tmp.path().join("assets");

        let first = write_jsonl(
            tmp.path(),
            "a.jsonl",
            r#"{"schema_version":3,"export":{"source":"sms-backup-restore","tool":"test","tool_version":"0","owner_handle":null,"owner_display_name":null},"conversation":{"chat_identifier":"+14075551234","conversation_type":"individual","group_title":null,"participants":[{"handle":"+14075551234","display_name":null}],"stats":{"message_count":2,"attachment_count":0,"first_timestamp_unix_ms":1426183462000,"last_timestamp_unix_ms":1426183522000}}}
{"guid":"g-keep","timestamp_unix_ms":1426183462000,"direction":"incoming","service":"sms","message_kind":"sms","sender_handle":"+14075551234","sender_display_name":null,"subject":null,"text":"one","attachments":[],"imessage":null,"source":null}
{"guid":"g-dup","timestamp_unix_ms":1426183522000,"direction":"outgoing","service":"sms","message_kind":"sms","sender_handle":null,"sender_display_name":null,"subject":null,"text":"two","attachments":[],"imessage":null,"source":null}
"#,
        );
        let first_stats = import_jsonl_files(
            &db,
            &[first],
            &ImportOptions::fixed(FixedImportArgs {
                assets_dir: &assets,
                asset_root: tmp.path(),
                contacts: None,
                overwrite_contacts: false,
                mode: ImportMode::Replace,
                source: "sms-backup-restore",
                account_id: TEST_ACCOUNT,
                fill_content_keys: true,
                import_id: None,
            }),
        )
        .await
        .unwrap();
        assert_eq!(first_stats.messages, 2);

        let second = write_jsonl(
            tmp.path(),
            "b.jsonl",
            r#"{"schema_version":3,"export":{"source":"sms-backup-restore","tool":"test","tool_version":"0","owner_handle":null,"owner_display_name":null},"conversation":{"chat_identifier":"+14075551234","conversation_type":"individual","group_title":null,"participants":[{"handle":"+14075551234","display_name":null}],"stats":{"message_count":3,"attachment_count":0,"first_timestamp_unix_ms":1426183522000,"last_timestamp_unix_ms":1426183642000}}}
{"guid":"g-dup","timestamp_unix_ms":1426183522000,"direction":"outgoing","service":"sms","message_kind":"sms","sender_handle":null,"sender_display_name":null,"subject":null,"text":"two again","attachments":[],"imessage":null,"source":null}
{"guid":"g-new","timestamp_unix_ms":1426183582000,"direction":"incoming","service":"sms","message_kind":"sms","sender_handle":"+14075551234","sender_display_name":null,"subject":null,"text":"three","attachments":[],"imessage":null,"source":null}
{"guid":"","timestamp_unix_ms":1426183642000,"direction":"incoming","service":"sms","message_kind":"sms","sender_handle":"+14075551234","sender_display_name":null,"subject":null,"text":"empty guid always inserts","attachments":[],"imessage":null,"source":null}
"#,
        );
        let second_stats = import_jsonl_files(
            &db,
            &[second],
            &ImportOptions::fixed(FixedImportArgs {
                assets_dir: &assets,
                asset_root: tmp.path(),
                contacts: None,
                overwrite_contacts: false,
                mode: ImportMode::Append,
                source: "sms-backup-restore",
                account_id: TEST_ACCOUNT,
                fill_content_keys: false,
                import_id: None,
            }),
        )
        .await
        .unwrap();
        assert_eq!(second_stats.messages_appended, 2);
        assert_eq!(second_stats.messages_deduped, 1);

        let (_pool, mut conn) = open_verify(&db).await;
        let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM messages")
            .fetch_one(&mut *conn)
            .await
            .unwrap();
        assert_eq!(n, 4);
        let dup_body: String = sqlx::query_scalar("SELECT body FROM messages WHERE guid = 'g-dup'")
            .fetch_one(&mut *conn)
            .await
            .unwrap();
        assert_eq!(dup_body, "two");

        // Deferred full-text search during promote must still index new bodies
        // and restore triggers.
        let fts_three: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM messages_fts WHERE messages_fts MATCH 'three'",
        )
        .fetch_one(&mut *conn)
        .await
        .unwrap();
        assert_eq!(fts_three, 1);
        let fts_one: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM messages_fts WHERE messages_fts MATCH 'one'")
                .fetch_one(&mut *conn)
                .await
                .unwrap();
        assert_eq!(fts_one, 1);
        let triggers: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'trigger' AND name LIKE '%_fts_%'",
        )
        .fetch_one(&mut *conn)
        .await
        .unwrap();
        assert_eq!(triggers, 6);
    }

    fn replace_opts<'a>(assets: &'a Path, root: &'a Path, source: &'a str) -> ImportOptions<'a> {
        ImportOptions::fixed(FixedImportArgs {
            assets_dir: assets,
            asset_root: root,
            contacts: None,
            overwrite_contacts: false,
            mode: ImportMode::Replace,
            source,
            account_id: TEST_ACCOUNT,
            fill_content_keys: false,
            import_id: None,
        })
    }

    fn missing_attachment_json(name: &str) -> String {
        format!(
            r#"[{{"path":"attachments/{name}","original_name":"{name}","mime_type":"application/octet-stream","digest_sha256":null,"is_sticker":false,"transcription":null,"sticker_effect":null,"size_bytes":12,"missing_reason":"not_found"}}]"#
        )
    }

    const TAPBACK_IMESSAGE: &str = r#"{"is_reply":false,"in_reply_to_guid":null,"thread_originator_part":null,"num_replies":null,"is_deleted":false,"send_effect":null,"shared_location":null,"announcement":null,"read_receipt_rfc3339":null,"parts":null,"edits":null,"tapbacks":[{"emoji":null,"is_from_me":false,"kind":"liked","part_index":0,"sender":"+15555550999"}],"app":null,"balloon_bundle_id":null,"balloon_kind":null,"associated_guid":null,"associated_part":null,"tapback_kind":null,"tapback_emoji":null,"tapback_action":null}"#;

    fn chunk_boundary_jsonl() -> String {
        let header = r#"{"schema_version":3,"export":{"source":"imessage","tool":"test","tool_version":"0","owner_handle":null,"owner_display_name":null},"conversation":{"chat_identifier":"+15555550123","conversation_type":"individual","group_title":null,"participants":[{"handle":"+15555550123","display_name":null},{"handle":"+15555550999","display_name":null}],"stats":{"message_count":56,"attachment_count":2,"first_timestamp_unix_ms":1426183462000,"last_timestamp_unix_ms":1426183517000}}}"#;
        let mut lines = vec![header.to_string()];
        for i in 0..56 {
            let guid = format!("g-{i:02}");
            let ts = 1_426_183_462_000i64 + i64::from(i) * 1000;
            let attachments = if i == 0 {
                missing_attachment_json("first.bin")
            } else if i == 55 {
                missing_attachment_json("last.bin")
            } else {
                "[]".to_string()
            };
            let imessage = if i == 1 { TAPBACK_IMESSAGE } else { "null" };
            lines.push(format!(
                r#"{{"guid":"{guid}","timestamp_unix_ms":{ts},"direction":"incoming","service":"imessage","message_kind":"imessage","sender_handle":"+15555550123","sender_display_name":null,"subject":null,"text":"msg {i}","attachments":{attachments},"imessage":{imessage},"source":null}}"#
            ));
        }
        lines.join("\n")
    }

    #[tokio::test]
    async fn staging_chunks_56_messages_and_keeps_children_on_right_rows() {
        let tmp = TempDir::new().unwrap();
        let db = tmp.path().join("vault.db");
        let assets = tmp.path().join("assets");
        let path = write_jsonl(tmp.path(), "chunk-boundary.jsonl", &chunk_boundary_jsonl());
        let stats =
            import_jsonl_files(&db, &[path], &replace_opts(&assets, tmp.path(), "imessage"))
                .await
                .unwrap();
        assert_eq!(stats.messages, 56);
        assert_eq!(stats.attachments, 2);
        assert_eq!(stats.tapbacks, 1);
        assert_eq!(stats.messages_deduped, 0);

        let (_pool, mut conn) = open_verify(&db).await;
        let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM messages")
            .fetch_one(&mut *conn)
            .await
            .unwrap();
        assert_eq!(n, 56);
        let first_atts: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM attachments WHERE message_id = (SELECT id FROM messages WHERE guid = 'g-00')",
        )
        .fetch_one(&mut *conn)
        .await
        .unwrap();
        let last_atts: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM attachments WHERE message_id = (SELECT id FROM messages WHERE guid = 'g-55')",
        )
        .fetch_one(&mut *conn)
        .await
        .unwrap();
        let second_taps: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM tapbacks WHERE message_id = (SELECT id FROM messages WHERE guid = 'g-01')",
        )
        .fetch_one(&mut *conn)
        .await
        .unwrap();
        assert_eq!(first_atts, 1);
        assert_eq!(last_atts, 1);
        assert_eq!(second_taps, 1);
    }

    #[tokio::test]
    async fn staging_skips_duplicate_guid_in_same_file_and_keeps_first_attachment() {
        let tmp = TempDir::new().unwrap();
        let db = tmp.path().join("vault.db");
        let assets = tmp.path().join("assets");
        let header = r#"{"schema_version":3,"export":{"source":"imessage","tool":"test","tool_version":"0","owner_handle":null,"owner_display_name":null},"conversation":{"chat_identifier":"+15555550123","conversation_type":"individual","group_title":null,"participants":[{"handle":"+15555550123","display_name":null},{"handle":"+15555550999","display_name":null}],"stats":{"message_count":2,"attachment_count":2,"first_timestamp_unix_ms":1426183462000,"last_timestamp_unix_ms":1426183463000}}}"#;
        let first = format!(
            r#"{{"guid":"g-once","timestamp_unix_ms":1426183462000,"direction":"incoming","service":"imessage","message_kind":"imessage","sender_handle":"+15555550123","sender_display_name":null,"subject":null,"text":"first","attachments":{},"imessage":null,"source":null}}"#,
            missing_attachment_json("first.bin")
        );
        let second = format!(
            r#"{{"guid":"g-once","timestamp_unix_ms":1426183463000,"direction":"incoming","service":"imessage","message_kind":"imessage","sender_handle":"+15555550123","sender_display_name":null,"subject":null,"text":"second","attachments":{},"imessage":{TAPBACK_IMESSAGE},"source":null}}"#,
            missing_attachment_json("second.bin")
        );
        let path = write_jsonl(
            tmp.path(),
            "dup-guid.jsonl",
            &format!("{header}\n{first}\n{second}\n"),
        );
        let stats =
            import_jsonl_files(&db, &[path], &replace_opts(&assets, tmp.path(), "imessage"))
                .await
                .unwrap();
        assert_eq!(stats.messages, 1);
        assert_eq!(stats.messages_deduped, 1);
        assert_eq!(stats.attachments, 1);
        assert_eq!(stats.tapbacks, 0);

        let (_pool, mut conn) = open_verify(&db).await;
        let (body, attachments, tapbacks): (String, i64, i64) = sqlx::query_as(
            r#"
            SELECT m.body, COUNT(DISTINCT a.id), COUNT(DISTINCT t.id)
            FROM messages m
            LEFT JOIN attachments a ON a.message_id = m.id
            LEFT JOIN tapbacks t ON t.message_id = m.id
            WHERE m.guid = 'g-once'
            GROUP BY m.id
            "#,
        )
        .fetch_one(&mut *conn)
        .await
        .unwrap();
        assert_eq!(body, "first");
        assert_eq!(attachments, 1);
        assert_eq!(tapbacks, 0);
        let name: String = sqlx::query_scalar(
            "SELECT original_name FROM attachments WHERE message_id = (SELECT id FROM messages WHERE guid = 'g-once')",
        )
        .fetch_one(&mut *conn)
        .await
        .unwrap();
        assert_eq!(name, "first.bin");
    }

    #[tokio::test]
    async fn staging_keeps_both_rows_when_guids_differ_only_by_whitespace() {
        let tmp = TempDir::new().unwrap();
        let db = tmp.path().join("vault.db");
        let assets = tmp.path().join("assets");
        let header = r#"{"schema_version":3,"export":{"source":"imessage","tool":"test","tool_version":"0","owner_handle":null,"owner_display_name":null},"conversation":{"chat_identifier":"+15555550123","conversation_type":"individual","group_title":null,"participants":[{"handle":"+15555550123","display_name":null}],"stats":{"message_count":2,"attachment_count":2,"first_timestamp_unix_ms":1426183462000,"last_timestamp_unix_ms":1426183463000}}}"#;
        let first = format!(
            r#"{{"guid":"g-space","timestamp_unix_ms":1426183462000,"direction":"incoming","service":"imessage","message_kind":"imessage","sender_handle":"+15555550123","sender_display_name":null,"subject":null,"text":"trimmed","attachments":{},"imessage":null,"source":null}}"#,
            missing_attachment_json("trim.bin")
        );
        let second = format!(
            r#"{{"guid":" g-space","timestamp_unix_ms":1426183463000,"direction":"incoming","service":"imessage","message_kind":"imessage","sender_handle":"+15555550123","sender_display_name":null,"subject":null,"text":"padded","attachments":{},"imessage":null,"source":null}}"#,
            missing_attachment_json("pad.bin")
        );
        let path = write_jsonl(
            tmp.path(),
            "guid-whitespace.jsonl",
            &format!("{header}\n{first}\n{second}\n"),
        );
        let stats =
            import_jsonl_files(&db, &[path], &replace_opts(&assets, tmp.path(), "imessage"))
                .await
                .unwrap();
        assert_eq!(stats.messages, 2);
        assert_eq!(stats.messages_deduped, 0);
        assert_eq!(stats.attachments, 2);

        let (_pool, mut conn) = open_verify(&db).await;
        let names: Vec<String> =
            sqlx::query_scalar("SELECT original_name FROM attachments ORDER BY original_name")
                .fetch_all(&mut *conn)
                .await
                .unwrap();
        assert_eq!(names, vec!["pad.bin".to_string(), "trim.bin".to_string()]);
    }

    #[tokio::test]
    async fn append_existing_guid_adds_missing_children() {
        let tmp = TempDir::new().unwrap();
        let db = tmp.path().join("vault.db");
        let assets = tmp.path().join("assets");
        let header = r#"{"schema_version":3,"export":{"source":"imessage","tool":"test","tool_version":"0","owner_handle":null,"owner_display_name":null},"conversation":{"chat_identifier":"+15555550123","conversation_type":"individual","group_title":null,"participants":[{"handle":"+15555550123","display_name":null},{"handle":"+15555550999","display_name":null}],"stats":{"message_count":1,"attachment_count":0,"first_timestamp_unix_ms":1426183462000,"last_timestamp_unix_ms":1426183462000}}}"#;
        let first = write_jsonl(
            tmp.path(),
            "children-first.jsonl",
            &format!(
                "{header}\n{}\n",
                r#"{"guid":"g-children","timestamp_unix_ms":1426183462000,"direction":"incoming","service":"imessage","message_kind":"imessage","sender_handle":"+15555550123","sender_display_name":null,"subject":null,"text":"original body","attachments":[],"imessage":null,"source":null}"#
            ),
        );
        let options = ImportOptions::fixed(FixedImportArgs {
            assets_dir: &assets,
            asset_root: tmp.path(),
            contacts: None,
            overwrite_contacts: false,
            mode: ImportMode::Append,
            source: "imessage",
            account_id: TEST_ACCOUNT,
            fill_content_keys: false,
            import_id: None,
        });
        import_jsonl_files(&db, &[first], &options).await.unwrap();

        let second = write_jsonl(
            tmp.path(),
            "children-second.jsonl",
            &format!(
                "{header}\n{}\n",
                r#"{"guid":"g-children","timestamp_unix_ms":1426183462000,"direction":"incoming","service":"imessage","message_kind":"imessage","sender_handle":"+15555550123","sender_display_name":null,"subject":null,"text":"replacement body","attachments":[{"path":"attachments/missing.bin","original_name":"missing.bin","mime_type":"application/octet-stream","digest_sha256":null,"is_sticker":false,"transcription":null,"sticker_effect":null,"size_bytes":12,"missing_reason":"not_found"}],"imessage":{"is_reply":false,"in_reply_to_guid":null,"thread_originator_part":null,"num_replies":null,"is_deleted":false,"send_effect":null,"shared_location":null,"announcement":null,"read_receipt_rfc3339":null,"parts":null,"edits":null,"tapbacks":[{"emoji":null,"is_from_me":false,"kind":"liked","part_index":0,"sender":"+15555550999"}],"app":null,"balloon_bundle_id":null,"balloon_kind":null,"associated_guid":null,"associated_part":null,"tapback_kind":null,"tapback_emoji":null,"tapback_action":null},"source":null}"#
            ),
        );

        for _ in 0..2 {
            import_jsonl_files(&db, std::slice::from_ref(&second), &options)
                .await
                .unwrap();
        }

        let (_pool, mut conn) = open_verify(&db).await;
        let (body, attachments, tapbacks): (String, i64, i64) = sqlx::query_as(
            r#"
            SELECT m.body, COUNT(DISTINCT a.id), COUNT(DISTINCT t.id)
            FROM messages m
            LEFT JOIN attachments a ON a.message_id = m.id
            LEFT JOIN tapbacks t ON t.message_id = m.id
            WHERE m.guid = 'g-children'
            GROUP BY m.id
            "#,
        )
        .fetch_one(&mut *conn)
        .await
        .unwrap();
        assert_eq!(body, "original body");
        assert_eq!(attachments, 1);
        assert_eq!(tapbacks, 1);
    }

    #[tokio::test]
    async fn repeated_append_keeps_one_fts_posting_per_message() {
        let tmp = TempDir::new().unwrap();
        let db = tmp.path().join("vault.db");
        let assets = tmp.path().join("assets");
        let path = write_jsonl(
            tmp.path(),
            "fts-append.jsonl",
            r#"{"schema_version":3,"export":{"source":"imessage","tool":"test","tool_version":"0","owner_handle":null,"owner_display_name":null},"conversation":{"chat_identifier":"+15555550123","conversation_type":"individual","group_title":null,"participants":[{"handle":"+15555550123","display_name":null}],"stats":{"message_count":1,"attachment_count":0,"first_timestamp_unix_ms":1426183462000,"last_timestamp_unix_ms":1426183462000}}}
{"guid":"g-fts","timestamp_unix_ms":1426183462000,"direction":"incoming","service":"imessage","message_kind":"imessage","sender_handle":"+15555550123","sender_display_name":null,"subject":null,"text":"zzuniqueterm body","attachments":[],"imessage":null,"source":null}
"#,
        );
        let options = ImportOptions::fixed(FixedImportArgs {
            assets_dir: &assets,
            asset_root: tmp.path(),
            contacts: None,
            overwrite_contacts: false,
            mode: ImportMode::Append,
            source: "imessage",
            account_id: TEST_ACCOUNT,
            fill_content_keys: false,
            import_id: None,
        });

        import_jsonl_files(&db, std::slice::from_ref(&path), &options)
            .await
            .unwrap();
        // Rows of the full-text search index storage: a redundant re-index writes a new
        // segment even when the indexed text is unchanged.
        async fn index_rows(db: &Path) -> i64 {
            let (_pool, mut conn) = open_verify(db).await;
            sqlx::query_scalar("SELECT COUNT(*) FROM messages_fts_data")
                .fetch_one(&mut *conn)
                .await
                .unwrap()
        }
        let after_first_import = index_rows(&db).await;
        for _ in 0..2 {
            import_jsonl_files(&db, std::slice::from_ref(&path), &options)
                .await
                .unwrap();
        }
        assert_eq!(
            index_rows(&db).await,
            after_first_import,
            "repeated append must not write additional FTS index entries"
        );

        let (_pool, mut conn) = open_verify(&db).await;
        let messages: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM messages")
            .fetch_one(&mut *conn)
            .await
            .unwrap();
        assert_eq!(messages, 1);

        // `MATCH` collapses repeated postings for one rowid, so read the index
        // itself: fts5vocab reports how many entries each term really has.
        sqlx::query("CREATE VIRTUAL TABLE fts_vocab USING fts5vocab(messages_fts, row);")
            .execute(&mut *conn)
            .await
            .unwrap();
        let term_entries = async |conn: &mut AnyConnection| {
            let (docs, cnts): (i64, i64) = sqlx::query_as(
                "SELECT COALESCE(SUM(doc), 0), COALESCE(SUM(cnt), 0)
                 FROM fts_vocab WHERE term = 'zzuniqueterm'",
            )
            .fetch_one(&mut *conn)
            .await
            .unwrap();
            (docs, cnts)
        };
        assert_eq!(
            term_entries(&mut conn).await,
            (1, 1),
            "repeated append must not add extra index entries for an already indexed message"
        );
        let matches: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM messages_fts WHERE messages_fts MATCH 'zzuniqueterm'",
        )
        .fetch_one(&mut *conn)
        .await
        .unwrap();
        assert_eq!(matches, 1);

        sqlx::query("DELETE FROM messages WHERE guid = 'g-fts'")
            .execute(&mut *conn)
            .await
            .unwrap();
        assert_eq!(
            term_entries(&mut conn).await,
            (0, 0),
            "deleting the message must not leave stale search terms behind"
        );
    }

    #[tokio::test]
    async fn deferred_fts_indexes_attachment_text_after_promote() {
        let tmp = TempDir::new().unwrap();
        let db = tmp.path().join("vault.db");
        let assets = tmp.path().join("assets");
        fs::create_dir_all(&assets).unwrap();
        let att_dir = tmp.path().join("attachments");
        fs::create_dir_all(&att_dir).unwrap();
        fs::write(att_dir.join("receipt.pdf"), b"%PDF-fixture").unwrap();

        let path = write_jsonl(
            tmp.path(),
            "att.jsonl",
            r#"{"schema_version":3,"export":{"source":"imessage","tool":"test","tool_version":"0","owner_handle":null,"owner_display_name":null},"conversation":{"chat_identifier":"+15555550123","conversation_type":"individual","group_title":null,"participants":[{"handle":"+15555550123","display_name":null}],"stats":{"message_count":1,"attachment_count":1,"first_timestamp_unix_ms":1426183462000,"last_timestamp_unix_ms":1426183462000}}}
{"guid":"g-att","timestamp_unix_ms":1426183462000,"direction":"incoming","service":"sms","message_kind":"mms","sender_handle":"+15555550123","sender_display_name":null,"subject":null,"text":"see attached","attachments":[{"path":"attachments/receipt.pdf","original_name":"uniqueinvoice.pdf","mime_type":"application/pdf","digest_sha256":null,"is_sticker":false,"transcription":null,"sticker_effect":null}],"imessage":null,"source":null}
"#,
        );
        import_jsonl_files(
            &db,
            &[path],
            &ImportOptions::fixed(FixedImportArgs {
                assets_dir: &assets,
                asset_root: tmp.path(),
                contacts: None,
                overwrite_contacts: false,
                mode: ImportMode::Append,
                source: "imessage",
                account_id: TEST_ACCOUNT,
                fill_content_keys: false,
                import_id: None,
            }),
        )
        .await
        .unwrap();

        let (_pool, mut conn) = open_verify(&db).await;
        let hits: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM messages_fts WHERE messages_fts MATCH 'uniqueinvoice'",
        )
        .fetch_one(&mut *conn)
        .await
        .unwrap();
        assert_eq!(
            hits, 1,
            "attachment original_name must be searchable after deferred FTS"
        );
    }

    #[tokio::test]
    async fn promote_stamps_messages_with_import_id() {
        let tmp = TempDir::new().unwrap();
        let db = tmp.path().join("vault.db");
        let assets = tmp.path().join("assets");
        let path = write_jsonl(
            tmp.path(),
            "import-id.jsonl",
            r#"{"schema_version":3,"export":{"source":"imessage","tool":"test","tool_version":"0","owner_handle":null,"owner_display_name":null},"conversation":{"chat_identifier":"+15555550123","conversation_type":"individual","group_title":null,"participants":[{"handle":"+15555550123","display_name":null}],"stats":{"message_count":1,"attachment_count":0,"first_timestamp_unix_ms":1426183462000,"last_timestamp_unix_ms":1426183462000}}}
{"guid":"g-import","timestamp_unix_ms":1426183462000,"direction":"incoming","service":"sms","message_kind":"sms","sender_handle":"+15555550123","sender_display_name":null,"subject":null,"text":"linked","attachments":[],"imessage":null,"source":null}
"#,
        );

        let (_pool, mut conn) = open_verify(&db).await;
        schema::ensure_vault_schema(&mut conn).await.unwrap();
        crate::db::account_profile::ensure_account_row(&mut conn, TEST_ACCOUNT)
            .await
            .unwrap();
        let import_id = crate::db::vault_imports::start_import(
            &mut conn,
            TEST_ACCOUNT,
            "imessage",
            "append",
            Some("test"),
        )
        .await
        .unwrap();

        let stats = import_jsonl_files_on_conn(
            &mut conn,
            &[path],
            &ImportOptions::fixed(FixedImportArgs {
                assets_dir: &assets,
                asset_root: tmp.path(),
                contacts: None,
                overwrite_contacts: false,
                mode: ImportMode::Append,
                source: "imessage",
                account_id: TEST_ACCOUNT,
                fill_content_keys: false,
                import_id: Some(import_id),
            }),
            ImportSchemaMode::AssumeReady,
        )
        .await
        .unwrap();
        assert_eq!(stats.messages, 1);

        let stamped: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM messages WHERE import_id = $1")
            .bind(import_id)
            .fetch_one(&mut *conn)
            .await
            .unwrap();
        assert_eq!(stamped, 1);

        let row = crate::db::vault_imports::complete_import(
            &mut conn,
            TEST_ACCOUNT,
            import_id,
            &crate::db::vault_imports::CompleteImportArgs {
                ok: true,
                message_count: Some(stats.messages as i64),
                attachment_count: Some(0),
                bytes_uploaded: Some(0),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(row.status, "completed");
        assert_eq!(row.message_count, 1);

        let listed =
            crate::db::vault_imports::list_imports_for_account(&mut conn, TEST_ACCOUNT, 10)
                .await
                .unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].source, "imessage");
        assert!(!listed[0].started_at.is_empty());
        assert!(listed[0].finished_at.is_some());
        assert_eq!(
            crate::db::vault_imports::account_attachment_bytes(&mut conn, TEST_ACCOUNT)
                .await
                .unwrap(),
            0
        );
        assert!(
            crate::db::vault_imports::top_attachments_by_size(&mut conn, TEST_ACCOUNT, 5)
                .await
                .unwrap()
                .is_empty()
        );
    }

    #[tokio::test]
    async fn trunk_zero_phone_imports_digits_with_review_note() {
        let tmp = TempDir::new().unwrap();
        let db = tmp.path().join("vault.db");
        let assets = tmp.path().join("assets");
        let path = write_jsonl(
            tmp.path(),
            "trunk-zero.jsonl",
            r#"{"schema_version":3,"export":{"source":"imessage","tool":"test","tool_version":"0","owner_handle":null,"owner_display_name":null},"conversation":{"chat_identifier":"020 7946 0000","conversation_type":"individual","group_title":null,"participants":[{"handle":"020 7946 0000","display_name":null}],"stats":{"message_count":1,"attachment_count":0,"first_timestamp_unix_ms":1426183462000,"last_timestamp_unix_ms":1426183462000}}}
{"guid":"g-trunk-zero","timestamp_unix_ms":1426183462000,"direction":"incoming","service":"sms","message_kind":"sms","sender_handle":"020 7946 0000","sender_display_name":null,"subject":null,"text":"hello","attachments":[],"imessage":null,"source":null}
"#,
        );

        let (_pool, mut conn) = open_verify(&db).await;
        schema::ensure_vault_schema(&mut conn).await.unwrap();
        crate::db::account_profile::ensure_account_row(&mut conn, TEST_ACCOUNT)
            .await
            .unwrap();

        let stats = import_jsonl_files_on_conn(
            &mut conn,
            &[path],
            &ImportOptions::fixed(FixedImportArgs {
                assets_dir: &assets,
                asset_root: tmp.path(),
                contacts: None,
                overwrite_contacts: false,
                mode: ImportMode::Append,
                source: "imessage",
                account_id: TEST_ACCOUNT,
                fill_content_keys: false,
                import_id: None,
            }),
            ImportSchemaMode::AssumeReady,
        )
        .await
        .unwrap();
        assert_eq!(stats.phones_needing_review, 1);

        // Guarded policy: normalized mirrors the digits (never +02079460000)
        // and the handles row carries a review note.
        let (normalized, note): (String, Option<String>) = sqlx::query_as(
            "SELECT normalized, normalized_note FROM handles
             WHERE account_id = $1 AND handle_type = 'phone'",
        )
        .bind(TEST_ACCOUNT)
        .fetch_one(&mut *conn)
        .await
        .unwrap();
        assert_eq!(normalized, "02079460000");
        assert!(
            note.as_deref().is_some(),
            "trunk-zero import must carry a review note"
        );
    }

    #[tokio::test]
    async fn source_from_jsonl_stamps_export_source_and_assets() {
        use crate::config::PathsConfig;
        use crate::import_media::MediaMode;

        let tmp = TempDir::new().unwrap();
        let db = tmp.path().join("vault.db");
        let data_dir = tmp.path().join("data");
        let paths = PathsConfig {
            db: db.clone(),
            data_dir: data_dir.clone(),
            assets_dir: "assets".into(),
            assets_converted_dir: "assets_converted".into(),
        };
        let placeholder = tmp.path().join("unused-assets");
        fs::create_dir_all(tmp.path().join("media")).unwrap();
        fs::write(tmp.path().join("media/photo.jpg"), b"jpeg-bytes").unwrap();

        let path = write_jsonl(
            tmp.path(),
            "c.jsonl",
            r#"{"schema_version":3,"export":{"source":"go-sms-pro","tool":"test","tool_version":"0","owner_handle":null,"owner_display_name":null},"conversation":{"chat_identifier":"+15555550100","conversation_type":"individual","group_title":null,"participants":[{"handle":"+15555550100","display_name":null}],"stats":{"message_count":1,"attachment_count":1,"first_timestamp_unix_ms":1426183462000,"last_timestamp_unix_ms":1426183462000}}}
{"guid":"g1","timestamp_unix_ms":1426183462000,"direction":"incoming","service":"sms","message_kind":"sms","sender_handle":"+15555550100","sender_display_name":null,"subject":null,"text":"hi","attachments":[{"path":"media/photo.jpg","original_name":"photo.jpg","mime_type":"image/jpeg","digest_sha256":null,"is_sticker":false,"transcription":null,"sticker_effect":null}],"imessage":null,"source":null}
"#,
        );
        let stats = import_jsonl_files(
            &db,
            &[path],
            &ImportOptions {
                assets_dir: &placeholder,
                asset_root: tmp.path(),
                contacts: None,
                overwrite_contacts: false,
                mode: ImportMode::Replace,
                source: "",
                account_id: TEST_ACCOUNT,
                fill_content_keys: true,
                import_id: None,
                source_from_jsonl: true,
                paths: Some(&paths),
                media: MediaMode::Copy,
                wipe_sources: Some(vec!["go-sms-pro".into()]),
                contact_name_mode: ContactNameMode::default(),
            },
        )
        .await
        .unwrap();
        assert_eq!(stats.messages, 1);
        assert_eq!(stats.assets_copied, 1);

        let (_pool, mut conn) = open_verify(&db).await;
        let source: String = sqlx::query_scalar("SELECT source FROM messages WHERE guid = 'g1'")
            .fetch_one(&mut *conn)
            .await
            .unwrap();
        assert_eq!(source, "go-sms-pro");
        let assets_root = paths.assets_dir_for_account(TEST_ACCOUNT, "go-sms-pro");
        assert!(assets_root.is_dir());
    }

    #[tokio::test]
    async fn media_none_skips_attachment_copy() {
        use crate::config::PathsConfig;
        use crate::import_media::MediaMode;

        let tmp = TempDir::new().unwrap();
        let db = tmp.path().join("vault.db");
        let data_dir = tmp.path().join("data");
        let paths = PathsConfig {
            db: db.clone(),
            data_dir,
            assets_dir: "assets".into(),
            assets_converted_dir: "assets_converted".into(),
        };
        let placeholder = tmp.path().join("unused-assets");
        fs::create_dir_all(tmp.path().join("media")).unwrap();
        fs::write(tmp.path().join("media/photo.jpg"), b"jpeg-bytes").unwrap();

        let path = write_jsonl(
            tmp.path(),
            "c.jsonl",
            r#"{"schema_version":3,"export":{"source":"sms","tool":"test","tool_version":"0","owner_handle":null,"owner_display_name":null},"conversation":{"chat_identifier":"+15555550100","conversation_type":"individual","group_title":null,"participants":[{"handle":"+15555550100","display_name":null}],"stats":{"message_count":1,"attachment_count":1,"first_timestamp_unix_ms":1426183462000,"last_timestamp_unix_ms":1426183462000}}}
{"guid":"g1","timestamp_unix_ms":1426183462000,"direction":"incoming","service":"sms","message_kind":"sms","sender_handle":"+15555550100","sender_display_name":null,"subject":null,"text":"hi","attachments":[{"path":"media/photo.jpg","original_name":"photo.jpg","mime_type":"image/jpeg","digest_sha256":null,"is_sticker":false,"transcription":null,"sticker_effect":null}],"imessage":null,"source":null}
"#,
        );
        let stats = import_jsonl_files(
            &db,
            &[path],
            &ImportOptions {
                assets_dir: &placeholder,
                asset_root: tmp.path(),
                contacts: None,
                overwrite_contacts: false,
                mode: ImportMode::Replace,
                source: "",
                account_id: TEST_ACCOUNT,
                fill_content_keys: false,
                import_id: None,
                source_from_jsonl: true,
                paths: Some(&paths),
                media: MediaMode::None,
                wipe_sources: Some(vec!["sms".into()]),
                contact_name_mode: ContactNameMode::default(),
            },
        )
        .await
        .unwrap();
        assert_eq!(stats.messages, 1);
        assert_eq!(stats.attachments, 0);
        assert_eq!(stats.assets_copied, 0);
    }

    async fn seed_contact(db: &Path, handle: &str, preferred_name: &str) {
        let (_pool, mut conn) = open_verify(db).await;
        schema::ensure_vault_schema(&mut conn).await.unwrap();
        crate::db::account_profile::ensure_account_row(&mut conn, TEST_ACCOUNT)
            .await
            .unwrap();
        let contact_id: i64 = sqlx::query_scalar(
            "INSERT INTO contacts (account_id, preferred_name) VALUES ($1, $2) RETURNING id",
        )
        .bind(TEST_ACCOUNT)
        .bind(preferred_name)
        .fetch_one(&mut *conn)
        .await
        .unwrap();
        let handle_id: i64 = sqlx::query_scalar(
            "INSERT INTO handles (account_id, raw, normalized, handle_type, service)
             VALUES ($1, $2, $2, 'phone', 'phone') RETURNING id",
        )
        .bind(TEST_ACCOUNT)
        .bind(handle)
        .fetch_one(&mut *conn)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO contact_handles (account_id, handle_id, contact_id)
             VALUES ($1, $2, $3)",
        )
        .bind(TEST_ACCOUNT)
        .bind(handle_id)
        .bind(contact_id)
        .execute(&mut *conn)
        .await
        .unwrap();
    }

    async fn participant_name_alias(db: &Path) -> Option<String> {
        let (_pool, mut conn) = open_verify(db).await;
        let raw: Option<String> = sqlx::query_scalar("SELECT name_alias FROM participants LIMIT 1")
            .fetch_optional(&mut *conn)
            .await
            .unwrap()
            .flatten();
        trim_nonempty(raw)
    }

    async fn contact_handle_name_alias(db: &Path) -> Option<String> {
        let (_pool, mut conn) = open_verify(db).await;
        let raw: Option<String> =
            sqlx::query_scalar("SELECT name_alias FROM contact_handles LIMIT 1")
                .fetch_optional(&mut *conn)
                .await
                .unwrap()
                .flatten();
        trim_nonempty(raw)
    }

    #[tokio::test]
    async fn contact_name_mode_fill_missing_keeps_import_name() {
        let tmp = TempDir::new().unwrap();
        let db = tmp.path().join("vault.db");
        let assets = tmp.path().join("assets");
        seed_contact(&db, "+15555550123", "Vault Alice").await;
        let path = write_jsonl(
            tmp.path(),
            "named.jsonl",
            r#"{"schema_version":3,"export":{"source":"imessage","tool":"test","tool_version":"0","owner_handle":null,"owner_display_name":null},"conversation":{"chat_identifier":"+15555550123","conversation_type":"individual","group_title":null,"participants":[{"handle":"+15555550123","display_name":"Backup Bob"}],"stats":{"message_count":1,"attachment_count":0,"first_timestamp_unix_ms":1426183462000,"last_timestamp_unix_ms":1426183462000}}}
{"guid":"g-fill","timestamp_unix_ms":1426183462000,"direction":"incoming","service":"sms","message_kind":"sms","sender_handle":"+15555550123","sender_display_name":null,"subject":null,"text":"hi","attachments":[],"imessage":null,"source":null}
"#,
        );
        let mut opts = ImportOptions::fixed(FixedImportArgs {
            assets_dir: &assets,
            asset_root: tmp.path(),
            contacts: None,
            overwrite_contacts: false,
            mode: ImportMode::Append,
            source: "imessage",
            account_id: TEST_ACCOUNT,
            fill_content_keys: false,
            import_id: None,
        });
        opts.contact_name_mode = ContactNameMode::FillMissing;
        import_jsonl_files(&db, &[path], &opts).await.unwrap();
        assert_eq!(
            participant_name_alias(&db).await.as_deref(),
            Some("Backup Bob")
        );
    }

    #[tokio::test]
    async fn contact_name_mode_fill_missing_uses_vault_when_empty() {
        let tmp = TempDir::new().unwrap();
        let db = tmp.path().join("vault.db");
        let assets = tmp.path().join("assets");
        seed_contact(&db, "+15555550123", "Vault Alice").await;
        let path = write_jsonl(
            tmp.path(),
            "missing.jsonl",
            r#"{"schema_version":3,"export":{"source":"imessage","tool":"test","tool_version":"0","owner_handle":null,"owner_display_name":null},"conversation":{"chat_identifier":"+15555550123","conversation_type":"individual","group_title":null,"participants":[{"handle":"+15555550123","display_name":null}],"stats":{"message_count":1,"attachment_count":0,"first_timestamp_unix_ms":1426183462000,"last_timestamp_unix_ms":1426183462000}}}
{"guid":"g-missing","timestamp_unix_ms":1426183462000,"direction":"incoming","service":"sms","message_kind":"sms","sender_handle":"+15555550123","sender_display_name":null,"subject":null,"text":"hi","attachments":[],"imessage":null,"source":null}
"#,
        );
        let mut opts = ImportOptions::fixed(FixedImportArgs {
            assets_dir: &assets,
            asset_root: tmp.path(),
            contacts: None,
            overwrite_contacts: false,
            mode: ImportMode::Append,
            source: "imessage",
            account_id: TEST_ACCOUNT,
            fill_content_keys: false,
            import_id: None,
        });
        opts.contact_name_mode = ContactNameMode::FillMissing;
        import_jsonl_files(&db, &[path], &opts).await.unwrap();
        assert_eq!(
            participant_name_alias(&db).await.as_deref(),
            Some("Vault Alice")
        );
    }

    #[tokio::test]
    async fn contact_name_mode_as_is_ignores_vault() {
        let tmp = TempDir::new().unwrap();
        let db = tmp.path().join("vault.db");
        let assets = tmp.path().join("assets");
        seed_contact(&db, "+15555550123", "Vault Alice").await;
        let path = write_jsonl(
            tmp.path(),
            "as-is.jsonl",
            r#"{"schema_version":3,"export":{"source":"imessage","tool":"test","tool_version":"0","owner_handle":null,"owner_display_name":null},"conversation":{"chat_identifier":"+15555550123","conversation_type":"individual","group_title":null,"participants":[{"handle":"+15555550123","display_name":null}],"stats":{"message_count":1,"attachment_count":0,"first_timestamp_unix_ms":1426183462000,"last_timestamp_unix_ms":1426183462000}}}
{"guid":"g-asis","timestamp_unix_ms":1426183462000,"direction":"incoming","service":"sms","message_kind":"sms","sender_handle":"+15555550123","sender_display_name":null,"subject":null,"text":"hi","attachments":[],"imessage":null,"source":null}
"#,
        );
        let mut opts = ImportOptions::fixed(FixedImportArgs {
            assets_dir: &assets,
            asset_root: tmp.path(),
            contacts: None,
            overwrite_contacts: false,
            mode: ImportMode::Append,
            source: "imessage",
            account_id: TEST_ACCOUNT,
            fill_content_keys: false,
            import_id: None,
        });
        opts.contact_name_mode = ContactNameMode::AsIs;
        import_jsonl_files(&db, &[path], &opts).await.unwrap();
        assert_eq!(participant_name_alias(&db).await, None);
    }

    #[tokio::test]
    async fn contact_name_mode_overwrite_prefers_vault() {
        let tmp = TempDir::new().unwrap();
        let db = tmp.path().join("vault.db");
        let assets = tmp.path().join("assets");
        seed_contact(&db, "+15555550123", "Vault Alice").await;
        let path = write_jsonl(
            tmp.path(),
            "overwrite.jsonl",
            r#"{"schema_version":3,"export":{"source":"imessage","tool":"test","tool_version":"0","owner_handle":null,"owner_display_name":null},"conversation":{"chat_identifier":"+15555550123","conversation_type":"individual","group_title":null,"participants":[{"handle":"+15555550123","display_name":"Backup Bob"}],"stats":{"message_count":1,"attachment_count":0,"first_timestamp_unix_ms":1426183462000,"last_timestamp_unix_ms":1426183462000}}}
{"guid":"g-over","timestamp_unix_ms":1426183462000,"direction":"incoming","service":"sms","message_kind":"sms","sender_handle":"+15555550123","sender_display_name":null,"subject":null,"text":"hi","attachments":[],"imessage":null,"source":null}
"#,
        );
        let mut opts = ImportOptions::fixed(FixedImportArgs {
            assets_dir: &assets,
            asset_root: tmp.path(),
            contacts: None,
            overwrite_contacts: false,
            mode: ImportMode::Append,
            source: "imessage",
            account_id: TEST_ACCOUNT,
            fill_content_keys: false,
            import_id: None,
        });
        opts.contact_name_mode = ContactNameMode::Overwrite;
        import_jsonl_files(&db, &[path], &opts).await.unwrap();
        assert_eq!(
            participant_name_alias(&db).await.as_deref(),
            Some("Vault Alice")
        );
    }

    #[tokio::test]
    async fn contact_handle_alias_seeds_first_wins() {
        let tmp = TempDir::new().unwrap();
        let db = tmp.path().join("vault.db");
        let assets = tmp.path().join("assets");
        seed_contact(&db, "+15555550123", "Vault Alice").await;
        assert!(contact_handle_name_alias(&db).await.is_none());

        let path1 = write_jsonl(
            tmp.path(),
            "alias1.jsonl",
            r#"{"schema_version":3,"export":{"source":"imessage","tool":"test","tool_version":"0","owner_handle":null,"owner_display_name":null},"conversation":{"chat_identifier":"+15555550123","conversation_type":"individual","group_title":null,"participants":[{"handle":"+15555550123","display_name":"Backup Bob"}],"stats":{"message_count":1,"attachment_count":0,"first_timestamp_unix_ms":1426183462000,"last_timestamp_unix_ms":1426183462000}}}
{"guid":"g-alias1","timestamp_unix_ms":1426183462000,"direction":"incoming","service":"sms","message_kind":"sms","sender_handle":"+15555550123","sender_display_name":null,"subject":null,"text":"hi","attachments":[],"imessage":null,"source":null}
"#,
        );
        let mut opts = ImportOptions::fixed(FixedImportArgs {
            assets_dir: &assets,
            asset_root: tmp.path(),
            contacts: None,
            overwrite_contacts: false,
            mode: ImportMode::Append,
            source: "imessage",
            account_id: TEST_ACCOUNT,
            fill_content_keys: false,
            import_id: None,
        });
        opts.contact_name_mode = ContactNameMode::FillMissing;
        import_jsonl_files(&db, &[path1], &opts).await.unwrap();
        assert_eq!(
            contact_handle_name_alias(&db).await.as_deref(),
            Some("Backup Bob")
        );

        let path2 = write_jsonl(
            tmp.path(),
            "alias2.jsonl",
            r#"{"schema_version":3,"export":{"source":"imessage","tool":"test","tool_version":"0","owner_handle":null,"owner_display_name":null},"conversation":{"chat_identifier":"+15555550123","conversation_type":"individual","group_title":null,"participants":[{"handle":"+15555550123","display_name":"Other Name"}],"stats":{"message_count":1,"attachment_count":0,"first_timestamp_unix_ms":1426183463000,"last_timestamp_unix_ms":1426183463000}}}
{"guid":"g-alias2","timestamp_unix_ms":1426183463000,"direction":"incoming","service":"sms","message_kind":"sms","sender_handle":"+15555550123","sender_display_name":null,"subject":null,"text":"yo","attachments":[],"imessage":null,"source":null}
"#,
        );
        import_jsonl_files(&db, &[path2], &opts).await.unwrap();
        assert_eq!(
            contact_handle_name_alias(&db).await.as_deref(),
            Some("Backup Bob")
        );
    }

    #[tokio::test]
    async fn persists_missing_reason_with_null_sha256() {
        let tmp = TempDir::new().unwrap();
        let db = tmp.path().join("vault.db");
        let assets = tmp.path().join("assets");
        fs::create_dir_all(&assets).unwrap();

        let path = write_jsonl(
            tmp.path(),
            "missing-att.jsonl",
            r#"{"schema_version":3,"export":{"source":"sms-backup-restore","tool":"test","tool_version":"0","owner_handle":null,"owner_display_name":null},"conversation":{"chat_identifier":"+15555550123","conversation_type":"individual","group_title":null,"participants":[{"handle":"+15555550123","display_name":null}],"stats":{"message_count":1,"attachment_count":1,"first_timestamp_unix_ms":1426183462000,"last_timestamp_unix_ms":1426183462000}}}
{"guid":"g-missing","timestamp_unix_ms":1426183462000,"direction":"incoming","service":"sms","message_kind":"mms","sender_handle":"+15555550123","sender_display_name":null,"subject":null,"text":"see attached","attachments":[{"path":"attachments/gone.bin","original_name":"gone.bin","mime_type":"application/octet-stream","digest_sha256":null,"is_sticker":false,"transcription":null,"sticker_effect":null,"size_bytes":999,"missing_reason":"too_large"}],"imessage":null,"source":null}
"#,
        );
        let stats = import_jsonl_files(
            &db,
            &[path],
            &ImportOptions::fixed(FixedImportArgs {
                assets_dir: &assets,
                asset_root: tmp.path(),
                contacts: None,
                overwrite_contacts: false,
                mode: ImportMode::Append,
                source: "sms-backup-restore",
                account_id: TEST_ACCOUNT,
                fill_content_keys: false,
                import_id: None,
            }),
        )
        .await
        .unwrap();
        assert_eq!(stats.messages, 1);
        assert_eq!(stats.attachments, 1);

        let (_pool, mut conn) = open_verify(&db).await;
        let (sha256, missing_reason, size_bytes, original_name): (
            Option<String>,
            Option<String>,
            Option<i64>,
            Option<String>,
        ) = sqlx::query_as(
            "SELECT sha256, missing_reason, size_bytes, original_name FROM attachments LIMIT 1",
        )
        .fetch_one(&mut *conn)
        .await
        .unwrap();
        assert!(sha256.is_none());
        assert_eq!(missing_reason.as_deref(), Some("too_large"));
        assert_eq!(size_bytes, Some(999));
        assert_eq!(original_name.as_deref(), Some("gone.bin"));
    }

    #[tokio::test]
    async fn claimed_import_rejects_corrupt_existing_asset() {
        let tmp = TempDir::new().unwrap();
        let db = tmp.path().join("vault.db");
        let assets = tmp.path().join("assets");
        let sha = assets::sha256_hex(b"expected-asset");
        let corrupt = assets.join(assets::shard_rel_path(&sha, ".bin"));
        fs::create_dir_all(corrupt.parent().unwrap()).unwrap();
        fs::write(&corrupt, b"corrupt-asset").unwrap();

        let message = format!(
            r#"{{"guid":"g-corrupt-asset","timestamp_unix_ms":1426183462000,"direction":"incoming","service":"imessage","message_kind":"imessage","sender_handle":"+15555550123","sender_display_name":null,"subject":null,"text":"missing asset","attachments":[{{"path":"attachments/missing.bin","original_name":"missing.bin","mime_type":"application/octet-stream","digest_sha256":"{sha}","is_sticker":false,"transcription":null,"sticker_effect":null}}],"imessage":null,"source":null}}"#
        );
        let jsonl = format!(
            "{}\n{}\n",
            r#"{"schema_version":3,"export":{"source":"imessage","tool":"test","tool_version":"0","owner_handle":null,"owner_display_name":null},"conversation":{"chat_identifier":"+15555550123","conversation_type":"individual","group_title":null,"participants":[{"handle":"+15555550123","display_name":null}],"stats":{"message_count":1,"attachment_count":1,"first_timestamp_unix_ms":1426183462000,"last_timestamp_unix_ms":1426183462000}}}"#,
            message
        );
        let path = write_jsonl(tmp.path(), "corrupt-existing.jsonl", &jsonl);

        let stats = import_jsonl_files(
            &db,
            &[path],
            &ImportOptions::fixed(FixedImportArgs {
                assets_dir: &assets,
                asset_root: tmp.path(),
                contacts: None,
                overwrite_contacts: false,
                mode: ImportMode::Append,
                source: "imessage",
                account_id: TEST_ACCOUNT,
                fill_content_keys: false,
                import_id: None,
            }),
        )
        .await
        .unwrap();

        assert_eq!(stats.assets_deduped, 0);
        assert_eq!(stats.assets_missing, 1);
        let (_pool, mut conn) = open_verify(&db).await;
        let assets_path: Option<String> =
            sqlx::query_scalar("SELECT assets_path FROM attachments LIMIT 1")
                .fetch_one(&mut *conn)
                .await
                .unwrap();
        assert!(assets_path.is_none());
    }

    #[tokio::test]
    async fn rejects_attachment_path_traversal() {
        let tmp = TempDir::new().unwrap();
        let db = tmp.path().join("vault.db");
        let assets = tmp.path().join("assets");
        let export_dir = tmp.path().join("export");
        fs::create_dir_all(&assets).unwrap();
        fs::create_dir_all(&export_dir).unwrap();
        fs::write(tmp.path().join("secret.txt"), b"secret-bytes").unwrap();

        let path = write_jsonl(
            &export_dir,
            "traverse.jsonl",
            r#"{"schema_version":3,"export":{"source":"sms-backup-restore","tool":"test","tool_version":"0","owner_handle":null,"owner_display_name":null},"conversation":{"chat_identifier":"+15555550123","conversation_type":"individual","group_title":null,"participants":[{"handle":"+15555550123","display_name":null}],"stats":{"message_count":1,"attachment_count":1,"first_timestamp_unix_ms":1426183462000,"last_timestamp_unix_ms":1426183462000}}}
{"guid":"g-trav","timestamp_unix_ms":1426183462000,"direction":"incoming","service":"sms","message_kind":"mms","sender_handle":"+15555550123","sender_display_name":null,"subject":null,"text":"x","attachments":[{"path":"../secret.txt","original_name":"secret.txt","mime_type":"text/plain","digest_sha256":null,"is_sticker":false,"transcription":null,"sticker_effect":null,"size_bytes":12,"missing_reason":null}],"imessage":null,"source":null}
"#,
        );
        let err = import_jsonl_files(
            &db,
            &[path],
            &ImportOptions::fixed(FixedImportArgs {
                assets_dir: &assets,
                asset_root: &export_dir,
                contacts: None,
                overwrite_contacts: false,
                mode: ImportMode::Append,
                source: "sms-backup-restore",
                account_id: TEST_ACCOUNT,
                fill_content_keys: false,
                import_id: None,
            }),
        )
        .await
        .unwrap_err();
        assert!(
            err.to_string()
                .contains(message_ir_format::UNSAFE_ATTACHMENT_PATH_PREFIX),
            "expected path rejection, got: {err}"
        );
    }

    #[tokio::test]
    async fn failed_replace_keeps_existing_messages() {
        let tmp = TempDir::new().unwrap();
        let db = tmp.path().join("vault.db");
        let assets = tmp.path().join("assets");
        let export_dir = tmp.path().join("export");
        fs::create_dir_all(&assets).unwrap();
        fs::create_dir_all(&export_dir).unwrap();

        let first = write_jsonl(
            &export_dir,
            "ok.jsonl",
            r#"{"schema_version":3,"export":{"source":"sms-backup-restore","tool":"test","tool_version":"0","owner_handle":null,"owner_display_name":null},"conversation":{"chat_identifier":"+14075551234","conversation_type":"individual","group_title":null,"participants":[{"handle":"+14075551234","display_name":null}],"stats":{"message_count":1,"attachment_count":0,"first_timestamp_unix_ms":1426183462000,"last_timestamp_unix_ms":1426183462000}}}
{"guid":"g-keep-replace","timestamp_unix_ms":1426183462000,"direction":"incoming","service":"sms","message_kind":"sms","sender_handle":"+14075551234","sender_display_name":null,"subject":null,"text":"keep me","attachments":[],"imessage":null,"source":null}
"#,
        );
        import_jsonl_files(
            &db,
            &[first],
            &ImportOptions::fixed(FixedImportArgs {
                assets_dir: &assets,
                asset_root: &export_dir,
                contacts: None,
                overwrite_contacts: false,
                mode: ImportMode::Replace,
                source: "sms-backup-restore",
                account_id: TEST_ACCOUNT,
                fill_content_keys: false,
                import_id: None,
            }),
        )
        .await
        .unwrap();

        let bad = write_jsonl(
            &export_dir,
            "bad.jsonl",
            r#"{"schema_version":3,"export":{"source":"sms-backup-restore","tool":"test","tool_version":"0","owner_handle":null,"owner_display_name":null},"conversation":{"chat_identifier":"+14075551234","conversation_type":"individual","group_title":null,"participants":[{"handle":"+14075551234","display_name":null}],"stats":{"message_count":1,"attachment_count":1,"first_timestamp_unix_ms":1426183462000,"last_timestamp_unix_ms":1426183462000}}}
{"guid":"g-bad","timestamp_unix_ms":1426183462000,"direction":"incoming","service":"sms","message_kind":"mms","sender_handle":"+14075551234","sender_display_name":null,"subject":null,"text":"nope","attachments":[{"path":"../secret.txt","original_name":"secret.txt","mime_type":"text/plain","digest_sha256":null,"is_sticker":false,"transcription":null,"sticker_effect":null,"size_bytes":1,"missing_reason":null}],"imessage":null,"source":null}
"#,
        );
        let err = import_jsonl_files(
            &db,
            &[bad],
            &ImportOptions::fixed(FixedImportArgs {
                assets_dir: &assets,
                asset_root: &export_dir,
                contacts: None,
                overwrite_contacts: false,
                mode: ImportMode::Replace,
                source: "sms-backup-restore",
                account_id: TEST_ACCOUNT,
                fill_content_keys: false,
                import_id: None,
            }),
        )
        .await
        .unwrap_err();
        assert!(
            err.to_string()
                .contains(message_ir_format::UNSAFE_ATTACHMENT_PATH_PREFIX)
        );

        let (_pool, mut conn) = open_verify(&db).await;
        let body: String =
            sqlx::query_scalar("SELECT body FROM messages WHERE guid = 'g-keep-replace'")
                .fetch_one(&mut *conn)
                .await
                .unwrap();
        assert_eq!(body, "keep me");
        let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM messages")
            .fetch_one(&mut *conn)
            .await
            .unwrap();
        assert_eq!(n, 1);
    }
}
