//! Convert an existing Message Vault output directory to another format.

use message_ir::ConversationDocument;
use message_ir_format::{
    CSV_HEADERS, ExportTransforms, FormatSink, FormatSinkResult, SbrReadOptions,
    clean_previous_ir_output, read_conversation_csv, read_conversation_eml_dir,
    read_conversation_json, read_conversation_jsonl, read_conversation_mbox, read_sbr_documents,
};
use anyhow::{Context, Result, bail};
use message_vault_io_core::{ExporterConfig, OutputFormat};
pub use message_vault_io_core::RunResult;
use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

/// Convert the prior export in `config.inputs[0]` to `config.output_format`.
///
/// # Errors
///
/// Returns an error when the input is missing or ambiguous, the output is the
/// input directory, an artifact cannot be read, or the output cannot be written.
pub fn run(config: &ExporterConfig) -> Result<RunResult> {
    let input = config.require_input().map_err(anyhow::Error::msg)?;
    if config.output.as_os_str().is_empty() {
        bail!("output directory is required");
    }
    let report = convert_export(input, config)?;
    Ok(RunResult {
        messages: report.log_lines(),
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct DetectedExport {
    format: OutputFormat,
}

#[derive(Debug, Default)]
struct ReexportReport {
    detected_format: String,
    conversations: usize,
    sink: FormatSinkResult,
}

impl ReexportReport {
    fn log_lines(&self) -> Vec<String> {
        let mut lines = vec![
            format!("Detected input format: {}", self.detected_format),
            format!("Conversations: {}", self.conversations),
        ];
        lines.extend(self.sink.log_lines());
        lines
    }
}

fn convert_export(input_dir: &Path, config: &ExporterConfig) -> Result<ReexportReport> {
    let input_canon = fs::canonicalize(input_dir)
        .with_context(|| format!("canonicalize input {}", input_dir.display()))?;
    if config.output.exists() {
        let output_canon = fs::canonicalize(&config.output)
            .with_context(|| format!("canonicalize output {}", config.output.display()))?;
        if input_canon == output_canon {
            bail!("input and output directories must be different");
        }
    }

    let detected = detect_ir_export(input_dir)?;
    let mut transforms = ExportTransforms::from_configs(&config.media, &config.obfuscate);
    transforms.log = config.log.clone();
    let copy_attachments = transforms.copies_attachments();

    fs::create_dir_all(&config.output)
        .with_context(|| format!("create {}", config.output.display()))?;
    clean_previous_ir_output(&config.output)?;

    if copy_attachments {
        copy_attachments_dir(input_dir, &config.output)?;
    }

    let documents = load_documents(input_dir, detected, config, copy_attachments)?;
    if documents.is_empty() {
        bail!("no conversations loaded from {}", input_dir.display());
    }

    let conversations = documents.len();
    let mut sink = FormatSink::open(&config.output, config.output_format, transforms)?;
    for document in documents {
        sink.write_document(document)?;
    }
    let sink = sink.finish()?;

    Ok(ReexportReport {
        detected_format: detected.format.as_str().to_string(),
        conversations,
        sink,
    })
}

fn load_documents(
    input_dir: &Path,
    detected: DetectedExport,
    config: &ExporterConfig,
    copy_attachments: bool,
) -> Result<Vec<ConversationDocument>> {
    if detected.format == OutputFormat::Xml {
        let attachments_dir = config.output.join("attachments");
        let (documents, report) = read_sbr_documents(
            input_dir,
            SbrReadOptions {
                owner_phones: &[],
                date_range: &config.date_range,
                attachments_dir: Some(&attachments_dir),
                copy_attachments,
                keep_attachment_bytes: false,
                cancel: config.cancel.as_ref(),
            },
        )?;
        for error in report.errors.iter().take(5) {
            config.emit_log(format!("xml warning: {error}"));
        }
        return Ok(documents);
    }

    list_artifacts(input_dir, detected.format)?
        .into_iter()
        .map(|path| match detected.format {
            OutputFormat::Json => read_conversation_json(&path),
            OutputFormat::Jsonl => read_conversation_jsonl(&path),
            OutputFormat::Csv => read_conversation_csv(&path),
            OutputFormat::Mbox => read_conversation_mbox(&path),
            OutputFormat::Eml => read_conversation_eml_dir(&path),
            OutputFormat::Xml => unreachable!("XML handled above"),
        })
        .collect()
}

fn detect_ir_export(input_dir: &Path) -> Result<DetectedExport> {
    if !input_dir.is_dir() {
        bail!("input is not a directory: {}", input_dir.display());
    }

    let mut present = Vec::new();
    let mut samples = Vec::new();
    for entry in fs::read_dir(input_dir).with_context(|| format!("read {}", input_dir.display()))? {
        let entry = entry?;
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if ignored_artifact(&name) {
            continue;
        }
        let format = if path.is_file() {
            let extension = path
                .extension()
                .and_then(|extension| extension.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            match extension.as_str() {
                "xml" if name.eq_ignore_ascii_case("smses.xml") || looks_like_smses(&path) => {
                    Some(OutputFormat::Xml)
                }
                "json" if looks_like_ir_json(&path)? => Some(OutputFormat::Json),
                "jsonl" | "ndjson" if looks_like_ir_jsonl(&path)? => Some(OutputFormat::Jsonl),
                "csv" if looks_like_ir_csv(&path)? => Some(OutputFormat::Csv),
                "mbox" => Some(OutputFormat::Mbox),
                _ => None,
            }
        } else if path.is_dir() && dir_has_eml(&path)? {
            Some(OutputFormat::Eml)
        } else {
            None
        };
        if let Some(format) = format {
            if !present.contains(&format) {
                present.push(format);
            }
            samples.push(if path.is_dir() {
                format!("{name}/")
            } else {
                name
            });
        }
    }
    present.sort_by_key(|format| match format {
        OutputFormat::Xml => 0,
        OutputFormat::Json => 1,
        OutputFormat::Jsonl => 2,
        OutputFormat::Csv => 3,
        OutputFormat::Mbox => 4,
        OutputFormat::Eml => 5,
    });

    match present.as_slice() {
        [format] => Ok(DetectedExport { format: *format }),
        [] => bail!(
            "unsupported input: no Message Vault IR export found in {} \
             (expected smses.xml, *.json, *.jsonl, *.csv, *.mbox, or EML folders)",
            input_dir.display()
        ),
        formats => bail!(
            "unsupported input: mixed formats in {} ({}); found: {}",
            input_dir.display(),
            formats
                .iter()
                .map(|format| format.as_str())
                .collect::<Vec<_>>()
                .join(", "),
            samples.join(", ")
        ),
    }
}

fn list_artifacts(input_dir: &Path, format: OutputFormat) -> Result<Vec<PathBuf>> {
    let mut paths = Vec::new();
    for entry in fs::read_dir(input_dir)? {
        let entry = entry?;
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if ignored_artifact(&name) {
            continue;
        }
        let matches = match format {
            OutputFormat::Xml => {
                path.is_file()
                    && (name.eq_ignore_ascii_case("smses.xml") || looks_like_smses(&path))
            }
            OutputFormat::Json => {
                path.is_file()
                    && path.extension().and_then(|extension| extension.to_str()) == Some("json")
                    && looks_like_ir_json(&path)?
            }
            OutputFormat::Jsonl => {
                path.is_file()
                    && path
                        .extension()
                        .and_then(|extension| extension.to_str())
                        .is_some_and(|extension| extension == "jsonl" || extension == "ndjson")
                    && looks_like_ir_jsonl(&path)?
            }
            OutputFormat::Csv => {
                path.is_file()
                    && path.extension().and_then(|extension| extension.to_str()) == Some("csv")
                    && looks_like_ir_csv(&path)?
            }
            OutputFormat::Mbox => {
                path.is_file()
                    && path.extension().and_then(|extension| extension.to_str()) == Some("mbox")
            }
            OutputFormat::Eml => path.is_dir() && dir_has_eml(&path)?,
        };
        if matches {
            paths.push(path);
        }
    }
    paths.sort();
    if paths.is_empty() {
        bail!(
            "no {} artifacts found in {}",
            format.as_str(),
            input_dir.display()
        );
    }
    Ok(paths)
}

fn ignored_artifact(name: &str) -> bool {
    name == "attachments"
        || name.starts_with('.')
        || name.ends_with(".meta.json")
        || name.ends_with(".tmp")
        || name.ends_with(".xml.tmp")
        || name.ends_with(".xml.sbrbody")
}

fn looks_like_smses(path: &Path) -> bool {
    let Ok(file) = File::open(path) else {
        return false;
    };
    let mut first_line = String::new();
    let _ = BufReader::new(file).read_line(&mut first_line);
    first_line.to_ascii_lowercase().contains("<smses")
}

fn looks_like_ir_json(path: &Path) -> Result<bool> {
    let raw = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    let value: serde_json::Value = match serde_json::from_str(&raw) {
        Ok(value) => value,
        Err(_) => return Ok(false),
    };
    Ok(
        value.get("schema_version").and_then(|value| value.as_u64()) == Some(message_ir::SCHEMA_VERSION as u64)
            && value.get("export").is_some()
            && value.get("conversation").is_some()
            && value.get("messages").is_some(),
    )
}

fn looks_like_ir_jsonl(path: &Path) -> Result<bool> {
    let file = File::open(path).with_context(|| format!("open {}", path.display()))?;
    let Some(Ok(first_line)) = BufReader::new(file).lines().next() else {
        return Ok(false);
    };
    let value: serde_json::Value = match serde_json::from_str(&first_line) {
        Ok(value) => value,
        Err(_) => return Ok(false),
    };
    Ok(
        value.get("schema_version").and_then(|value| value.as_u64()) == Some(message_ir::SCHEMA_VERSION as u64)
            && value.get("export").is_some()
            && value.get("conversation").is_some()
            && value.get("messages").is_none(),
    )
}

fn looks_like_ir_csv(path: &Path) -> Result<bool> {
    let file = File::open(path).with_context(|| format!("open {}", path.display()))?;
    let mut reader = csv::ReaderBuilder::new()
        .flexible(true)
        .from_reader(BufReader::new(file));
    let Ok(headers) = reader.headers() else {
        return Ok(false);
    };
    let headers: HashSet<&str> = headers.iter().collect();
    Ok(CSV_HEADERS.iter().all(|header| headers.contains(header)))
}

fn dir_has_eml(dir: &Path) -> Result<bool> {
    for entry in fs::read_dir(dir)? {
        if entry?
            .path()
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("eml"))
        {
            return Ok(true);
        }
    }
    Ok(false)
}

fn copy_attachments_dir(input_dir: &Path, output_dir: &Path) -> Result<()> {
    let source = input_dir.join("attachments");
    if !source.is_dir() {
        return Ok(());
    }
    let destination = output_dir.join("attachments");
    fs::create_dir_all(&destination)?;
    copy_dir_recursive(&source, &destination)
}

fn copy_dir_recursive(source: &Path, destination: &Path) -> Result<()> {
    for entry in fs::read_dir(source).with_context(|| format!("read {}", source.display()))? {
        let entry = entry?;
        let from = entry.path();
        let to = destination.join(entry.file_name());
        if from.is_dir() {
            fs::create_dir_all(&to)?;
            copy_dir_recursive(&from, &to)?;
        } else {
            fs::copy(&from, &to)
                .with_context(|| format!("copy {} → {}", from.display(), to.display()))?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use message_ir::{
        ConversationMeta, ConversationStats, ExportMeta, IrConversationType, IrDirection,
        IrMessage, IrMessageKind, IrParticipant, IrService, SCHEMA_VERSION,
    };
    use message_ir_format::{read_conversation_csv, read_conversation_json};
    use message_vault_io_core::{
        MediaConfig, FormatConfig, ObfuscateConfig, SourceConfig,
    };

    fn sample_doc() -> ConversationDocument {
        let mut document = ConversationDocument {
            schema_version: SCHEMA_VERSION,
            export: ExportMeta {
                source: "sms-backup-restore".into(),
                tool: "SMS Backup & Restore".into(),
                tool_version: "10.26.003".into(),
                owner_handle: Some("+15555550100".into()),
                owner_display_name: Some("Me".into()),
            },
            conversation: ConversationMeta {
                chat_identifier: "+15555550101".into(),
                conversation_type: IrConversationType::Individual,
                group_title: None,
                participants: vec![IrParticipant {
                    handle: "+15555550101".into(),
                    display_name: Some("Sam".into()),
                }],
                stats: ConversationStats::default(),
            },
            messages: vec![IrMessage {
                guid: "aabbccddeeff00112233445566778899".into(),
                timestamp_unix_ms: 1_400_773_261_000,
                direction: IrDirection::Incoming,
                service: IrService::Sms,
                message_kind: IrMessageKind::Sms,
                sender_handle: Some("+15555550101".into()),
                sender_display_name: Some("Sam".into()),
                subject: None,
                text: "hello reexport".into(),
                attachments: vec![],
                imessage: None,
                source: None,
            }],
            packaging_stem_suffix: None,
        };
        document.finalize_stats();
        document
    }

    fn write_fixture(dir: &Path, format: OutputFormat) {
        fs::create_dir_all(dir).unwrap();
        clean_previous_ir_output(dir).unwrap();
        let mut sink = FormatSink::open(dir, format, ExportTransforms::none()).unwrap();
        sink.write_document(sample_doc()).unwrap();
        sink.finish().unwrap();
    }

    fn config(input: &Path, output: &Path, output_format: OutputFormat) -> ExporterConfig {
        ExporterConfig {
            inputs: vec![input.to_path_buf()],
            output: output.to_path_buf(),
            date_range: Default::default(),
            contacts: None,
            obfuscate: ObfuscateConfig::default(),
            media: MediaConfig::default(),
            cancel: None,
            log: None,
            output_format,
            source: SourceConfig::Format(FormatConfig {}),
        }
    }

    #[test]
    fn detect_json_and_convert_to_csv() {
        let source = tempfile::tempdir().unwrap();
        write_fixture(source.path(), OutputFormat::Json);
        assert_eq!(
            detect_ir_export(source.path()).unwrap().format,
            OutputFormat::Json
        );
        let destination = tempfile::tempdir().unwrap();
        let report = convert_export(
            source.path(),
            &config(source.path(), destination.path(), OutputFormat::Csv),
        )
        .unwrap();
        assert_eq!(report.conversations, 1);
        assert_eq!(report.detected_format, "json");
        let csv = fs::read_dir(destination.path())
            .unwrap()
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .find(|path| path.extension().and_then(|extension| extension.to_str()) == Some("csv"))
            .expect("csv");
        assert_eq!(
            read_conversation_csv(&csv).unwrap().messages[0].text,
            "hello reexport"
        );
    }

    #[test]
    fn convert_csv_to_json() {
        let source = tempfile::tempdir().unwrap();
        write_fixture(source.path(), OutputFormat::Csv);
        let destination = tempfile::tempdir().unwrap();
        convert_export(
            source.path(),
            &config(source.path(), destination.path(), OutputFormat::Json),
        )
        .unwrap();
        let json = fs::read_dir(destination.path())
            .unwrap()
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .find(|path| {
                path.extension().and_then(|extension| extension.to_str()) == Some("json")
                    && !path
                        .file_name()
                        .and_then(|name| name.to_str())
                        .is_some_and(|name| name.ends_with(".meta.json"))
            })
            .expect("json");
        assert_eq!(
            read_conversation_json(&json).unwrap().messages[0].text,
            "hello reexport"
        );
    }

    #[test]
    fn convert_json_to_xml() {
        let source = tempfile::tempdir().unwrap();
        write_fixture(source.path(), OutputFormat::Json);
        let destination = tempfile::tempdir().unwrap();
        convert_export(
            source.path(),
            &config(source.path(), destination.path(), OutputFormat::Xml),
        )
        .unwrap();
        assert!(destination.path().join("smses.xml").is_file());
    }

    #[test]
    fn convert_xml_with_ir_reader() {
        let source = tempfile::tempdir().unwrap();
        fs::write(
            source.path().join("smses.xml"),
            r#"<smses><sms protocol="0" address="+15555550101" date="1400773261000" type="1" body="hello xml" contact_name="Sam"/></smses>"#,
        )
        .unwrap();
        let destination = tempfile::tempdir().unwrap();
        let report = convert_export(
            source.path(),
            &config(source.path(), destination.path(), OutputFormat::Json),
        )
        .unwrap();
        assert_eq!(report.detected_format, "xml");
        assert_eq!(report.conversations, 1);
        let json = fs::read_dir(destination.path())
            .unwrap()
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .find(|path| path.extension().and_then(|extension| extension.to_str()) == Some("json"))
            .expect("json");
        assert_eq!(
            read_conversation_json(&json).unwrap().messages[0].text,
            "hello xml"
        );
    }

    #[test]
    fn mixed_formats_error() {
        let source = tempfile::tempdir().unwrap();
        write_fixture(source.path(), OutputFormat::Json);
        let mut sink =
            FormatSink::open(source.path(), OutputFormat::Csv, ExportTransforms::none()).unwrap();
        sink.write_document(sample_doc()).unwrap();
        sink.finish().unwrap();
        let error = detect_ir_export(source.path()).unwrap_err().to_string();
        assert!(error.contains("mixed"), "{error}");
    }

    #[test]
    fn same_path_errors() {
        let directory = tempfile::tempdir().unwrap();
        write_fixture(directory.path(), OutputFormat::Json);
        let error = convert_export(
            directory.path(),
            &config(directory.path(), directory.path(), OutputFormat::Csv),
        )
        .unwrap_err()
        .to_string();
        assert!(error.contains("different"), "{error}");
    }

    #[test]
    fn meta_json_does_not_count_as_json_export() {
        let source = tempfile::tempdir().unwrap();
        write_fixture(source.path(), OutputFormat::Csv);
        assert_eq!(
            detect_ir_export(source.path()).unwrap().format,
            OutputFormat::Csv
        );
    }
}
