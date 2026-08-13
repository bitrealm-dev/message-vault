//! Combo-box option lists shared with the Slint adapters.
//!
//! Each function returns labels in combo-box order, or maps an enum to that
//! index (and back). Slint combo boxes are driven by integer indexes.

use media::MaxResolution;
use message_vault_io_core::{
    APPLE_PLATFORMS, ATTACHMENT_MEDIA, ApplePlatform, AttachmentMedia, EXPORTERS, Exporter,
    MAX_RESOLUTIONS, OUTPUT_FORMATS_MAIL, OutputFormat, WHATSAPP_PLATFORMS, WhatsappPlatform,
};
use slint::{ModelRc, SharedString, VecModel};

const OUTPUT_FORMATS_ALPHABETICAL: [OutputFormat; OUTPUT_FORMATS_MAIL.len()] = [
    OutputFormat::Csv,
    OutputFormat::Eml,
    OutputFormat::Json,
    OutputFormat::Jsonl,
    OutputFormat::Mbox,
    OutputFormat::Xml,
];

pub const UTC_OFFSETS: &[&str] = &[
    "UTC-12:00",
    "UTC-11:00",
    "UTC-10:00",
    "UTC-09:30",
    "UTC-09:00",
    "UTC-08:00",
    "UTC-07:00",
    "UTC-06:00",
    "UTC-05:00",
    "UTC-04:00",
    "UTC-03:30",
    "UTC-03:00",
    "UTC-02:00",
    "UTC-01:00",
    "UTC+00:00",
    "UTC+01:00",
    "UTC+02:00",
    "UTC+03:00",
    "UTC+03:30",
    "UTC+04:00",
    "UTC+04:30",
    "UTC+05:00",
    "UTC+05:30",
    "UTC+05:45",
    "UTC+06:00",
    "UTC+06:30",
    "UTC+07:00",
    "UTC+08:00",
    "UTC+08:45",
    "UTC+09:00",
    "UTC+09:30",
    "UTC+10:00",
    "UTC+10:30",
    "UTC+11:00",
    "UTC+12:00",
    "UTC+12:45",
    "UTC+13:00",
    "UTC+14:00",
];

/// Build a Slint string model from owned label strings.
fn model_from_labels(labels: impl IntoIterator<Item = String>) -> ModelRc<SharedString> {
    let items: Vec<SharedString> = labels.into_iter().map(SharedString::from).collect();
    ModelRc::new(VecModel::from(items))
}

/// Index of `value` in `items`, or `0` if it is missing.
fn index_of<T: PartialEq>(items: &[T], value: &T) -> i32 {
    items.iter().position(|item| item == value).unwrap_or(0) as i32
}

/// Item at `index`, or `T::default()` if the index is out of range.
fn value_at<T: Copy + Default>(items: &[T], index: i32) -> T {
    items.get(index as usize).copied().unwrap_or_default()
}

/// Exporter names for the Extract Messages combo box, in `EXPORTERS` order.
pub fn exporter_options() -> ModelRc<SharedString> {
    model_from_labels(EXPORTERS.iter().map(|exporter| exporter.dropdown_label()))
}

/// Combo index where unsupported (experimental) exporters begin, or `-1` if none.
pub fn exporter_separator_before_index() -> i32 {
    match EXPORTERS
        .iter()
        .position(|exporter| !exporter.is_supported())
    {
        Some(index) => index as i32,
        None => -1,
    }
}

/// Output format labels as file extensions (`.jsonl`, `.csv`, …), A–Z.
pub fn output_format_options() -> ModelRc<SharedString> {
    model_from_labels(
        OUTPUT_FORMATS_ALPHABETICAL
            .iter()
            .map(|format| format!(".{}", format.as_str())),
    )
}

/// Attachment handling labels: Copy, Convert, Compress & Convert, Skip.
pub fn attachment_media_options() -> ModelRc<SharedString> {
    model_from_labels(
        ["Copy", "Convert", "Compress & Convert", "Skip"]
            .into_iter()
            .map(str::to_string),
    )
}

