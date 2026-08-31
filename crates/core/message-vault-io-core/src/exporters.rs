//! Backup-type forms, dropdown labels, and validation used by the desktop app.
//!
//! [`Form`] is the GUI field set. [`Form::to_config`] turns it into a typed
//! [`ExporterConfig`] after checking required paths and options.

use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use media::{MaxResolution, MediaMode};
use message_csv::DateRange;

use crate::config::{
    AppleConfig, ContactsConfig, ExporterConfig, FormatConfig, GoSmsProConfig, ImazingConfig,
    MediaConfig, ObfuscateConfig, OpenExtractConfig, OutputFormat, SmsBackupPlusConfig,
    SmsBackupRestoreConfig, SourceConfig, WhatsappConfig,
};

/// Supported exporters first, then experimental (alphabetical by display name).
pub const EXPORTERS: [Exporter; 7] = [
    Exporter::Imessage,
    Exporter::SmsBackupRestore,
    Exporter::Whatsapp,
    Exporter::GoSmsPro,
    Exporter::Imazing,
    Exporter::OpenExtract,
    Exporter::SmsBackupPlus,
];

/// iMessage Import / CLI copy when Convert or Compress is selected and ffmpeg is missing.
pub const CONVERT_COMPRESS_FFMPEG_REQUIRED: &str = "Convert and Compress need ffmpeg and ffprobe. Put them on PATH, or in the desktop app set the ffmpeg directory in Settings → System.";

/// Which backup type the user selected (iMessage, WhatsApp, SMS Backup & Restore, …).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Exporter {
    /// GO SMS Pro backup type.
    GoSmsPro,
    /// iMazing backup type.
    Imazing,
    #[default]
    /// iMessage / iPhone backup type.
    Imessage,
    /// OpenExtract backup type.
    OpenExtract,
    /// SMS Backup & Restore backup type.
    SmsBackupRestore,
    /// SMS Backup+ backup type.
    SmsBackupPlus,
    /// WhatsApp backup type.
    Whatsapp,
}

impl Exporter {
    /// Standalone CLI binary name for this backup type.
    pub fn binary(self) -> &'static str {
        match self {
            Self::GoSmsPro => "go-sms-pro-exporter",
            Self::SmsBackupRestore => "sms-backup-restore-exporter",
            Self::SmsBackupPlus => "sms-backup-plus-exporter",
            Self::OpenExtract => "openextract-exporter",
            Self::Imazing => "imazing-exporter",
            Self::Imessage => "imessage-ir-exporter",
            Self::Whatsapp => "whatsapp-exporter",
        }
    }

    /// Short product name shown in the backup-type dropdown.
    pub fn display_name(self) -> &'static str {
        match self {
            Self::GoSmsPro => "GO SMS Pro",
            Self::SmsBackupRestore => "SMS Backup & Restore",
            Self::SmsBackupPlus => "SMS Backup+",
            Self::OpenExtract => "OpenExtract",
            Self::Imazing => "iMazing",
            Self::Imessage => "iPhone backup",
            Self::Whatsapp => "WhatsApp",
        }
    }

    /// Officially supported exporters (XML/spec or maintained bridges).
    pub fn is_supported(self) -> bool {
        matches!(
            self,
            Self::Imessage | Self::SmsBackupRestore | Self::Whatsapp
        )
    }

    /// Backup-type dropdown label; experimental exporters get a suffix.
    pub fn dropdown_label(self) -> String {
        if self.is_supported() {
            self.display_name().to_string()
        } else {
            format!("{} (experimental)", self.display_name())
        }
    }

    /// Form title / hyperlink text (may be longer than the dropdown label).
    pub fn link_label(self) -> &'static str {
        match self {
            Self::Imessage => "imessage-ir-exporter",
            other => other.display_name(),
        }
    }

    /// Homepage or docs URL for this backup type.
    pub fn product_url(self) -> &'static str {
        match self {
            Self::GoSmsPro => "https://play.google.com/store/apps/details?id=com.jb.gosms",
            Self::SmsBackupRestore => "https://www.synctech.com.au/sms-backup-restore/",
            Self::SmsBackupPlus => "https://github.com/jberkel/sms-backup-plus",
            Self::OpenExtract => "https://www.openextract.app/",
            Self::Imazing => "https://imazing.com/",
            Self::Imessage => {
                "https://github.com/bitrealm-io/message-vault/tree/main/crates/exporters/imessage-ir-exporter"
            }
            Self::Whatsapp => "https://github.com/KnugiHK/WhatsApp-Chat-Exporter",
        }
    }

    /// Default output folder name under the user's export directory.
    pub fn output_subdir(self) -> &'static str {
        match self {
            Self::GoSmsPro => "go-sms-pro",
            Self::SmsBackupRestore => "sms-backup-restore",
            Self::SmsBackupPlus => "sms-backup-plus",
            Self::OpenExtract => "openextract",
            Self::Imazing => "imazing",
            Self::Imessage => "iphone-backup",
            Self::Whatsapp => "whatsapp",
        }
    }

    /// INI section name / `exporter=` value (same as [`Self::output_subdir`]).
    pub fn ini_key(self) -> &'static str {
        self.output_subdir()
    }

    /// Parse an `export.ini` `exporter=` value, or `None` if unknown.
    pub fn from_ini_key(key: &str) -> Option<Self> {
        match key.trim().to_ascii_lowercase().as_str() {
            "go-sms-pro" => Some(Self::GoSmsPro),
            "sms-backup-restore" => Some(Self::SmsBackupRestore),
            "sms-backup-plus" => Some(Self::SmsBackupPlus),
            "openextract" => Some(Self::OpenExtract),
            "imazing" => Some(Self::Imazing),
            "iphone-backup" => Some(Self::Imessage),
            "whatsapp" => Some(Self::Whatsapp),
            _ => None,
        }
    }
}

