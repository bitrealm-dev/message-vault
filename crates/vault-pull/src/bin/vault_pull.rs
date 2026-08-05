//! Headless CLI: pull messages from Message Vault into a message-ir folder.

use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Result, bail};
use clap::Parser;
use vault_pull::{
    DEFAULT_PAGE_LIMIT, ProgressEvent, VaultPullConfig, authenticate, compose_query, run,
};

#[derive(Debug, Parser)]
#[command(
    name = "vault-pull",
    about = "Pull messages from Message Vault into a message-ir export folder",
    long_about = "Calls GET /v1/export/messages with Fastmail-style search, downloads \
attachments via GET /v1/assets/{sha256}, and writes per-conversation .jsonl files plus \
attachments/.\n\nPrefer VAULT_KEY for the vault key. Prefer Message Vault → Vault Export \
for a GUI."
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

    /// Output directory for message-ir JSONL + attachments/
    #[arg(long)]
    out: PathBuf,

    /// Fastmail-style search query (optional)
    #[arg(long, default_value = "")]
    query: String,

    /// Only messages on or after this date (YYYY-MM-DD); adds after:
    #[arg(long)]
    after: Option<String>,

    /// Only messages before this date (YYYY-MM-DD); adds before:
    #[arg(long)]
    before: Option<String>,

    /// Restrict to one vault source id
    #[arg(long)]
    source: Option<String>,

    /// Skip attachment downloads
    #[arg(long)]
    skip_attachments: bool,

    /// Page size for /v1/export/messages
    #[arg(long, default_value_t = DEFAULT_PAGE_LIMIT)]
    page_limit: usize,

    /// Authenticate only; do not export
    #[arg(long)]
    auth_only: bool,
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

    let q = compose_query(&cli.query, cli.after.as_deref(), cli.before.as_deref());
    let cfg = VaultPullConfig {
        out_dir: cli.out,
        base_url: cli.url,
        username: cli.username,
        key: cli.key,
        query: q,
        // already composed into query
        after: None,
        before: None,
        source: cli.source,
        skip_attachments: cli.skip_attachments,
        page_limit: cli.page_limit,
        expected_messages: None,
        cancel: None,
    };

    let mut on_progress = |event: ProgressEvent| {
        if let ProgressEvent::Log(line) = event {
            println!("{line}");
        }
    };
    let report = run(&cfg, Some(&mut on_progress))?;
    println!(
        "ok conversations={} messages={} attachments_downloaded={} out={}",
        report.conversations, report.messages, report.attachments_downloaded, report.out_dir
    );
    Ok(if report.ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    })
}
