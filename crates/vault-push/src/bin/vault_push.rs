//! Headless CLI: push a message-ir JSONL export folder to Message Vault.

use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Result, bail};
use clap::Parser;
use vault_push::{
    DEFAULT_ASSET_MAX_BYTES, DEFAULT_ASSET_UPLOAD_WORKERS, DEFAULT_BATCH_SIZE, ProgressEvent,
    VaultPushConfig, authenticate, run,
};

#[derive(Debug, Parser)]
#[command(
    name = "vault-push",
    about = "Push a Message Vault JSONL folder into Message Vault",
    long_about = "Reads per-conversation .jsonl files (message-ir schema v3) plus \
attachments/, uploads media by SHA-256, then imports message batches.\n\n\
Prefer VAULT_KEY for the vault key. Prefer Message Vault → Vault for a GUI."
)]
struct Cli {
    /// Vault base URL (e.g. http://127.0.0.1:8080)
    #[arg(long, env = "VAULT_URL")]
    url: String,

    /// Vault account username (optional; resolved from the vault key)
    #[arg(long, default_value = "")]
    username: String,

    /// Per-account Import API token (Vault key). Prefer VAULT_KEY env.
    #[arg(long, env = "VAULT_KEY")]
    key: String,

    /// Export directory containing .jsonl files and attachments/
    #[arg(long)]
    input: PathBuf,

    /// Import mode: append (resume-safe) or replace
    #[arg(long, default_value = "append")]
    mode: String,

    /// Continue after a failed conversation
    #[arg(long, default_value_t = true)]
    continue_on_error: bool,

    /// Ignore journal; re-upload assets and re-import messages
    #[arg(long)]
    force: bool,

    /// Import messages without uploading attachments
    #[arg(long)]
    skip_attachments: bool,

    /// Hash attachments and fail when on-disk sha256 differs from export digest_sha256
    #[arg(long, default_value_t = false)]
    verify_digests: bool,

    /// Max retries for transient HTTP errors
    #[arg(long, default_value_t = 3)]
    max_retries: u32,

    /// Messages per import request
    #[arg(long, default_value_t = DEFAULT_BATCH_SIZE)]
    batch_size: usize,

    /// Simultaneous attachment uploads; message imports remain sequential
    #[arg(long, default_value_t = DEFAULT_ASSET_UPLOAD_WORKERS)]
    asset_upload_workers: usize,

    /// Max attachment size in bytes (must not exceed vault server.asset_max_bytes)
    #[arg(long, default_value_t = DEFAULT_ASSET_MAX_BYTES)]
    asset_max_bytes: u64,

    /// Authenticate only; do not import
    #[arg(long)]
    auth_only: bool,

    /// Report JSON path (default: <input>/vault-push-report.json)
    #[arg(long)]
    report: Option<PathBuf>,

    /// Log path (default: <input>/vault-push.log)
    #[arg(long)]
    log: Option<PathBuf>,

    /// Journal path (default: <input>/.vault-import-state.jsonl)
    #[arg(long)]
    journal: Option<PathBuf>,
}

fn main() -> ExitCode {
    match real_main() {
        Ok(code) => code,
        Err(e) => {
            eprintln!("error: {e:#}");
            ExitCode::from(2)
        }
    }
}

fn real_main() -> Result<ExitCode> {
    let cli = Cli::parse();
    match cli.mode.as_str() {
        "append" | "replace" => {}
        other => bail!("invalid --mode {other:?} (expected append or replace)"),
    }
    if cli.key.trim().is_empty() {
        bail!("vault key is required (--key or VAULT_KEY)");
    }
    if cli.auth_only {
        let auth = authenticate(&cli.url, &cli.key, &cli.username)?;
        println!(
            "ok account={} username={}",
            auth.account_id,
            auth.username.unwrap_or_default()
        );
        return Ok(ExitCode::SUCCESS);
    }

    let cfg = VaultPushConfig {
        input: cli.input,
        base_url: cli.url,
        username: cli.username,
        key: cli.key,
        mode: cli.mode,
        continue_on_error: cli.continue_on_error,
        force: cli.force,
        skip_attachments: cli.skip_attachments,
        verify_digests: cli.verify_digests,
        max_retries: cli.max_retries,
        batch_size: cli.batch_size,
        asset_upload_workers: cli.asset_upload_workers,
        asset_multipart_threshold: vault_push::MAX_PROXY_BODY_BYTES,
        asset_max_bytes: cli.asset_max_bytes,
        report_path: cli.report,
        log_path: cli.log,
        journal_path: cli.journal,
        cancel: None,
    };

    let mut on_progress = |event: ProgressEvent| {
        if let ProgressEvent::Log(line) = event {
            println!("{line}");
        }
    };
    let report = run(&cfg, Some(&mut on_progress))?;
    println!("{}", vault_push::format_push_summary(&report));
    Ok(if report.ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    })
}