/// Max video/image resolution labels from `MAX_RESOLUTIONS`.
pub fn max_resolution_options() -> ModelRc<SharedString> {
    model_from_labels(MAX_RESOLUTIONS.iter().map(|res| res.as_str().to_string()))
}

/// Apple platform labels (`ios` / `macos`) as stored in `export.ini`.
pub fn apple_platform_options() -> ModelRc<SharedString> {
    model_from_labels(
        APPLE_PLATFORMS
            .iter()
            .map(|platform| platform.as_ini_str().to_string()),
    )
}

/// WhatsApp platform labels as stored in `export.ini`.
pub fn whatsapp_platform_options() -> ModelRc<SharedString> {
    model_from_labels(
        WHATSAPP_PLATFORMS
            .iter()
            .map(|platform| platform.as_ini_str().to_string()),
    )
}

/// Timezone combo: "Local time" at index 0, then the [`UTC_OFFSETS`] list.
pub fn timezone_options() -> ModelRc<SharedString> {
    let mut labels = vec!["Local time".to_string()];
    labels.extend(UTC_OFFSETS.iter().map(|offset| (*offset).to_string()));
    model_from_labels(labels)
}

/// Phone-number region labels for the contacts validator.
pub fn region_options() -> ModelRc<SharedString> {
    model_from_labels(["USA".into(), "International".into()])
}

/// Vault Operation choices on the guided credentials screen.
/// Index 0 = Import, 1 = Export.
pub fn vault_operation_options() -> ModelRc<SharedString> {
    model_from_labels(["Import".into(), "Export".into()])
}

/// Exporter types on the guided Vault Export screen (iMessage covers iOS and macOS).
pub fn vault_export_exporter_options() -> ModelRc<SharedString> {
    model_from_labels(["iMessage".into()])
}

/// Directory-name slug for the guided Vault Export exporter combo index.
pub fn vault_export_type_slug(_index: i32) -> &'static str {
    crate::staging::IMESSAGE_EXPORTER
}

/// Guided Import Messages format (combo index order).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GuidedImportFormat {
    #[default]
    Ios,
    MacOs,
    ExistingArchive,
}

impl GuidedImportFormat {
    /// Value stored in `export.ini` under `import_format`.
    pub fn as_ini_str(self) -> &'static str {
        match self {
            Self::Ios => "ios",
            Self::MacOs => "macos",
            Self::ExistingArchive => "existing-archive",
        }
    }

    /// Parse an `import_format` string from `export.ini`.
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim() {
            "ios" => Some(Self::Ios),
            "macos" => Some(Self::MacOs),
            "existing-archive" => Some(Self::ExistingArchive),
            _ => None,
        }
    }

    /// Default format from the Apple platform already stored on the form.
    pub fn from_platform(platform: ApplePlatform) -> Self {
        match platform {
            ApplePlatform::MacOs => Self::MacOs,
            _ => Self::Ios,
        }
    }

    /// Matching Apple platform, or `None` for an existing archive (no extract step).
    pub fn apple_platform(self) -> Option<ApplePlatform> {
        match self {
            Self::Ios => Some(ApplePlatform::Ios),
            Self::MacOs => Some(ApplePlatform::MacOs),
            Self::ExistingArchive => None,
        }
    }
}

/// Format choices for the guided Import Messages screen.
pub fn guided_import_format_options() -> ModelRc<SharedString> {
    model_from_labels([
        "iMessage - iOS".into(),
        "iMessage - macOS".into(),
        "Existing Archive (.jsonl)".into(),
    ])
}

/// Guided import format index: 0 = iOS, 1 = macOS, 2 = existing archive.
pub fn guided_import_format_index(format: GuidedImportFormat) -> i32 {
    match format {
        GuidedImportFormat::Ios => 0,
        GuidedImportFormat::MacOs => 1,
        GuidedImportFormat::ExistingArchive => 2,
    }
}

