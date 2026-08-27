//! Command-line entry for downloading messages from Message Vault.
//!
//! Message Vault is the HTTP server that stores imported messages. Output is a
//! folder of JSON Lines files (one JSON object per line) plus `attachments/`.

use std::process::ExitCode;

use anyhow::{Result, bail};
use clap::Parser;
use vault_pull::cli::Cli;
use vault_pull::{
    DEFAULT_ASSET_DOWNLOAD_WORKERS, ProgressEvent, VaultPullConfig, authenticate, compose_query,
    run,
};

fn main() -> ExitCode {
    match real_main() {
        Ok(code) => code,
        Err(e) => {
            eprintln!("error: {e:#}");
            ExitCode::from(2)
        }
    }
}

/// Parse flags, check the API key, and either authenticate or run a full download.
///
/// # Errors
///
/// Returns an error when the key is missing, login fails, or the download fails.
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
        asset_download_workers: DEFAULT_ASSET_DOWNLOAD_WORKERS,
        force: false,
        journal_path: None,
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
