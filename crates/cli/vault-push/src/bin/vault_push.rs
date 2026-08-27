//! Command-line entry for uploading a conversation folder to Message Vault.
//!
//! Message Vault is the HTTP server that stores imported messages. The folder
//! holds JSON Lines files (one JSON object per line) plus an `attachments/`
//! directory.

use std::process::ExitCode;

use anyhow::{Result, bail};
use clap::Parser;
use vault_push::cli::Cli;
use vault_push::{ProgressEvent, VaultPushConfig, authenticate, run};

fn main() -> ExitCode {
    match real_main() {
        Ok(code) => code,
        Err(e) => {
            eprintln!("error: {e:#}");
            ExitCode::from(2)
        }
    }
}

/// Parse flags, check the API key, and either authenticate or run a full push.
///
/// # Errors
///
/// Returns an error when flags are invalid, the key is missing, login fails, or
/// the push itself fails.
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
        trust_export: cli.trust_export,
        max_retries: cli.max_retries,
        batch_size: cli.batch_size,
        asset_upload_workers: cli.asset_upload_workers,
        prepare_ahead: cli.prepare_ahead,
        prepare_workers: cli.prepare_workers,
        asset_multipart_threshold: vault_push::MAX_PROXY_BODY_BYTES,
        asset_max_bytes: cli.asset_max_bytes,
        report_path: cli.report,
        log_path: cli.log,
        journal_path: cli.journal,
        cancel: None,
        contact_name_mode: "fill_missing".into(),
        import_id: None,
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