/// Android vs iOS for the WhatsApp / wtsexporter bridge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WhatsappPlatform {
    #[default]
    /// Android WhatsApp backup layout.
    Android,
    /// iOS WhatsApp backup layout.
    Ios,
}

impl fmt::Display for WhatsappPlatform {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Android => "Android",
            Self::Ios => "iOS",
        })
    }
}

impl WhatsappPlatform {
    /// CLI flag value (`android` or `ios`).
    pub fn as_cli_str(self) -> &'static str {
        match self {
            Self::Android => "android",
            Self::Ios => "ios",
        }
    }

    /// Value stored in `export.ini` for this platform.
    pub fn as_ini_str(self) -> &'static str {
        self.as_cli_str()
    }

    /// Parse an `export.ini` WhatsApp platform string, or `None` if unknown.
    pub fn from_ini_str(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "android" | "a" | "" => Some(Self::Android),
            "ios" | "iphone" | "ipad" | "i" => Some(Self::Ios),
            _ => None,
        }
    }
}

/// The WhatsApp platforms in GUI dropdown order.
pub const WHATSAPP_PLATFORMS: [WhatsappPlatform; 2] =
    [WhatsappPlatform::Android, WhatsappPlatform::Ios];

/// Create `path` and parents if missing.
///
/// # Errors
///
/// Returns an error string when the directory cannot be created.
pub fn ensure_output_dir(path: &Path) -> Result<(), String> {
    fs::create_dir_all(path).map_err(|error| {
        format!(
            "Could not create output directory {}: {error}",
            path.display()
        )
    })
}

impl fmt::Display for Exporter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.display_name())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
/// How a contacts file is parsed: none, CSV, or vCard (VCF).
pub enum ContactsKind {
    #[default]
    /// No contacts file.
    None,
    /// CSV contacts file.
    Csv,
    /// vCard (VCF) contacts file.
    Vcf,
}

/// Infer contacts kind from a path extension (empty → [`ContactsKind::None`]).
pub fn contacts_kind_from_path(path: &str) -> ContactsKind {
    let path = path.trim();
    if path.is_empty() {
        return ContactsKind::None;
    }
    let lower = path.to_ascii_lowercase();
    if lower.ends_with(".vcf") || lower.ends_with(".vcard") {
        ContactsKind::Vcf
    } else {
        ContactsKind::Csv
    }
}

impl fmt::Display for ContactsKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::None => "No contacts",
            Self::Csv => "Contacts CSV",
            Self::Vcf => "Contacts VCF",
        })
    }
}

/// How attachments are copied or converted when writing output files.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AttachmentMedia {
    #[default]
    /// Copy attachments without re-encoding.
    Clone,
    /// Re-encode attachments to a standard format.
    Convert,
    /// Re-encode and compress videos (720p/1080p/4k).
    Compress,
    /// Do not copy attachments.
    Disabled,
}

impl fmt::Display for AttachmentMedia {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Clone => "Copy",
            Self::Convert => "Convert",
            Self::Compress => "Convert & compress",
            Self::Disabled => "Do not copy",
        })
    }
}

impl AttachmentMedia {
    /// The `media::MediaMode` this GUI choice maps to (the same mode the
    /// `--media-mode` CLI flag selects).
    pub fn media_mode(self) -> MediaMode {
        match self {
            Self::Clone => MediaMode::Clone,
            Self::Convert => MediaMode::Convert,
            Self::Compress => MediaMode::Compress,
            Self::Disabled => MediaMode::Disabled,
        }
    }

    /// True when convert or compress is selected (ffmpeg must be on PATH).
    pub fn needs_ffmpeg(self) -> bool {
        matches!(self, Self::Convert | Self::Compress)
    }

    /// Value stored in `export.ini` for this media choice.
    pub fn as_ini_str(self) -> &'static str {
        self.media_mode().as_str()
    }

    /// Parse an `export.ini` media string, or `None` if unknown.
    pub fn from_ini_str(s: &str) -> Option<Self> {
        MediaMode::parse(s).map(|mode| match mode {
            MediaMode::Clone => Self::Clone,
            MediaMode::Convert => Self::Convert,
            MediaMode::Compress => Self::Compress,
            MediaMode::Disabled => Self::Disabled,
        })
    }
}

/// The attachment-media choices in GUI dropdown order.
pub const ATTACHMENT_MEDIA: [AttachmentMedia; 4] = [
    AttachmentMedia::Clone,
    AttachmentMedia::Convert,
    AttachmentMedia::Compress,
    AttachmentMedia::Disabled,
];

/// The video resolution choices for compress mode.
pub const MAX_RESOLUTIONS: [MaxResolution; 3] = [
    MaxResolution::P720,
    MaxResolution::P1080,
    MaxResolution::P4k,
];

/// iPhone vs Mac backup layout for iMessage / iMazing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ApplePlatform {
    #[default]
    /// Auto-detect the backup layout.
    Auto,
    /// macOS backup layout.
    MacOs,
    /// iOS backup layout.
    Ios,
}

impl fmt::Display for ApplePlatform {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Auto => "Auto-detect",
            Self::MacOs => "macOS",
            Self::Ios => "iOS backup",
        })
    }
}

impl ApplePlatform {
    /// Value stored in `export.ini` for this Apple platform.
    pub fn as_ini_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::MacOs => "macos",
            Self::Ios => "ios",
        }
    }

    /// Parse an `export.ini` Apple platform string, or `None` if unknown.
    pub fn from_ini_str(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "auto" | "" => Some(Self::Auto),
            "macos" | "mac" | "mac-os" => Some(Self::MacOs),
            "ios" | "iphone" => Some(Self::Ios),
            _ => None,
        }
    }
}

