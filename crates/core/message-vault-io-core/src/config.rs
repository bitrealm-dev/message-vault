//! Shared typed export configuration for CLI, library, and GUI.
//!
//! [`ExporterConfig`] holds options common to (nearly) every exporter.
//! Exporter-specific knobs live in [`SourceConfig`].

use std::fmt;
use std::path::{Path, PathBuf};

use media::{CompressOptions, MediaMode};
use message_csv::DateRange;

use crate::exporters::{ApplePlatform, ContactsKind, Exporter, WhatsappPlatform};
use crate::process::{CancelFlag, LogSink, emit_log};

/// Output packaging projected from the common message.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum OutputFormat {
    /// Per-conversation CSV.
    Csv,
    /// Per-conversation folder of `.eml` files (see https://vault.bitrealm.dev/developer/formats/mail-archive/).
    Eml,
    /// Per-conversation `.mbox` (mboxrd) mailbox file.
    Mbox,
    /// Per-conversation common message JSON (default; see docs/maintainers/architecture/message-ir.md).
    #[default]
    Json,
    /// Per-conversation common message as JSON Lines (header + one message per line).
    Jsonl,
    /// Single SMS Backup & Restore XML backup (`smses.xml`).
    Xml,
}

impl fmt::Display for OutputFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Csv => "CSV (per conversation)",
            Self::Eml => "EML archive (mail folders)",
            Self::Mbox => "MBOX (per conversation)",
            Self::Json => "JSON (common message)",
            Self::Jsonl => "JSONL (common message lines)",
            Self::Xml => "XML (SMS Backup & Restore)",
        })
    }
}

impl OutputFormat {
    /// Short format id used on the CLI (`json`, `jsonl`, `csv`, …).
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Csv => "csv",
            Self::Eml => "eml",
            Self::Mbox => "mbox",
            Self::Json => "json",
            Self::Jsonl => "jsonl",
            Self::Xml => "xml",
        }
    }

    /// Parse a format id. `ndjson` is accepted as JSON Lines; `sbr`/`smses` as XML.
    ///
    /// # Errors
    ///
    /// Returns an error string when `s` is not a known format.
    pub fn parse(s: &str) -> Result<Self, String> {
        match s.trim().to_ascii_lowercase().as_str() {
            "csv" => Ok(Self::Csv),
            "eml" => Ok(Self::Eml),
            "mbox" => Ok(Self::Mbox),
            "json" => Ok(Self::Json),
            "jsonl" | "ndjson" => Ok(Self::Jsonl),
            "xml" | "sbr" | "smses" => Ok(Self::Xml),
            other => Err(format!(
                "unknown output format '{other}' (expected csv, eml, mbox, json, jsonl, or xml)"
            )),
        }
    }

    /// True for mail-archive packaging (EML folders or MBOX files).
    pub fn is_mail_archive(self) -> bool {
        matches!(self, Self::Eml | Self::Mbox)
    }

    /// True when export writes a single SyncTech `smses.xml` (use [`message_ir_format::FormatSink`]).
    pub fn is_sbr_xml(self) -> bool {
        matches!(self, Self::Xml)
    }
}

/// Values shown in the GUI for full packaging choices (JSON first = default).
pub const OUTPUT_FORMATS_MAIL: [OutputFormat; 6] = [
    OutputFormat::Json,
    OutputFormat::Jsonl,
    OutputFormat::Csv,
    OutputFormat::Eml,
    OutputFormat::Mbox,
    OutputFormat::Xml,
];

/// Shared export inputs. Source-specific fields are in [`Self::source`].
#[derive(Debug, Clone)]
pub struct ExporterConfig {
    /// Input paths (usually one). SMS Backup+ CLI may pass several; WhatsApp may leave empty.
    pub inputs: Vec<PathBuf>,
    pub output: PathBuf,
    pub date_range: DateRange,
    /// Optional fixed UTC offset for naive timestamps, e.g. `UTC-05:00`.
    /// When `None`, dates are interpreted in host-local time.
    pub timezone: Option<String>,
    pub contacts: Option<ContactsConfig>,
    pub obfuscate: ObfuscateConfig,
    /// Attachment handling for FormatSink (none / copy / convert / compress).
    pub media: MediaConfig,
    pub cancel: Option<CancelFlag>,
    /// Mid-run progress / warnings. `None` → stderr (CLI); GUI sets a sink.
    pub log: Option<LogSink>,
    /// Packaging format (`csv` / `eml` / `mbox` / `json` / `jsonl` / `xml`).
    pub output_format: OutputFormat,
    pub source: SourceConfig,
}

impl ExporterConfig {
    /// Send a progress or warning line to the log sink, or to stderr if none is set.
    pub fn emit_log(&self, line: impl AsRef<str>) {
        emit_log(self.log.as_ref(), line);
    }

    /// First input path, if any.
    pub fn primary_input(&self) -> Option<&Path> {
        self.inputs.first().map(PathBuf::as_path)
    }

    /// Require a single primary input (most exporters).
    ///
    /// # Errors
    ///
    /// Returns an error when no input is set, or when more than one path is set.
    pub fn require_input(&self) -> Result<&Path, String> {
        match self.inputs.as_slice() {
            [path] => Ok(path.as_path()),
            [] => Err("input is required".into()),
            _ => Err("expected a single input path".into()),
        }
    }

    /// Split contacts into `(--contacts, --vcf)` paths for loaders that take both.
    pub fn contacts_csv_vcf(&self) -> (Option<PathBuf>, Option<PathBuf>) {
        match &self.contacts {
            Some(c) => c.csv_and_vcf(),
            None => (None, None),
        }
    }

