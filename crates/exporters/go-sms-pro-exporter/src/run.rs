//! Full export pipeline for CLI and in-process GUI.

use crate::emit::{ExportReport, convert_export};
use anyhow::{Result, bail};
use contacts::resolve_contacts_cli;
use message_vault_io_core::{RunResult, ExporterConfig, SourceConfig};
use message_ir_format::ExportTransforms;
use std::path::Path;

/// Resolve contacts, convert, apply media/obfuscate via FormatSink.
pub fn run(config: &ExporterConfig) -> Result<RunResult> {
    let SourceConfig::GoSmsPro(source) = &config.source else {
        bail!("go-sms-pro-exporter requires SourceConfig::GoSmsPro");
    };
    let input = config.require_input().map_err(anyhow::Error::msg)?;
    let mut messages = Vec::new();
    let (contacts_path, vcf) = config.contacts_csv_vcf();
    let log_fn = |line: &str| config.emit_log(line);
    let (contacts, _) = resolve_contacts_cli(contacts_path, vcf, Some(&log_fn))?;
    let mut transforms = ExportTransforms::from_configs(&config.media, &config.obfuscate);
    transforms.log = config.log.clone();
    let (report, sink) = convert_export(
        input,
        &config.output,
        &source.owner_phones,
        &contacts,
        &config.date_range,
        transforms,
        config.output_format,
        config.cancel.as_ref(),
    )?;
    if !sink.media.errors.is_empty() && sink.media.processed == 0 && config.media.mode.needs_tools()
    {
        anyhow::bail!("media processing failed for all candidate files");
    }
    messages.extend(sink.log_lines());
    messages.extend(report_summary_lines(&report, &config.output));
    Ok(RunResult { messages })
}

/// Format the convert summary the same way the CLI prints it.
fn report_summary_lines(report: &ExportReport, output: &Path) -> Vec<String> {
    let mut lines = vec![
        format!("Wrote {}", output.display()),
        format!("  conversations:     {}", report.conversations),
        format!("  XML messages seen: {}", report.xml_messages_seen),
        format!("  PDU messages:      {}", report.pdu_messages),
        format!("  PDU group MMS:     {}", report.pdu_group_messages),
        format!("  attachments:       {}", report.attachments_saved),
        format!("  sent / received:   {} / {}", report.sent, report.received),
    ];
    if report.skipped_invalid_date > 0 {
        lines.push(format!(
            "  skipped bad date:  {}",
            report.skipped_invalid_date
        ));
    }
    if report.skipped_out_of_range > 0 {
        lines.push(format!(
            "  skipped date range:{}",
            report.skipped_out_of_range
        ));
    }
    if report.skipped_unknown_type > 0 {
        lines.push(format!(
            "  skipped bad type:  {}",
            report.skipped_unknown_type
        ));
    }
    if report.skipped_unknown_address > 0 {
        lines.push(format!(
            "  skipped invalid address: {}",
            report.skipped_unknown_address
        ));
        lines.push(format!(
            "  invalid-address detail: {}/skipped_invalid_address.csv",
            output.display()
        ));
        for d in report.skipped_unknown_address_details.iter().take(10) {
            lines.push(format!(
                "    invalid address: {} address={:?} contact={:?} type={} date_ms={} body={:?}",
                d.xml_file, d.address, d.contact_name, d.android_type, d.date_ms, d.body,
            ));
        }
        let extra = report.skipped_unknown_address_details.len().saturating_sub(10) as u64
            + report.skipped_unknown_address_details_more;
        if extra > 0 {
            lines.push(format!(
                "    … and {extra} more (see skipped_invalid_address.csv)"
            ));
        }
    }
    if report.skipped_empty_pdu > 0 {
        lines.push(format!("  skipped empty pdu: {}", report.skipped_empty_pdu));
        lines.push(format!(
            "  empty-pdu detail:   {}/skipped_empty_pdu.csv",
            output.display()
        ));
    }
    if report.skipped_no_other_party > 0 {
        lines.push(format!(
            "  skipped no party:  {}",
            report.skipped_no_other_party
        ));
        lines.push(format!(
            "  no-party detail:    {}/skipped_no_party.csv",
            output.display()
        ));
        for d in report.skipped_no_other_party_details.iter().take(10) {
            lines.push(format!(
                "    no party: {} participants=[{}] sent={} from={} to={}",
                d.pdu_filename, d.participants, d.is_sent as u8, d.has_from as u8, d.has_to as u8,
            ));
        }
        let extra = report.skipped_no_other_party_details.len().saturating_sub(10) as u64
            + report.skipped_no_other_party_details_more;
        if extra > 0 {
            lines.push(format!(
                "    … and {extra} more (see skipped_no_party.csv)"
            ));
        }
    }
    if report.skipped_unparseable_pdu > 0 {
        lines.push(format!(
            "  skipped bad PDU:   {}",
            report.skipped_unparseable_pdu
        ));
    }
    if !report.errors.is_empty() {
        lines.push(format!("  errors:            {}", report.errors.len()));
        for err in report.errors.iter().take(10) {
            lines.push(format!("    {err}"));
        }
    }
    lines
}