/// The Apple platform choices in GUI dropdown order.
pub const APPLE_PLATFORMS: [ApplePlatform; 3] = [
    ApplePlatform::Auto,
    ApplePlatform::MacOs,
    ApplePlatform::Ios,
];

/// GUI field set for one backup type, plus shared output and media options.
#[derive(Debug, Clone)]
pub struct Form {
    /// Primary input path (source backup file or directory).
    pub input: String,
    /// Output directory for the export.
    pub output: String,
    /// Contacts file path (CSV or VCF) for phone→name resolution.
    pub contacts: String,
    /// How the contacts file is parsed.
    pub contacts_kind: ContactsKind,
    /// Comma-separated owner phone numbers (marks outgoing messages).
    pub owner_phones: String,
    /// Comma-separated owner email addresses (marks outgoing messages).
    pub owner_emails: String,
    /// Optional incorrect-name mapping file path.
    pub name_mapping: String,
    /// Optional fixed UTC offset (e.g. `UTC-05:00`) for naive timestamps.
    pub timezone: String,
    /// Whether to rewrite output with stable fake identities.
    pub obfuscate: bool,
    /// Optional hex seed for reproducible obfuscation.
    pub obfuscate_seed: String,
    /// Whether the advanced section of the GUI form is shown.
    pub advanced: bool,
    /// iMessage chat database path (Apple sources).
    pub db_path: String,
    /// Apple backup attachment root directory.
    pub attachment_root: String,
    /// Start-date filter (`YYYY-MM-DD`).
    pub start_date: String,
    /// End-date filter (`YYYY-MM-DD`, exclusive).
    pub end_date: String,
    /// iMessage conversation filter (chat id).
    pub conversation_filter: String,
    /// macOS AddressBook path (Apple sources).
    pub apple_contacts: String,
    /// Apple backup decryption password (never written to `export.ini`).
    pub backup_password: String,
    /// Packaging format projected from the common message (`json` default).
    pub output_format: OutputFormat,
    /// Attachment handling choice for the export.
    pub attachment_media: AttachmentMedia,
    /// Compress-only long-edge cap (720p/1080p/4k).
    pub media_max_resolution: MaxResolution,
    /// Compress-only max frame rate.
    pub media_max_fps: String,
    /// Compress-only minimum video size (e.g. `20M`).
    pub media_min_size: String,
    /// Compress-only: skip already-efficient HEVC videos.
    pub media_skip_efficient: bool,
    /// iPhone vs Mac backup layout.
    pub apple_platform: ApplePlatform,
    /// Android vs iOS WhatsApp layout.
    pub whatsapp_platform: WhatsappPlatform,
    /// WhatsApp backup encryption key (never written to `export.ini`).
    pub whatsapp_key: String,
    /// WhatsApp backup file path.
    pub whatsapp_backup: String,
    /// WhatsApp Web session/wa path.
    pub whatsapp_wa: String,
    /// WhatsApp media folder path.
    pub whatsapp_media: String,
    /// WhatsApp message database path.
    pub whatsapp_db: String,
    /// Whether the backup is a WhatsApp Business backup.
    pub whatsapp_business: bool,
}

impl Default for Form {
    fn default() -> Self {
        Self {
            input: String::new(),
            output: String::new(),
            contacts: String::new(),
            contacts_kind: ContactsKind::default(),
            owner_phones: String::new(),
            owner_emails: String::new(),
            name_mapping: String::new(),
            timezone: String::new(),
            obfuscate: false,
            obfuscate_seed: String::new(),
            advanced: false,
            db_path: String::new(),
            attachment_root: String::new(),
            start_date: String::new(),
            end_date: String::new(),
            conversation_filter: String::new(),
            apple_contacts: String::new(),
            backup_password: String::new(),
            output_format: OutputFormat::default(),
            attachment_media: AttachmentMedia::default(),
            media_max_resolution: MaxResolution::default(),
            media_max_fps: "30".into(),
            media_min_size: "20M".into(),
            media_skip_efficient: true,
            apple_platform: ApplePlatform::default(),
            whatsapp_platform: WhatsappPlatform::default(),
            whatsapp_key: String::new(),
            whatsapp_backup: String::new(),
            whatsapp_wa: String::new(),
            whatsapp_media: String::new(),
            whatsapp_db: String::new(),
            whatsapp_business: false,
        }
    }
}

impl Form {
    /// Validate the form and build a typed [`ExporterConfig`] for `exporter`.
    ///
    /// # Errors
    ///
    /// Returns one string per validation problem (missing path, bad seed, …).
    pub fn to_config(&self, exporter: Exporter) -> Result<ExporterConfig, Vec<String>> {
        let mut errors = Vec::new();
        let obfuscate = self.validate_obfuscate(&mut errors);

        let config = match exporter {
            Exporter::Imessage => self.to_imessage_config(obfuscate, &mut errors),
            Exporter::Whatsapp => self.to_whatsapp_config(obfuscate, &mut errors),
            Exporter::Imazing => self.to_imazing_config(obfuscate, &mut errors),
            Exporter::OpenExtract => self.to_openextract_config(obfuscate, &mut errors),
            Exporter::GoSmsPro => self.to_go_sms_pro_config(obfuscate, &mut errors),
            Exporter::SmsBackupRestore => self.to_sms_restore_config(obfuscate, &mut errors),
            Exporter::SmsBackupPlus => self.to_sms_plus_config(obfuscate, &mut errors),
        };

        if errors.is_empty() {
            Ok(config)
        } else {
            Err(errors)
        }
    }