    /// True when fake-name rewrite is on, or a seed was supplied.
    pub fn obfuscate_active(&self) -> bool {
        self.obfuscate.enabled || self.obfuscate.seed.is_some()
    }
}

/// Path and kind of an optional contacts file used to resolve phone numbers to names.
#[derive(Debug, Clone)]
pub struct ContactsConfig {
    pub path: PathBuf,
    pub kind: ContactsKind,
}

impl ContactsConfig {
    /// Split this contacts file into `(csv_path, vcf_path)` for loaders that take both.
    pub fn csv_and_vcf(&self) -> (Option<PathBuf>, Option<PathBuf>) {
        match self.kind {
            ContactsKind::Csv => (Some(self.path.clone()), None),
            ContactsKind::Vcf => (None, Some(self.path.clone())),
            ContactsKind::None => (None, None),
        }
    }
}

#[derive(Debug, Clone, Default)]
/// Fake-name rewrite: on/off plus an optional hex seed for repeatable output.
pub struct ObfuscateConfig {
    pub enabled: bool,
    pub seed: Option<String>,
}

#[derive(Debug, Clone)]
/// How attachments are copied, converted, or compressed when writing output.
pub struct MediaConfig {
    pub mode: MediaMode,
    pub compress: CompressOptions,
}

impl Default for MediaConfig {
    fn default() -> Self {
        Self {
            mode: MediaMode::Clone,
            compress: CompressOptions::default(),
        }
    }
}

/// Exporter-specific options. Exactly one variant is set per run.
#[derive(Debug, Clone)]
pub enum SourceConfig {
    GoSmsPro(GoSmsProConfig),
    SmsBackupRestore(SmsBackupRestoreConfig),
    SmsBackupPlus(SmsBackupPlusConfig),
    OpenExtract(OpenExtractConfig),
    Imazing(ImazingConfig),
    Apple(AppleConfig),
    Whatsapp(WhatsappConfig),
    /// Existing Message Vault output → another IR format (`message-reexporter`).
    /// Not listed in [`crate::exporters::EXPORTERS`] (own GUI Format tab).
    Format(FormatConfig),
}

impl SourceConfig {
    /// Backup type for this source, or `None` for the Format-tab converter.
    pub fn exporter(&self) -> Option<Exporter> {
        match self {
            Self::GoSmsPro(_) => Some(Exporter::GoSmsPro),
            Self::SmsBackupRestore(_) => Some(Exporter::SmsBackupRestore),
            Self::SmsBackupPlus(_) => Some(Exporter::SmsBackupPlus),
            Self::OpenExtract(_) => Some(Exporter::OpenExtract),
            Self::Imazing(_) => Some(Exporter::Imazing),
            Self::Apple(_) => Some(Exporter::Imessage),
            Self::Whatsapp(_) => Some(Exporter::Whatsapp),
            Self::Format(_) => None,
        }
    }
}

#[derive(Debug, Clone, Default)]
/// Empty marker: convert an existing export folder to another output format.
pub struct FormatConfig {}

#[derive(Debug, Clone)]
/// GO SMS Pro extras: owner phone numbers used to mark outgoing messages.
pub struct GoSmsProConfig {
    pub owner_phones: Vec<String>,
}

#[derive(Debug, Clone)]
/// SMS Backup & Restore extras: owner phone numbers used to mark outgoing messages.
pub struct SmsBackupRestoreConfig {
    pub owner_phones: Vec<String>,
}

#[derive(Debug, Clone)]
/// SMS Backup+ extras: owner phones/emails, optional name-mapping file, log flags.
pub struct SmsBackupPlusConfig {
    pub owner_phones: Vec<String>,
    pub owner_emails: Vec<String>,
    pub name_mapping: Option<PathBuf>,
    pub verbose: bool,
    pub include_summary: bool,
}

#[derive(Debug, Clone, Default)]
/// OpenExtract has no extra fields beyond the shared [`ExporterConfig`].
pub struct OpenExtractConfig {}

#[derive(Debug, Clone, Default)]
/// iMazing has no extra fields beyond the shared [`ExporterConfig`] (timezone lives there).
pub struct ImazingConfig {}

#[derive(Debug, Clone)]
/// iMessage / iPhone backup extras: platform, copy method, contacts, password.
pub struct AppleConfig {
    pub platform: Option<ApplePlatform>,
    pub attachment_root: Option<String>,
    /// `disabled`, `clone`, `basic`, or `full`.
    pub copy_method: String,
    pub apple_contacts: Option<PathBuf>,
    pub backup_password: Option<String>,
    pub conversation_filter: Option<String>,
    pub use_caller_id: bool,
    pub show_progress: bool,
    pub ignore_disk_space: bool,
}

impl Default for AppleConfig {
    fn default() -> Self {
        Self {
            platform: None,
            attachment_root: None,
            copy_method: "clone".into(),
            apple_contacts: None,
            backup_password: None,
            conversation_filter: None,
            use_caller_id: true,
            show_progress: false,
            ignore_disk_space: false,
        }
    }
}

#[derive(Debug, Clone, Default)]
/// WhatsApp extras: Android vs iOS, key, backup folder, and optional media/db paths.
pub struct WhatsappConfig {
    pub platform: Option<WhatsappPlatform>,
    pub json: Option<PathBuf>,
    pub key: Option<String>,
    pub backup: Option<PathBuf>,
    pub wa: Option<PathBuf>,
    pub media: Option<PathBuf>,
    pub db: Option<PathBuf>,
    pub business: bool,
}
