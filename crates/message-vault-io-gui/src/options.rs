//! Combo-box option lists shared with the Slint adapters.

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

fn model_from_labels(labels: impl IntoIterator<Item = String>) -> ModelRc<SharedString> {
    let items: Vec<SharedString> = labels.into_iter().map(SharedString::from).collect();
    ModelRc::new(VecModel::from(items))
}

pub fn exporter_options() -> ModelRc<SharedString> {
    model_from_labels(EXPORTERS.iter().map(|e| e.dropdown_label()))
}

pub fn exporter_separator_before_index() -> i32 {
    EXPORTERS
        .iter()
        .position(|exporter| !exporter.is_supported())
        .map_or(-1, |index| index as i32)
}

pub fn output_format_options() -> ModelRc<SharedString> {
    model_from_labels(
        OUTPUT_FORMATS_ALPHABETICAL
            .iter()
            .map(|f| format!(".{}", f.as_str())),
    )
}

pub fn attachment_media_options() -> ModelRc<SharedString> {
    model_from_labels(
        ["Copy", "Convert", "Compress & Convert", "Skip"]
            .into_iter()
            .map(str::to_string),
    )
}

pub fn max_resolution_options() -> ModelRc<SharedString> {
    model_from_labels(MAX_RESOLUTIONS.iter().map(|r| r.as_str().to_string()))
}

pub fn apple_platform_options() -> ModelRc<SharedString> {
    model_from_labels(APPLE_PLATFORMS.iter().map(|p| p.as_ini_str().to_string()))
}

pub fn whatsapp_platform_options() -> ModelRc<SharedString> {
    model_from_labels(
        WHATSAPP_PLATFORMS
            .iter()
            .map(|p| p.as_ini_str().to_string()),
    )
}

pub fn timezone_options() -> ModelRc<SharedString> {
    let mut labels = vec!["Local time".to_string()];
    labels.extend(UTC_OFFSETS.iter().map(|s| (*s).to_string()));
    model_from_labels(labels)
}

pub fn region_options() -> ModelRc<SharedString> {
    model_from_labels(["USA".into(), "International".into()])
}

/// Format choices for the guided Import Messages screen.
pub fn guided_import_format_options() -> ModelRc<SharedString> {
    model_from_labels(["iPhone - iOS".into(), "macOS".into()])
}

/// Guided import format index: 0 = iPhone iOS, 1 = macOS.
pub fn guided_import_format_index(platform: ApplePlatform) -> i32 {
    match platform {
        ApplePlatform::MacOs => 1,
        _ => 0,
    }
}

pub fn guided_import_platform_at(index: i32) -> ApplePlatform {
    match index {
        1 => ApplePlatform::MacOs,
        _ => ApplePlatform::Ios,
    }
}

pub fn exporter_index(exporter: Exporter) -> i32 {
    EXPORTERS.iter().position(|&e| e == exporter).unwrap_or(0) as i32
}

pub fn exporter_at(index: i32) -> Exporter {
    EXPORTERS.get(index as usize).copied().unwrap_or_default()
}

pub fn output_format_index(format: OutputFormat) -> i32 {
    OUTPUT_FORMATS_ALPHABETICAL
        .iter()
        .position(|&f| f == format)
        .unwrap_or(0) as i32
}

pub fn output_format_at(index: i32) -> OutputFormat {
    OUTPUT_FORMATS_ALPHABETICAL
        .get(index as usize)
        .copied()
        .unwrap_or_default()
}

pub fn attachment_media_index(media: AttachmentMedia) -> i32 {
    ATTACHMENT_MEDIA
        .iter()
        .position(|&m| m == media)
        .unwrap_or(0) as i32
}

pub fn attachment_media_at(index: i32) -> AttachmentMedia {
    ATTACHMENT_MEDIA
        .get(index as usize)
        .copied()
        .unwrap_or_default()
}

pub fn max_resolution_index(res: MaxResolution) -> i32 {
    MAX_RESOLUTIONS.iter().position(|&r| r == res).unwrap_or(0) as i32
}

pub fn max_resolution_at(index: i32) -> MaxResolution {
    MAX_RESOLUTIONS
        .get(index as usize)
        .copied()
        .unwrap_or_default()
}

pub fn apple_platform_index(platform: ApplePlatform) -> i32 {
    APPLE_PLATFORMS
        .iter()
        .position(|&p| p == platform)
        .unwrap_or(0) as i32
}

pub fn apple_platform_at(index: i32) -> ApplePlatform {
    APPLE_PLATFORMS
        .get(index as usize)
        .copied()
        .unwrap_or_default()
}

pub fn whatsapp_platform_index(platform: WhatsappPlatform) -> i32 {
    WHATSAPP_PLATFORMS
        .iter()
        .position(|&p| p == platform)
        .unwrap_or(0) as i32
}

pub fn whatsapp_platform_at(index: i32) -> WhatsappPlatform {
    WHATSAPP_PLATFORMS
        .get(index as usize)
        .copied()
        .unwrap_or_default()
}

pub fn timezone_index(timezone: &str) -> i32 {
    let trimmed = timezone.trim();
    if trimmed.is_empty() {
        return 0;
    }
    UTC_OFFSETS
        .iter()
        .position(|&o| o == trimmed)
        .map(|i| (i + 1) as i32)
        .unwrap_or(0)
}

pub fn timezone_at(index: i32) -> String {
    if index <= 0 {
        String::new()
    } else {
        UTC_OFFSETS
            .get((index as usize).saturating_sub(1))
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