    /// Validate shared output options and build a Format-tab configuration.
    ///
    /// # Errors
    ///
    /// Returns one string per validation problem (missing folder, bad media, …).
    pub fn to_format_config(
        &self,
        input: &str,
        output: &str,
        output_format: OutputFormat,
    ) -> Result<ExporterConfig, Vec<String>> {
        let mut errors = Vec::new();
        let input = require_existing_directory(input, "Input directory", &mut errors);
        required_text(output, "Output directory", &mut errors);
        let obfuscate = self.validate_obfuscate(&mut errors);
        let media = self.validate_media(&mut errors);
        let config = ExporterConfig {
            inputs: input.into_iter().collect(),
            output: PathBuf::from(output.trim()),
            date_range: DateRange::default(),
            timezone: None,
            contacts: None,
            obfuscate,
            media,
            cancel: None,
            log: None,
            output_format,
            resume: false,
            source: SourceConfig::Format(FormatConfig {}),
        };

        if errors.is_empty() {
            Ok(config)
        } else {
            Err(errors)
        }
    }

    /// Build an iMessage config, pushing path and ffmpeg problems onto `errors`.
    fn to_imessage_config(
        &self,
        obfuscate: ObfuscateConfig,
        errors: &mut Vec<String>,
    ) -> ExporterConfig {
        required_text(&self.output, "Output directory", errors);
        let obfuscate_active = self.obfuscate || !self.obfuscate_seed.trim().is_empty();
        if !obfuscate_active && self.attachment_media.needs_ffmpeg() && !media::ffmpeg_available() {
            errors.push(CONVERT_COMPRESS_FFMPEG_REQUIRED.into());
        }
        let media = self.media_config_for(
            matches!(self.attachment_media, AttachmentMedia::Compress),
            errors,
        );
        let copy_method = match self.attachment_media {
            AttachmentMedia::Disabled => "disabled".into(),
            _ => "clone".into(),
        };
        let platform = match self.apple_platform {
            ApplePlatform::Auto => None,
            other => Some(other),
        };
        let inputs = non_empty(self.db_path.trim())
            .map(|p| vec![PathBuf::from(p)])
            .unwrap_or_default();
        let date_range = parse_date_range_local(
            non_empty(self.start_date.trim()),
            non_empty(self.end_date.trim()),
            errors,
        );
        ExporterConfig {
            inputs,
            output: PathBuf::from(self.output.trim()),
            date_range,
            timezone: None,
            contacts: None,
            obfuscate,
            media,
            cancel: None,
            log: None,
            output_format: self.output_format,
            resume: false,
            source: SourceConfig::Apple(AppleConfig {
                platform,
                attachment_root: non_empty(self.attachment_root.trim()).map(str::to_string),
                copy_method,
                apple_contacts: non_empty_path(&self.apple_contacts),
                backup_password: non_empty(self.backup_password.trim()).map(str::to_string),
                conversation_filter: non_empty(self.conversation_filter.trim()).map(str::to_string),
                use_caller_id: true,
                show_progress: false,
                ignore_disk_space: false,
            }),
        }
    }

    /// Build a WhatsApp config, pushing path and key problems onto `errors`.
    ///
    /// `input` is optional here: when set it becomes the backup search root
    /// (and an allowed media root) for the wtsexporter bridge, which falls
    /// back to its working directory when no input is given.
    fn to_whatsapp_config(
        &self,
        obfuscate: ObfuscateConfig,
        errors: &mut Vec<String>,
    ) -> ExporterConfig {
        let inputs = if self.input.trim().is_empty() {
            Vec::new()
        } else {
            require_single_existing_path(&self.input, "Input", errors)
                .into_iter()
                .collect()
        };
        required_text(&self.output, "Output", errors);
        if self.whatsapp_platform == WhatsappPlatform::Ios && self.whatsapp_backup.trim().is_empty()
        {
            errors.push("Backup path is required for iOS.".into());
        }
        let media = self.validate_media(errors);
        let date_range = parse_date_range_local(
            non_empty(self.start_date.trim()),
            non_empty(self.end_date.trim()),
            errors,
        );
        ExporterConfig {
            inputs,
            output: PathBuf::from(self.output.trim()),
            date_range,
            timezone: None,
            contacts: None,
            obfuscate,
            media,
            cancel: None,
            log: None,
            output_format: self.output_format,
            resume: false,
            source: SourceConfig::Whatsapp(WhatsappConfig {
                platform: Some(self.whatsapp_platform),
                json: None,
                key: non_empty(self.whatsapp_key.trim()).map(str::to_string),
                backup: non_empty_path(&self.whatsapp_backup),
                wa: non_empty_path(&self.whatsapp_wa),
                media: non_empty_path(&self.whatsapp_media),
                db: non_empty_path(&self.whatsapp_db),
                business: self.whatsapp_business,
            }),
        }
    }

    /// Build an iMazing config, pushing path and timezone problems onto `errors`.
    fn to_imazing_config(
        &self,
        obfuscate: ObfuscateConfig,
        errors: &mut Vec<String>,
    ) -> ExporterConfig {
        let input = require_single_existing_path(&self.input, "Input", errors);
        required_text(&self.output, "Output", errors);
        let media = self.validate_media(errors);
        let timezone = non_empty(self.timezone.trim()).map(str::to_string);
        let date_range = match DateRange::parse_optional_tz(
            non_empty(self.start_date.trim()),
            non_empty(self.end_date.trim()),
            timezone.as_deref(),
        ) {
            Ok(range) => range,
            Err(error) => {
                errors.push(error.to_string());
                DateRange::default()
            }
        };
        let contacts = self.contacts_config(errors);
        ExporterConfig {
            inputs: input.into_iter().collect(),
            output: PathBuf::from(self.output.trim()),
            date_range,
            timezone: timezone.clone(),
            contacts,
            obfuscate,
            media,
            cancel: None,
            log: None,
            output_format: self.output_format,
            resume: false,
            source: SourceConfig::Imazing(ImazingConfig {}),
        }
    }

