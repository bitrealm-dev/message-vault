//! Shared exporter forms, typed config, and process helpers for desktop GUIs.

#[cfg(feature = "cli")]
mod cli;
mod config;
mod export_ini;
mod exporters;
mod pipeline;
mod process;

#[cfg(feature = "cli")]
pub use cli::CommonCli;
pub use config::{
    AppleConfig, ContactsConfig, ExporterConfig, GoSmsProConfig, ImazingConfig, MediaConfig,
    FormatConfig, OUTPUT_FORMATS_MAIL, ObfuscateConfig, OpenExtractConfig, OutputFormat,
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
    print_result,
};
pub use process::{
    CancelFlag, JobError, LogSink, ProcessControl, ProcessEvent, check_cancel, emit_log,
    is_cancelled, spawn_job,
};