/// Guided import format for a combo index. Unknown indexes map to iOS.
pub fn guided_import_format_at(index: i32) -> GuidedImportFormat {
    match index {
        1 => GuidedImportFormat::MacOs,
        2 => GuidedImportFormat::ExistingArchive,
        _ => GuidedImportFormat::Ios,
    }
}

/// Combo index of `exporter` in `EXPORTERS`, or `0` if missing.
pub fn exporter_index(exporter: Exporter) -> i32 {
    index_of(&EXPORTERS, &exporter)
}

/// Exporter at `index`, or the default exporter if the index is out of range.
pub fn exporter_at(index: i32) -> Exporter {
    value_at(&EXPORTERS, index)
}

/// Combo index of `format` in the A–Z output format list, or `0` if missing.
pub fn output_format_index(format: OutputFormat) -> i32 {
    index_of(&OUTPUT_FORMATS_ALPHABETICAL, &format)
}

/// Output format at `index`, or the default format if the index is out of range.
pub fn output_format_at(index: i32) -> OutputFormat {
    value_at(&OUTPUT_FORMATS_ALPHABETICAL, index)
}

/// Combo index of `media` in `ATTACHMENT_MEDIA`, or `0` if missing.
pub fn attachment_media_index(media: AttachmentMedia) -> i32 {
    index_of(&ATTACHMENT_MEDIA, &media)
}

/// Attachment handling at `index`, or the default if the index is out of range.
pub fn attachment_media_at(index: i32) -> AttachmentMedia {
    value_at(&ATTACHMENT_MEDIA, index)
}

/// Combo index of `res` in `MAX_RESOLUTIONS`, or `0` if missing.
pub fn max_resolution_index(res: MaxResolution) -> i32 {
    index_of(&MAX_RESOLUTIONS, &res)
}

/// Max resolution at `index`, or the default if the index is out of range.
pub fn max_resolution_at(index: i32) -> MaxResolution {
    value_at(&MAX_RESOLUTIONS, index)
}

/// Combo index of `platform` in `APPLE_PLATFORMS`, or `0` if missing.
pub fn apple_platform_index(platform: ApplePlatform) -> i32 {
    index_of(&APPLE_PLATFORMS, &platform)
}

/// Apple platform at `index`, or the default if the index is out of range.
pub fn apple_platform_at(index: i32) -> ApplePlatform {
    value_at(&APPLE_PLATFORMS, index)
}

/// Combo index of `platform` in `WHATSAPP_PLATFORMS`, or `0` if missing.
pub fn whatsapp_platform_index(platform: WhatsappPlatform) -> i32 {
    index_of(&WHATSAPP_PLATFORMS, &platform)
}

/// WhatsApp platform at `index`, or the default if the index is out of range.
pub fn whatsapp_platform_at(index: i32) -> WhatsappPlatform {
    value_at(&WHATSAPP_PLATFORMS, index)
}

/// Combo index for a timezone string.
///
/// Empty or unknown values map to 0 (Local time). A matching UTC offset is
/// stored at `position + 1` because index 0 is Local time.
pub fn timezone_index(timezone: &str) -> i32 {
    let trimmed = timezone.trim();
    if trimmed.is_empty() {
        return 0;
    }
    match UTC_OFFSETS.iter().position(|&offset| offset == trimmed) {
        Some(position) => (position + 1) as i32,
        None => 0,
    }
}

/// Timezone string for a combo index.
///
/// Index 0 (and anything below) is Local time, stored as an empty string.
pub fn timezone_at(index: i32) -> String {
    if index <= 0 {
        String::new()
    } else {
        let offset_index = (index as usize).saturating_sub(1);
        UTC_OFFSETS
            .get(offset_index)
            .copied()
            .unwrap_or("")
            .to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::exporter_separator_before_index;

    #[test]
    fn experimental_exporters_start_after_supported_exporters() {
        assert_eq!(exporter_separator_before_index(), 3);
    }
}