    /// Build an OpenExtract config, pushing path problems onto `errors`.
    fn to_openextract_config(
        &self,
        obfuscate: ObfuscateConfig,
        errors: &mut Vec<String>,
    ) -> ExporterConfig {
        let input = require_single_existing_path(&self.input, "Input", errors);
        required_text(&self.output, "Output", errors);
        let contacts = self.contacts_config(errors);
        let date_range = parse_date_range_local(
            non_empty(self.start_date.trim()),
            non_empty(self.end_date.trim()),
            errors,
        );
        let media = self.validate_media(errors);
        ExporterConfig {
            inputs: input.into_iter().collect(),
            output: PathBuf::from(self.output.trim()),
            date_range,
            timezone: None,
            contacts,
            obfuscate,
            media,
            cancel: None,
            log: None,
            output_format: self.output_format,
            resume: false,
            source: SourceConfig::OpenExtract(OpenExtractConfig {}),
        }
    }

    /// Build a GO SMS Pro config from the shared Android fields.
    fn to_go_sms_pro_config(
        &self,
        obfuscate: ObfuscateConfig,
        errors: &mut Vec<String>,
    ) -> ExporterConfig {
        let (inputs, contacts, date_range, media, owner_phones) = self.android_common(errors);
        ExporterConfig {
            inputs,
            output: PathBuf::from(self.output.trim()),
            date_range,
            timezone: None,
            contacts,
            obfuscate,
            media,
            cancel: None,
            log: None,
            output_format: self.output_format,
            resume: false,
            source: SourceConfig::GoSmsPro(GoSmsProConfig { owner_phones }),
        }
    }

    /// Build an SMS Backup & Restore config from the shared Android fields.
    fn to_sms_restore_config(
        &self,
        obfuscate: ObfuscateConfig,
        errors: &mut Vec<String>,
    ) -> ExporterConfig {
        let (inputs, contacts, date_range, media, owner_phones) = self.android_common(errors);
        ExporterConfig {
            inputs,
            output: PathBuf::from(self.output.trim()),
            date_range,
            timezone: None,
            contacts,
            obfuscate,
            media,
            cancel: None,
            log: None,
            output_format: self.output_format,
            resume: false,
            source: SourceConfig::SmsBackupRestore(SmsBackupRestoreConfig { owner_phones }),
        }
    }

    /// Build an SMS Backup+ config, including owner emails and name mapping.
    fn to_sms_plus_config(
        &self,
        obfuscate: ObfuscateConfig,
        errors: &mut Vec<String>,
    ) -> ExporterConfig {
        let (inputs, contacts, date_range, media, owner_phones) = self.android_common(errors);
        let owner_emails: Vec<String> = values(&self.owner_emails)
            .into_iter()
            .map(str::to_string)
            .collect();
        if owner_emails.is_empty() {
            errors.push("At least one email address is required.".into());
        }
        ExporterConfig {
            inputs,
            output: PathBuf::from(self.output.trim()),
            date_range,
            timezone: None,
            contacts,
            obfuscate,
            media,
            cancel: None,
            log: None,
            output_format: self.output_format,
            resume: false,
            source: SourceConfig::SmsBackupPlus(SmsBackupPlusConfig {
                owner_phones,
                owner_emails,
                name_mapping: non_empty_path(&self.name_mapping),
                verbose: true,
                include_summary: true,
            }),
        }
    }

    /// Shared Android backup fields: input path, owner phones, contacts, dates, media.
    fn android_common(
        &self,
        errors: &mut Vec<String>,
    ) -> (
        Vec<PathBuf>,
        Option<ContactsConfig>,
        DateRange,
        MediaConfig,
        Vec<String>,
    ) {
        let input = require_single_existing_path(&self.input, "Input", errors);
        required_text(&self.output, "Output", errors);
        let mut owner_phones = Vec::new();
        for phone in values(&self.owner_phones) {
            owner_phones.push(phone.to_string());
        }
        if owner_phones.is_empty() {
            errors.push("At least one phone number is required.".into());
        }
        let contacts = self.contacts_config(errors);
        let date_range = parse_date_range_local(
            non_empty(self.start_date.trim()),
            non_empty(self.end_date.trim()),
            errors,
        );
        let media = self.validate_media(errors);
        (
            input.into_iter().collect(),
            contacts,
            date_range,
            media,
            owner_phones,
        )
    }

    /// Contacts file from the form, or `None` when the user chose no contacts.
    fn contacts_config(&self, errors: &mut Vec<String>) -> Option<ContactsConfig> {
        match self.contacts_kind {
            ContactsKind::None => None,
            ContactsKind::Csv => {
                if self.contacts.trim().is_empty() {
                    errors.push("Choose a contacts CSV or select No contacts.".into());
                    None
                } else {
                    Some(ContactsConfig {
                        path: PathBuf::from(self.contacts.trim()),
                        kind: ContactsKind::Csv,
                    })
                }
            }
            ContactsKind::Vcf => {
                if self.contacts.trim().is_empty() {
                    errors.push("Choose a contacts VCF or select No contacts.".into());
                    None
                } else {
                    Some(ContactsConfig {
                        path: PathBuf::from(self.contacts.trim()),
                        kind: ContactsKind::Vcf,
                    })
                }
            }
        }
    }

    /// Media options for Android exporters (always validate compress settings).
    fn validate_media(&self, errors: &mut Vec<String>) -> MediaConfig {
        let mode = self.attachment_media.media_mode();
        let obfuscate_active = self.obfuscate || !self.obfuscate_seed.trim().is_empty();
        // Obfuscate skips copy/convert, so ffmpeg is not required.
        if !obfuscate_active && mode.needs_tools() && !media::ffmpeg_available() {
            errors.push(
                "Convert/Compress require ffmpeg and ffprobe in lib/ (or beside the program), in MESSAGE_VAULT_IO_BIN, or on PATH.".into(),
            );
        }
        self.media_config_for(matches!(mode, MediaMode::Compress), errors)
    }

