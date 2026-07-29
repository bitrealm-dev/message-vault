//! Convert exporter CSV → vault NDJSON and POST one conversation at a time.

use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{bail, Context, Result};
use clap::Parser;
use csv_ingest::push::{run_push, PushConfig};
use csv_ingest::detect_export_source;

#[derive(Debug, Parser)]
#[command(
    name = "vault-push",
    about = "DEPRECATED: Convert staging CSV and push to message-vault-rs (use Message Exporters vault-push)"
)]
struct Cli {
    /// Local export directory (or single .csv) from message-exporters
    #[arg(long)]
    input: PathBuf,

    /// Directory for NDJSON `.json` files (default: same as --input)
    #[arg(long)]
    output: Option<PathBuf>,

    /// Source id (`go-sms-pro`, `imessage`, …). Detected from CSV when omitted.
    #[arg(long)]
    source_id: Option<String>,

    /// Vault base URL (e.g. http://127.0.0.1:8080)
    #[arg(long, env = "VAULT_URL")]
    url: Option<String>,

    /// Bearer API token (or env VAULT_API_TOKEN)
    #[arg(long, env = "VAULT_API_TOKEN")]
    token: Option<String>,

    /// Account username or UUID (optional for user API tokens from Settings; required for admin server.api_token)
    #[arg(long)]
    account: Option<String>,

    /// Import mode: append (default, resume-safe) or replace
    #[arg(long, default_value = "append")]
    mode: String,

    /// Run cross-source dedupe after each import (usually leave off for chunked push)
    #[arg(long)]
    dedupe: bool,

    /// Convert CSV → NDJSON only; do not POST
    #[arg(long)]
    convert_only: bool,

    /// Skip convert; POST existing `.json` under --output/--input
    #[arg(long)]
    skip_convert: bool,

    /// Continue after a conversation failure (still exit non-zero if any failed)
    #[arg(long)]
    continue_on_error: bool,

    /// Ignore checkpoint; re-POST all conversations
    #[arg(long)]
    force_repush: bool,

    /// Path for vault-push-report.json (default: under output/input)
    #[arg(long)]
    report: Option<PathBuf>,

    /// Path for vault-push.log (default: under output/input)
    #[arg(long)]
    log: Option<PathBuf>,

    /// Path for vault-push-done.json checkpoint (default: under output/input)
    #[arg(long)]
    checkpoint: Option<PathBuf>,

    /// HTTP retries per conversation
    #[arg(long, default_value_t = 3)]
    max_retries: u32,
}

fn main() -> ExitCode {
    eprintln!(
        "warning: this csv-ingest vault-push is deprecated. \
         Prefer Message Exporters JSONL vault-push \
         (cargo run -p message-vault-client --bin vault-push --features cli)."
    );
    match run() {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::from(1),
        Err(e) => {
            eprintln!("error: {e:#}");
            ExitCode::from(2)
        }
    }
}

fn run() -> Result<bool> {
    let cli = Cli::parse();
    if cli.convert_only && cli.skip_convert {
        bail!("use either --convert-only or --skip-convert, not both");
    }

    let output = cli.output.clone().unwrap_or_else(|| {
        if cli.input.is_file() {
            cli.input
                .parent()
                .map(|p| p.to_path_buf())
                .unwrap_or_else(|| PathBuf::from("."))
        } else {
            cli.input.clone()
        }
    });

    let source_id = match &cli.source_id {
        Some(id) => id.clone(),
        None => detect_export_source(&cli.input)?
            .ok_or_else(|| anyhow::anyhow!("could not detect source; pass --source-id"))?,
    };

    match cli.mode.to_ascii_lowercase().as_str() {
        "replace" | "append" => {}
        other => bail!("invalid --mode {other:?} (expected replace or append)"),
    }

    if cli.convert_only {
        let report = csv_ingest::convert_directory(&cli.input, &output, &source_id)?;
        println!("Converted CSV → NDJSON under {}", output.display());
        println!("  source:        {source_id}");
        println!("  conversations: {}", report.conversations);
        println!("  messages:      {}", report.messages);
        return Ok(report.conversations > 0 || report.errors.is_empty());
    }

    let url = cli
        .url
        .filter(|s| !s.trim().is_empty())
        .context("--url or VAULT_URL is required to push")?;
    let token = cli
        .token
        .filter(|s| !s.trim().is_empty())
        .context("--token or VAULT_API_TOKEN is required to push")?;
    let account = cli
        .account
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_default();

    let base = if output.is_dir() {
        output.clone()
    } else {
        output
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| PathBuf::from("."))
    };

    let cfg = PushConfig {
        input: cli.input,
        output,
        source_id,
        base_url: url,
        token,
        account,
        mode: cli.mode.to_ascii_lowercase(),
        dedupe: cli.dedupe,
        skip_convert: cli.skip_convert,
        continue_on_error: cli.continue_on_error,
        force_repush: cli.force_repush,
        report_path: cli
            .report
            .unwrap_or_else(|| base.join("vault-push-report.json")),
        log_path: cli.log.unwrap_or_else(|| base.join("vault-push.log")),
        checkpoint_path: cli
            .checkpoint
            .unwrap_or_else(|| base.join("vault-push-done.json")),
        max_retries: cli.max_retries,
    };

    let report = run_push(&cfg)?;
    Ok(report.ok)
}
