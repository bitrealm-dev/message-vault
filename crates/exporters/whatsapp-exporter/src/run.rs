//! Full export pipeline (wtsexporter/JSON convert) for CLI and GUI.

use crate::emit::{ExportReport, convert_json};
use crate::wtsexporter::{Platform, WtsexporterArgs, resolve_wtsexporter, run_wtsexporter};
use anyhow::{Context, Result, bail};
use message_vault_io_core::{RunResult, ExporterConfig, SourceConfig, WhatsappPlatform as CorePlatform};
use message_ir_format::ExportTransforms;
use std::env;
use std::fs;
use std::path::Path;

/// Resolve JSON (via wtsexporter or `--json`), convert, apply media/obfuscate via FormatSink.
pub fn run(config: &ExporterConfig) -> Result<RunResult> {
    let SourceConfig::Whatsapp(source) = &config.source else {
        bail!("whatsapp-exporter requires SourceConfig::Whatsapp");
    };
    message_vault_io_core::check_cancel(config.cancel.as_ref()).map_err(anyhow::Error::msg)?;
    let mut messages = Vec::new();

    let platform = match source.platform {
        Some(CorePlatform::Android) => Some(Platform::Android),
        Some(CorePlatform::Ios) => Some(Platform::Ios),
        None => None,
    };
    let input = config.primary_input().map(|p| p.to_path_buf());

    let (json_path, media_roots, _work_keep_alive) = if let Some(json) = &source.json {
        let mut media_roots = Vec::new();
        if let Ok(cwd) = env::current_dir() {
            media_roots.push(cwd);
        }
        if let Some(path) = &input {
            media_roots.push(path.clone());
        }
        if let Some(parent) = json.parent() {
            media_roots.push(parent.to_path_buf());
        }
        media_roots.sort();
        media_roots.dedup();
        (json.clone(), media_roots, None)
    } else {
        let platform =
            platform.ok_or_else(|| anyhow::anyhow!("platform is required unless json is set"))?;
        let input = match input {
            Some(path) => path,
            None => env::current_dir().context("resolve current working directory")?,
        };

        message_vault_io_core::check_cancel(config.cancel.as_ref()).map_err(anyhow::Error::msg)?;
        let bin = resolve_wtsexporter()?;
        fs::create_dir_all(&config.output)
            .with_context(|| format!("create {}", config.output.display()))?;
        // Scratch dir for wtsexporter cwd (iOS/Android extract) + result.json.
        // Kept until after convert so media copy can read extracted files.
        let work = tempfile::Builder::new()
            .prefix("wtsexporter-")
            .tempdir_in(&config.output)
            .context("create temp dir for wtsexporter")?;
        let json_out = work.path().join("result.json");

        // Cooperative only: we check cancel before and after the external process.
        // Killing wtsexporter mid-run is not implemented.
        message_vault_io_core::check_cancel(config.cancel.as_ref()).map_err(anyhow::Error::msg)?;
        let log = run_wtsexporter(
            &bin,
            &WtsexporterArgs {
                platform,
                input: input.clone(),
                work_dir: work.path().to_path_buf(),
                key: source.key.clone(),
                backup: source.backup.clone(),
                wa: source.wa.clone(),
                media: source.media.clone(),
                db: source.db.clone(),
                business: source.business,
            },
            &json_out,
        )?;
        message_vault_io_core::check_cancel(config.cancel.as_ref()).map_err(anyhow::Error::msg)?;

        if !log.trim().is_empty() {
            let trimmed = log.trim_end_matches('\n');
            messages.push(trimmed.to_string());
        }

        let kept = config.output.join("wtsexporter_result.json");
        fs::copy(&json_out, &kept).with_context(|| format!("copy JSON to {}", kept.display()))?;

        let mut media_roots = vec![work.path().to_path_buf(), input];
        if let Ok(cwd) = env::current_dir() {
            media_roots.push(cwd);
        }
        media_roots.sort();
        media_roots.dedup();

        (kept, media_roots, Some(work))
    };

    if !json_path.is_file() {
        bail!("JSON not found: {}", json_path.display());
    }

    message_vault_io_core::check_cancel(config.cancel.as_ref()).map_err(anyhow::Error::msg)?;
    let mut transforms = ExportTransforms::from_configs(&config.media, &config.obfuscate);
    transforms.log = config.log.clone();
    let needs_media_tools = transforms.needs_media_tools();
    let (report, sink) = convert_json(
        &json_path,
        &config.output,
        &config.date_range,
        transforms,
        &media_roots,
        config.output_format,
        config.cancel.as_ref(),
    )?;
    // Drop tempdir after convert (media files already copied).
    drop(_work_keep_alive);

    if !sink.media.errors.is_empty() && sink.media.processed == 0 && needs_media_tools {
        bail!("media processing failed for all candidate files");
    }
    messages.extend(sink.log_lines());

    messages.extend(report_summary_lines(&report, &config.output));
    Ok(RunResult { messages })
}

/// Format the convert summary the same way the CLI prints it.
fn report_summary_lines(report: &ExportReport, output: &Path) -> Vec<String> {
    let mut lines = vec![
        format!("Wrote {}", output.display()),
        format!("  conversations:      {}", report.conversations),
        format!("  messages:           {}", report.messages),
        format!("  attachments:        {}", report.attachments_saved),
    ];
    if report.attachments_missing > 0 {
        lines.push(format!(
            "  attachments missing:{}",
            report.attachments_missing
        ));
    }
    lines.push(format!(
        "  sent / received:    {} / {}",
        report.sent, report.received
    ));
    if report.skipped_invalid_date > 0 {
        lines.push(format!(
            "  skipped bad date:   {}",
            report.skipped_invalid_date
        ));
    }
    if report.skipped_out_of_range > 0 {
        lines.push(format!(
            "  skipped date range: {}",
            report.skipped_out_of_range
        ));
    }
    if !report.errors.is_empty() {
        lines.push(format!("  errors:             {}", report.errors.len()));
        for err in report.errors.iter().take(10) {
            lines.push(format!("    {err}"));
        }
    }
    lines
}