    /// Fake-name rewrite flag and optional hex seed.
    fn validate_obfuscate(&self, errors: &mut Vec<String>) -> ObfuscateConfig {
        let seed = validate_obfuscate_seed(&self.obfuscate_seed, errors);
        ObfuscateConfig {
            enabled: self.obfuscate || seed.is_some(),
            seed,
        }
    }

    /// Attachment copy/convert/compress options; optionally check compress fields.
    fn media_config_for(&self, validate_compress: bool, errors: &mut Vec<String>) -> MediaConfig {
        let mode = self.attachment_media.media_mode();
        let compress = if validate_compress || matches!(mode, MediaMode::Compress) {
            match self.compress_options() {
                Ok(options) => options,
                Err(error) => {
                    errors.push(error);
                    media::CompressOptions::default()
                }
            }
        } else {
            media::CompressOptions::default()
        };
        MediaConfig { mode, compress }
    }

    /// Compress options for GUI iMessage post-process (after exporter exits).
    ///
    /// # Errors
    ///
    /// Returns an error when fps or min-size cannot be parsed.
    pub fn compress_options(&self) -> Result<media::CompressOptions, String> {
        let fps = self.media_max_fps.trim();
        if fps.is_empty() {
            return Err("Max fps is required for Compress.".into());
        }
        let fps: f32 = fps
            .parse()
            .map_err(|_| "Max fps must be a number.".to_string())?;
        let min_size = self.media_min_size.trim();
        if min_size.is_empty() {
            return Err("Min size is required for Compress.".into());
        }
        media::compress_options_from_cli(
            self.media_max_resolution,
            fps,
            min_size,
            self.media_skip_efficient,
        )
        .map_err(|e| e.to_string())
    }
}

/// Require one existing file or directory; push a message onto `errors` if missing.
fn require_single_existing_path(
    value: &str,
    label: &str,
    errors: &mut Vec<String>,
) -> Option<PathBuf> {
    let paths = lines(value);
    if paths.is_empty() {
        errors.push(format!("{label} is required."));
        return None;
    }
    if paths.len() > 1 {
        errors.push(format!("{label} must be a single file or folder."));
        return None;
    }
    let path = paths[0];
    if !Path::new(path).exists() {
        errors.push(format!("{label} path does not exist: {path}"));
    }
    Some(PathBuf::from(path))
}

/// Push `"{label} is required."` when `value` is blank.
fn required_text(value: &str, label: &str, errors: &mut Vec<String>) {
    if value.trim().is_empty() {
        errors.push(format!("{label} is required."));
    }
}

/// Require an existing directory; push a message onto `errors` if missing.
fn require_existing_directory(
    value: &str,
    label: &str,
    errors: &mut Vec<String>,
) -> Option<PathBuf> {
    let value = value.trim();
    if value.is_empty() {
        errors.push(format!("{label} is required."));
        return None;
    }
    let path = PathBuf::from(value);
    if !path.is_dir() {
        errors.push(format!("{label} does not exist: {value}"));
        return None;
    }
    Some(path)
}

/// Return a trimmed hex seed, or push an error when the string is not hex.
fn validate_obfuscate_seed(seed: &str, errors: &mut Vec<String>) -> Option<String> {
    let seed = seed.trim();
    if seed.is_empty() {
        return None;
    }
    if seed.len() != 8 || !seed.chars().all(|c| c.is_ascii_hexdigit()) {
        errors.push("Obfuscate seed must be exactly 8 hexadecimal characters.".into());
        None
    } else {
        Some(seed.to_string())
    }
}

/// Parse optional start/end dates in host-local time; push parse errors onto `errors`.
fn parse_date_range_local(
    start: Option<&str>,
    end: Option<&str>,
    errors: &mut Vec<String>,
) -> DateRange {
    match DateRange::parse(start, end) {
        Ok(range) => range,
        Err(error) => {
            errors.push(error.to_string());
            DateRange::default()
        }
    }
}

/// Non-empty trimmed lines from a multiline text field.
fn lines(value: &str) -> Vec<&str> {
    value
        .lines()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .collect()
}

/// Non-empty tokens split on commas or whitespace.
fn values(value: &str) -> Vec<&str> {
    value
        .split(['\n', ',', ';'])
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .collect()
}

/// `Some(trimmed)` when the string is not blank.
fn non_empty(value: &str) -> Option<&str> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

