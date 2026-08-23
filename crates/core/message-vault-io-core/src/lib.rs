//! Shared exporter forms, typed config, and background-job helpers for desktop apps.
//!
//! The desktop app and exporter command-line tools share this crate so every
//! backup type validates the same way before a job starts.

#![warn(missing_docs)]

pub mod attachments;
#[cfg(feature = "cli")]
mod cli;
mod config;
mod export_ini;
mod exporters;
mod pipeline;
mod process;

pub use attachments::{attachment_dest_name, copy_if_missing, digest_prefix, write_if_missing};
#[cfg(feature = "cli")]
pub use cli::{CommonCli, clap_command};
pub use config::{
    AppleConfig, ContactsConfig, ExporterConfig, FormatConfig, GoSmsProConfig, ImazingConfig,
    MediaConfig, OUTPUT_FORMATS_MAIL, ObfuscateConfig, OpenExtractConfig, OutputFormat,
    SmsBackupPlusConfig, SmsBackupRestoreConfig, SourceConfig, WhatsappConfig,
};
pub use export_ini::{AppearanceSection, ExportIniState, FormatSection, VaultSection};
pub use exporters::{
    APPLE_PLATFORMS, ATTACHMENT_MEDIA, ApplePlatform, AttachmentMedia, ContactsKind, EXPORTERS,
    Exporter, Form, MAX_RESOLUTIONS, WHATSAPP_PLATFORMS, WhatsappPlatform, contacts_kind_from_path,
    ensure_output_dir,
};
pub use pipeline::{
    ExportReport, RunResult, discover_files, name_stem, parse_date_range, parse_date_range_tz,
    prepare_outputs, print_result,
};
pub use process::{
    CancelFlag, JobError, LogSink, ProcessControl, ProcessEvent, check_cancel, emit_log,
    is_cancelled, spawn_job,
};