/// `Some(path)` when the string is not blank.
fn non_empty_path(value: &str) -> Option<PathBuf> {
    non_empty(value).map(PathBuf::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn imazing_passes_obfuscate() {
        let form = Form {
            input: std::env::current_dir().unwrap().display().to_string(),
            output: "out".into(),
            obfuscate: true,
            ..Form::default()
        };
        let config = form.to_config(Exporter::Imazing).unwrap();
        assert!(config.obfuscate.enabled);
        assert!(matches!(config.source, SourceConfig::Imazing(_)));
    }

    #[test]
    fn seed_must_be_valid_hex() {
        let form = Form {
            input: std::env::current_dir().unwrap().display().to_string(),
            output: "out".into(),
            obfuscate_seed: "bad".into(),
            ..Form::default()
        };
        assert!(form.to_config(Exporter::OpenExtract).is_err());
        let form = Form {
            input: std::env::current_dir().unwrap().display().to_string(),
            output: "out".into(),
            obfuscate_seed: "01234567".into(),
            ..Form::default()
        };
        let config = form.to_config(Exporter::OpenExtract).unwrap();
        assert!(config.obfuscate.enabled);
        assert_eq!(config.obfuscate.seed.as_deref(), Some("01234567"));
    }

    #[test]
    fn plus_verbose_and_owner_fields() {
        let cwd = std::env::current_dir().unwrap().display().to_string();
        let form = Form {
            input: cwd,
            output: "out".into(),
            owner_phones: "+15555550100\n+15555550101".into(),
            owner_emails: "me@example.com".into(),
            ..Form::default()
        };
        let config = form.to_config(Exporter::SmsBackupPlus).unwrap();
        assert_eq!(config.inputs.len(), 1);
        let SourceConfig::SmsBackupPlus(plus) = config.source else {
            panic!("expected SmsBackupPlus");
        };
        assert_eq!(plus.owner_phones.len(), 2);
        assert!(plus.verbose);
        assert!(plus.include_summary);
    }

    #[test]
    fn plus_rejects_multiple_inputs() {
        let cwd = std::env::current_dir().unwrap().display().to_string();
        let form = Form {
            input: format!("{cwd}\n{cwd}"),
            output: "out".into(),
            owner_phones: "+15555550100".into(),
            owner_emails: "me@example.com".into(),
            ..Form::default()
        };
        let err = form.to_config(Exporter::SmsBackupPlus).unwrap_err();
        assert!(err.iter().any(|e| e.contains("single file or folder")));
    }

    #[test]
    fn exporters_order_supported_then_experimental_alpha() {
        assert_eq!(
            &EXPORTERS[..3],
            &[
                Exporter::Imessage,
                Exporter::SmsBackupRestore,
                Exporter::Whatsapp,
            ]
        );
        assert!(EXPORTERS[..3].iter().all(|e| e.is_supported()));
        assert!(EXPORTERS[3..].iter().all(|e| !e.is_supported()));

        let experimental: Vec<_> = EXPORTERS[3..].iter().map(|e| e.display_name()).collect();
        let mut sorted = experimental.clone();
        sorted.sort_by_key(|a| a.to_lowercase());
        assert_eq!(experimental, sorted);

        assert_eq!(Exporter::Imessage.dropdown_label(), "iPhone backup");
        assert_eq!(
            Exporter::GoSmsPro.dropdown_label(),
            "GO SMS Pro (experimental)"
        );
    }

    #[test]
    fn imessage_requires_output_and_uses_caller_id() {
        let form = Form {
            output: String::new(),
            ..Form::default()
        };
        assert!(form.to_config(Exporter::Imessage).is_err());

        let form = Form {
            output: "out".into(),
            ..Form::default()
        };
        let config = form.to_config(Exporter::Imessage).unwrap();
        assert_eq!(config.output, PathBuf::from("out"));
        let SourceConfig::Apple(apple) = config.source else {
            panic!("expected Apple");
        };
        assert!(apple.use_caller_id);
        assert_eq!(apple.copy_method, "clone");
        assert!(!apple.ignore_disk_space);
        assert!(!apple.show_progress);
    }

    #[test]
    fn sbr_passes_output_format() {
        let form = Form {
            input: std::env::current_dir().unwrap().display().to_string(),
            output: "out".into(),
            owner_phones: "+15555550100".into(),
            output_format: OutputFormat::Eml,
            ..Form::default()
        };
        let config = form.to_config(Exporter::SmsBackupRestore).unwrap();
        assert_eq!(config.output_format, OutputFormat::Eml);

        let go = form.to_config(Exporter::GoSmsPro).unwrap();
        assert_eq!(go.output_format, OutputFormat::Eml);
    }

    #[test]
    fn imessage_passes_output_format() {
        let form = Form {
            output: "out".into(),
            output_format: OutputFormat::Eml,
            ..Form::default()
        };
        let config = form.to_config(Exporter::Imessage).unwrap();
        assert_eq!(config.output_format, OutputFormat::Eml);
    }

    #[test]
    fn android_passes_media_mode() {
        let form = Form {
            input: std::env::current_dir().unwrap().display().to_string(),
            output: "out".into(),
            owner_phones: "+15555550100".into(),
            attachment_media: AttachmentMedia::Clone,
            ..Form::default()
        };
        let config = form.to_config(Exporter::GoSmsPro).unwrap();
        assert_eq!(config.media.mode, MediaMode::Clone);

        let form = Form {
            input: std::env::current_dir().unwrap().display().to_string(),
            output: "out".into(),
            owner_phones: "+15555550100".into(),
            attachment_media: AttachmentMedia::Disabled,
            ..Form::default()
        };
        let config = form.to_config(Exporter::GoSmsPro).unwrap();
        assert_eq!(config.media.mode, MediaMode::Disabled);
    }

    #[test]
    fn openextract_passes_date_range() {
        let form = Form {
            input: std::env::current_dir().unwrap().display().to_string(),
            output: "out".into(),
            start_date: "2020-01-01".into(),
            end_date: "2020-02-01".into(),
            ..Form::default()
        };
        let config = form.to_config(Exporter::OpenExtract).unwrap();
        assert!(!config.date_range.is_unbounded());
        assert!(matches!(config.source, SourceConfig::OpenExtract(_)));
    }

    #[test]
    fn imazing_passes_timezone_to_config() {
        let form = Form {
            input: std::env::current_dir().unwrap().display().to_string(),
            output: "out".into(),
            timezone: "UTC-05:00".into(),
            start_date: "2020-01-01".into(),
            ..Form::default()
        };
        let config = form.to_config(Exporter::Imazing).unwrap();
        let SourceConfig::Imazing(_) = &config.source else {
            panic!("expected Imazing");
        };
        assert_eq!(config.timezone.as_deref(), Some("UTC-05:00"));
    }

    #[test]
    fn imazing_honors_vcf_contacts_kind() {
        let form = Form {
            input: std::env::current_dir().unwrap().display().to_string(),
            output: "out".into(),
            contacts: "/tmp/contacts.vcf".into(),
            contacts_kind: ContactsKind::Vcf,
            ..Form::default()
        };
        let config = form.to_config(Exporter::Imazing).unwrap();
        let contacts = config.contacts.as_ref().expect("contacts");
        assert_eq!(contacts.kind, ContactsKind::Vcf);
        assert_eq!(contacts.path, PathBuf::from("/tmp/contacts.vcf"));
        let (csv, vcf) = config.contacts_csv_vcf();
        assert!(csv.is_none());
        assert_eq!(vcf, Some(PathBuf::from("/tmp/contacts.vcf")));
    }

    #[test]
    fn whatsapp_passes_platform_and_media() {
        let form = Form {
            output: "out".into(),
            whatsapp_platform: WhatsappPlatform::Android,
            whatsapp_key: "abc123".into(),
            whatsapp_backup: "/tmp/backup".into(),
            whatsapp_media: "/tmp/media".into(),
            whatsapp_business: true,
            attachment_media: AttachmentMedia::Clone,
            ..Form::default()
        };
        let config = form.to_config(Exporter::Whatsapp).unwrap();
        assert!(config.inputs.is_empty());
        assert_eq!(config.media.mode, MediaMode::Clone);
        let SourceConfig::Whatsapp(wa) = config.source else {
            panic!("expected Whatsapp");
        };
        assert_eq!(wa.platform, Some(WhatsappPlatform::Android));
        assert_eq!(wa.key.as_deref(), Some("abc123"));
        assert_eq!(wa.backup, Some(PathBuf::from("/tmp/backup")));
        assert_eq!(wa.media, Some(PathBuf::from("/tmp/media")));
        assert!(wa.business);

        let ios = Form {
            output: "out".into(),
            whatsapp_platform: WhatsappPlatform::Ios,
            whatsapp_backup: "/tmp/ios-backup".into(),
            ..Form::default()
        };
        let ios_config = ios.to_config(Exporter::Whatsapp).unwrap();
        let SourceConfig::Whatsapp(wa) = ios_config.source else {
            panic!("expected Whatsapp");
        };
        assert_eq!(wa.platform, Some(WhatsappPlatform::Ios));
        assert_eq!(wa.backup, Some(PathBuf::from("/tmp/ios-backup")));
        assert!(wa.key.is_none());

        let ios_missing = Form {
            output: "out".into(),
            whatsapp_platform: WhatsappPlatform::Ios,
            ..Form::default()
        };
        let err = ios_missing.to_config(Exporter::Whatsapp).unwrap_err();
        assert!(
            err.iter()
                .any(|e| e.contains("Backup path is required for iOS"))
        );
    }

    #[test]
    fn whatsapp_forwards_existing_input_as_search_root() {
        let dir = tempfile::tempdir().unwrap();
        let form = Form {
            input: dir.path().display().to_string(),
            output: "out".into(),
            ..Form::default()
        };
        let config = form.to_config(Exporter::Whatsapp).unwrap();
        assert_eq!(config.inputs, vec![dir.path().to_path_buf()]);

        let missing = Form {
            input: "/does/not/exist-whatsapp-input".into(),
            output: "out".into(),
            ..Form::default()
        };
        let err = missing.to_config(Exporter::Whatsapp).unwrap_err();
        assert!(err.iter().any(|e| e.contains("does not exist")), "{err:?}");
    }

    #[test]
    fn ensure_output_dir_creates_missing_path() {
        let out = std::env::temp_dir().join(format!(
            "message-vault-io-core-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        ensure_output_dir(&out).unwrap();
        assert!(out.is_dir());
        let _ = fs::remove_dir_all(&out);
    }

    #[test]
    fn reexport_config_uses_shared_output_validation() {
        let input = tempfile::tempdir().unwrap();
        let form = Form {
            obfuscate_seed: "01234567".into(),
            attachment_media: AttachmentMedia::Disabled,
            ..Form::default()
        };

        let config = form
            .to_format_config(input.path().to_str().unwrap(), "out", OutputFormat::Json)
            .unwrap();

        assert_eq!(config.inputs, vec![input.path().to_path_buf()]);
        assert!(config.obfuscate.enabled);
        assert_eq!(config.obfuscate.seed.as_deref(), Some("01234567"));
        assert!(matches!(config.source, SourceConfig::Format(_)));
    }

    #[test]
    fn format_config_reports_paths_and_shared_options_together() {
        let form = Form {
            obfuscate_seed: "invalid".into(),
            attachment_media: AttachmentMedia::Disabled,
            ..Form::default()
        };

        let errors = form
            .to_format_config("", "", OutputFormat::Json)
            .unwrap_err();

        assert!(errors.iter().any(|error| error.contains("Input directory")));
        assert!(
            errors
                .iter()
                .any(|error| error.contains("Output directory"))
        );
        assert!(errors.iter().any(|error| error.contains("Obfuscate seed")));
    }

    struct RestoreToolsDir;

    impl Drop for RestoreToolsDir {
        fn drop(&mut self) {
            media::set_tools_dir(None);
        }
    }

    #[test]
    fn imessage_convert_without_ffmpeg_uses_locked_copy() {
        let dir = tempfile::tempdir().unwrap();
        let _restore = RestoreToolsDir;
        media::set_tools_dir(Some(dir.path().to_path_buf()));
        let form = Form {
            output: "out".into(),
            attachment_media: AttachmentMedia::Convert,
            ..Form::default()
        };
        let err = form.to_config(Exporter::Imessage).unwrap_err();
        assert!(
            err.iter().any(|e| e == CONVERT_COMPRESS_FFMPEG_REQUIRED),
            "{err:?}"
        );
    }
}
