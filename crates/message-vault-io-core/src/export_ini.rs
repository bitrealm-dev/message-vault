//! Load/save GUI export options as INI (`export.ini`).

use std::env;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use ini::Ini;
use media::MaxResolution;

#[cfg(unix)]
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

use crate::config::OutputFormat;
use crate::exporters::{
    ApplePlatform, AttachmentMedia, EXPORTERS, Exporter, Form, WhatsappPlatform,
    contacts_kind_from_path,
};

const COMMON: &str = "common";
const FORMAT: &str = "format";
const MESSAGE_REEXPORT_LEGACY: &str = "message-reexport";
const VAULT: &str = "vault";
const APPEARANCE: &str = "appearance";
const EXPORT_INI_NAME: &str = "export.ini";

/// Fields for the Format top-level tab (`message-reexporter`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FormatSection {
    pub input: String,
    pub output: String,
    pub output_format: OutputFormat,
}

/// Fields for the Vault top-level tab (`vault-push`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VaultSection {
    pub url: String,
    pub username: String,
    /// Import API token (persisted in `[vault] key`; treat `export.ini` as secret).
    pub key: String,
    pub input: String,
    /// Guided Import format: `ios` | `macos` | `existing-archive` (empty = derive from platform).
    pub import_format: String,
    pub continue_on_error: bool,
    pub force: bool,
    pub skip_attachments: bool,
}

impl Default for VaultSection {
    fn default() -> Self {
        Self {
            url: String::new(),
            username: String::new(),
            key: String::new(),
            input: String::new(),
            import_format: String::new(),
            continue_on_error: true,
            force: false,
            skip_attachments: false,
        }
    }
}

/// GUI appearance (`[appearance]`): theme mode and named preset.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppearanceSection {
    /// `light` | `dark` | `system` (default `dark`).
    pub mode: String,
    /// Preset id, e.g. `graphite-blue` (default).
    pub preset: String,
}

impl Default for AppearanceSection {
    fn default() -> Self {
        Self {
            mode: "dark".into(),
            preset: "graphite-blue".into(),
        }
    }
}

/// Per-exporter path / type-specific fields kept when switching backup types.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct ExporterSection {
    pub input: String,
    pub output: String,
    pub owner_emails: String,
    pub name_mapping: String,
    pub timezone: String,
    pub apple_platform: ApplePlatform,
    pub apple_contacts: String,
    pub attachment_root: String,
    pub conversation_filter: String,
    pub whatsapp_platform: WhatsappPlatform,
    pub whatsapp_backup: String,
    pub whatsapp_wa: String,
    pub whatsapp_media: String,
    pub whatsapp_db: String,
    pub whatsapp_business: bool,
}

/// In-memory export.ini plus the path used for load/save.
#[derive(Debug, Clone)]
pub struct ExportIniState {
    pub path: PathBuf,
    pub exporter: Exporter,
    sections: [ExporterSection; 7],
    pub format: FormatSection,
    pub vault: VaultSection,
    pub appearance: AppearanceSection,
}

impl ExportIniState {
    fn section_mut(&mut self, exporter: Exporter) -> &mut ExporterSection {
        &mut self.sections[exporter_index(exporter)]
    }

    fn section(&self, exporter: Exporter) -> &ExporterSection {
        &self.sections[exporter_index(exporter)]
    }

    /// Resolve the preferred path, loading it when present and preserving any load error.
    pub fn load_or_default() -> (Self, Form, Option<String>) {
        let path = resolve_export_ini_path();
        Self::load_or_default_at(path)
    }

    fn load_or_default_at(path: PathBuf) -> (Self, Form, Option<String>) {
        if path.is_file() {
            match Self::load(&path) {
                Ok((state, form)) => return (state, form, None),
                Err(error) => {
                    let (state, form) = Self::defaults_at(path);
                    return (state, form, Some(error));
                }
            }
        }
        let (state, form) = Self::defaults_at(path);
        (state, form, None)
    }

    fn defaults_at(path: PathBuf) -> (Self, Form) {
        let state = Self {
            path,
            exporter: Exporter::default(),
            sections: Default::default(),
            format: FormatSection::default(),
            vault: VaultSection {
                continue_on_error: true,
                ..VaultSection::default()
            },
            appearance: AppearanceSection::default(),
        };
        let mut form = Form::default();
        state.apply_section_to_form(&mut form);
        (state, form)
    }

    pub fn load(path: &Path) -> Result<(Self, Form), String> {
        let text = fs::read_to_string(path)
            .map_err(|e| format!("Could not read {}: {e}", path.display()))?;
        let ini = Ini::load_from_str(&text)
            .map_err(|e| format!("Could not parse {}: {e}", path.display()))?;

        let mut form = Form::default();
        apply_common(&ini, &mut form);

        let exporter = ini
            .get_from(Some(COMMON), "exporter")
            .and_then(Exporter::from_ini_key)
            .unwrap_or_default();

        let mut sections: [ExporterSection; 7] = Default::default();
        for (i, exp) in EXPORTERS.iter().copied().enumerate() {
            sections[i] = read_section(&ini, exp);
        }
        let format = read_format_section(&ini);
        let vault = read_vault_section(&ini);
        let appearance = read_appearance_section(&ini);

        let state = Self {
            path: path.to_path_buf(),
            exporter,
            sections,
            format,
            vault,
            appearance,
        };
        state.apply_section_to_form(&mut form);
        Ok((state, form))
    }

    /// Copy the active exporter's section fields into `form`.
    pub fn apply_section_to_form(&self, form: &mut Form) {
        let section = self.section(self.exporter);
        match self.exporter {
            Exporter::Imessage => {
                form.db_path = section.input.clone();
                form.input.clear();
            }
            _ => {
                form.input = section.input.clone();
                form.db_path.clear();
            }
        }
        form.output = section.output.clone();
        form.owner_emails = section.owner_emails.clone();
        form.name_mapping = section.name_mapping.clone();
        form.timezone = section.timezone.clone();
        form.apple_platform = section.apple_platform;
        form.apple_contacts = section.apple_contacts.clone();
        form.attachment_root = section.attachment_root.clone();
        form.conversation_filter = section.conversation_filter.clone();
        form.whatsapp_platform = section.whatsapp_platform;
        form.whatsapp_backup = section.whatsapp_backup.clone();
        form.whatsapp_wa = section.whatsapp_wa.clone();
        form.whatsapp_media = section.whatsapp_media.clone();
        form.whatsapp_db = section.whatsapp_db.clone();
        form.whatsapp_business = section.whatsapp_business;
        form.contacts_kind = contacts_kind_from_path(&form.contacts);
    }

    /// Store type-specific fields from `form` into the active exporter section.
    pub fn capture_form_section(&mut self, form: &Form) {
        let exporter = self.exporter;
        let section = self.section_mut(exporter);
        section.input = match exporter {
            Exporter::Imessage => form.db_path.clone(),
            _ => form.input.clone(),
        };
        section.output = form.output.clone();
        section.owner_emails = form.owner_emails.clone();
        section.name_mapping = form.name_mapping.clone();
        section.timezone = form.timezone.clone();
        section.apple_platform = form.apple_platform;
        section.apple_contacts = form.apple_contacts.clone();
        section.attachment_root = form.attachment_root.clone();
        section.conversation_filter = form.conversation_filter.clone();
        section.whatsapp_platform = form.whatsapp_platform;
        section.whatsapp_backup = form.whatsapp_backup.clone();
        section.whatsapp_wa = form.whatsapp_wa.clone();
        section.whatsapp_media = form.whatsapp_media.clone();
        section.whatsapp_db = form.whatsapp_db.clone();
        section.whatsapp_business = form.whatsapp_business;
    }

    /// Flush current section, switch exporter, apply the new section.
    pub fn switch_exporter(&mut self, next: Exporter, form: &mut Form) {
        if next == self.exporter {
            return;
        }
        self.capture_form_section(form);
        self.exporter = next;
        self.apply_section_to_form(form);
    }

    /// Empty the active exporter section and copy those blanks into `form`.
    /// Leaves `[common]` fields on `form` unchanged.
    pub fn clear_active_section(&mut self, form: &mut Form) {
        *self.section_mut(self.exporter) = ExporterSection::default();
        self.apply_section_to_form(form);
        form.backup_password.clear();
        form.whatsapp_key.clear();
        form.advanced = false;
    }

    pub fn save(&mut self, form: &Form) -> Result<(), String> {
        self.capture_form_section(form);
        let ini = build_ini(self, form);
        if let Some(parent) = self.path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent)
                    .map_err(|e| format!("Could not create {}: {e}", parent.display()))?;
            }
        }
        write_ini_restricted(&self.path, &ini)
    }
}

/// Write INI contents. On Unix, create/update with mode `0o600` (owner read/write only).
fn write_ini_restricted(path: &Path, ini: &Ini) -> Result<(), String> {
    let mut buf = Vec::new();
    ini.write_to(&mut buf)
        .map_err(|e| format!("Could not serialize {}: {e}", path.display()))?;

    #[cfg(unix)]
    {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(path)
            .map_err(|e| format!("Could not open {}: {e}", path.display()))?;
        file.write_all(&buf)
            .map_err(|e| format!("Could not write {}: {e}", path.display()))?;
        file.flush()
            .map_err(|e| format!("Could not flush {}: {e}", path.display()))?;
        // `.mode()` applies only on create; tighten an existing world-readable file.
        let mut perms = file
            .metadata()
            .map_err(|e| format!("Could not stat {}: {e}", path.display()))?
            .permissions();
        perms.set_mode(0o600);
        fs::set_permissions(path, perms)
            .map_err(|e| format!("Could not set permissions on {}: {e}", path.display()))?;
    }

    #[cfg(not(unix))]
    {
        let mut file = fs::File::create(path)
            .map_err(|e| format!("Could not create {}: {e}", path.display()))?;
        file.write_all(&buf)
            .map_err(|e| format!("Could not write {}: {e}", path.display()))?;
        file.flush()
            .map_err(|e| format!("Could not flush {}: {e}", path.display()))?;
    }

    Ok(())
}

/// Prefer an existing `export.ini` in cwd, then beside the executable; otherwise cwd.
fn resolve_export_ini_path() -> PathBuf {
    if let Ok(dir) = env::current_dir() {
        let candidate = dir.join(EXPORT_INI_NAME);
        if candidate.is_file() {
            return candidate;
        }
    }
    if let Ok(exe) = env::current_exe() {
        if let Some(dir) = exe.parent() {
            let candidate = dir.join(EXPORT_INI_NAME);
            if candidate.is_file() {
                return candidate;
            }
        }
    }
    env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join(EXPORT_INI_NAME)
}

fn exporter_index(exporter: Exporter) -> usize {
    EXPORTERS
        .iter()
        .position(|&e| e == exporter)
        .expect("exporter in EXPORTERS")
}

fn get(ini: &Ini, section: Option<&str>, key: &str) -> String {
    ini.get_from(section, key)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("")
        .to_string()
}

fn apply_common(ini: &Ini, form: &mut Form) {
    form.start_date = get(ini, Some(COMMON), "start_date");
    form.end_date = get(ini, Some(COMMON), "end_date");
    form.obfuscate = parse_bool(&get(ini, Some(COMMON), "obfuscate"), false);
    form.obfuscate_seed = get(ini, Some(COMMON), "obfuscate_seed");
    form.owner_phones = multiline_value(&get(ini, Some(COMMON), "owner_phones"));
    form.contacts = get(ini, Some(COMMON), "contacts");
    form.contacts_kind = contacts_kind_from_path(&form.contacts);

    if let Ok(format) = OutputFormat::parse(&get(ini, Some(COMMON), "output_format")) {
        form.output_format = format;
    }
    if let Some(media) = AttachmentMedia::from_ini_str(&get(ini, Some(COMMON), "attachment_media"))
    {
        form.attachment_media = media;
    }
    if let Some(res) = MaxResolution::parse(&get(ini, Some(COMMON), "media_max_resolution")) {
        form.media_max_resolution = res;
    }
    let fps = get(ini, Some(COMMON), "media_max_fps");
    if !fps.is_empty() {
        form.media_max_fps = fps;
    }
    let min_size = get(ini, Some(COMMON), "media_min_size");
    if !min_size.is_empty() {
        form.media_min_size = min_size;
    }
    form.media_skip_efficient = parse_bool(&get(ini, Some(COMMON), "media_skip_efficient"), true);
}

fn read_section(ini: &Ini, exporter: Exporter) -> ExporterSection {
    let name = exporter.ini_key();
    ExporterSection {
        input: get(ini, Some(name), "input"),
        output: get(ini, Some(name), "output"),
        owner_emails: multiline_value(&get(ini, Some(name), "owner_emails")),
        name_mapping: get(ini, Some(name), "name_mapping"),
        timezone: get(ini, Some(name), "timezone"),
        apple_platform: ApplePlatform::from_ini_str(&get(ini, Some(name), "apple_platform"))
            .unwrap_or_default(),
        apple_contacts: get(ini, Some(name), "apple_contacts"),
        attachment_root: get(ini, Some(name), "attachment_root"),
        conversation_filter: get(ini, Some(name), "conversation_filter"),
        whatsapp_platform: WhatsappPlatform::from_ini_str(&get(ini, Some(name), "platform"))
            .unwrap_or_default(),
        whatsapp_backup: get(ini, Some(name), "backup"),
        whatsapp_wa: get(ini, Some(name), "wa"),
        whatsapp_media: get(ini, Some(name), "media"),
        whatsapp_db: get(ini, Some(name), "db"),
        whatsapp_business: parse_bool(&get(ini, Some(name), "business"), false),
    }
}

fn build_ini(state: &ExportIniState, form: &Form) -> Ini {
    let mut ini = Ini::new();
    {
        let mut common = ini.with_section(Some(COMMON));
        common
            .set("exporter", state.exporter.ini_key())
            .set("start_date", form.start_date.trim())
            .set("end_date", form.end_date.trim())
            .set("obfuscate", bool_str(form.obfuscate))
            .set("obfuscate_seed", form.obfuscate_seed.trim())
            .set("owner_phones", escape_multiline(form.owner_phones.trim()))
            .set("contacts", form.contacts.trim())
            .set("output_format", form.output_format.as_str())
            .set("attachment_media", form.attachment_media.as_ini_str())
            .set("media_max_resolution", form.media_max_resolution.as_str())
            .set("media_max_fps", form.media_max_fps.trim())
            .set("media_min_size", form.media_min_size.trim())
            .set("media_skip_efficient", bool_str(form.media_skip_efficient));
    }

    for exporter in EXPORTERS {
        let section = state.section(exporter);
        let mut s = ini.with_section(Some(exporter.ini_key()));
        s.set("input", section.input.trim())
            .set("output", section.output.trim());
        match exporter {
            Exporter::SmsBackupPlus => {
                s.set(
                    "owner_emails",
                    escape_multiline(section.owner_emails.trim()),
                )
                .set("name_mapping", section.name_mapping.trim());
            }
            Exporter::Imazing => {
                s.set("timezone", section.timezone.trim());
            }
            Exporter::Imessage => {
                s.set("apple_platform", section.apple_platform.as_ini_str())
                    .set("apple_contacts", section.apple_contacts.trim())
                    .set("attachment_root", section.attachment_root.trim())
                    .set("conversation_filter", section.conversation_filter.trim());
                // backup_password intentionally omitted
            }
            Exporter::Whatsapp => {
                s.set("platform", section.whatsapp_platform.as_ini_str())
                    .set("backup", section.whatsapp_backup.trim())
                    .set("wa", section.whatsapp_wa.trim())
                    .set("media", section.whatsapp_media.trim())
                    .set("db", section.whatsapp_db.trim())
                    .set("business", bool_str(section.whatsapp_business));
                // whatsapp_key intentionally omitted (crypt15 hex / key path)
            }
            _ => {}
        }
    }

    {
        let mut s = ini.with_section(Some(FORMAT));
        s.set("input", state.format.input.trim())
            .set("output", state.format.output.trim())
            .set("output_format", state.format.output_format.as_str());
    }
    {
        let mut s = ini.with_section(Some(VAULT));
        s.set("url", state.vault.url.trim())
            .set("username", state.vault.username.trim())
            .set("key", state.vault.key.trim())
            .set("input", state.vault.input.trim())
            .set("import_format", state.vault.import_format.trim())
            .set("continue_on_error", bool_str(state.vault.continue_on_error))
            .set("force", bool_str(state.vault.force))
            .set("skip_attachments", bool_str(state.vault.skip_attachments));
    }
    {
        let mut s = ini.with_section(Some(APPEARANCE));
        s.set("mode", state.appearance.mode.trim())
            .set("preset", state.appearance.preset.trim());
    }
    ini
}

fn read_format_section(ini: &Ini) -> FormatSection {
    // Prefer `[format]`; fall back to legacy `[message-reexport]` for older files.
    let section = if ini.section(Some(FORMAT)).is_some() {
        FORMAT
    } else {
        MESSAGE_REEXPORT_LEGACY
    };
    let output_format = get(ini, Some(section), "output_format");
    FormatSection {
        input: get(ini, Some(section), "input"),
        output: get(ini, Some(section), "output"),
        output_format: OutputFormat::parse(&output_format).unwrap_or_default(),
    }
}

fn read_vault_section(ini: &Ini) -> VaultSection {
    VaultSection {
        url: get(ini, Some(VAULT), "url"),
        username: get(ini, Some(VAULT), "username"),
        key: get(ini, Some(VAULT), "key"),
        input: get(ini, Some(VAULT), "input"),
        import_format: get(ini, Some(VAULT), "import_format"),
        continue_on_error: parse_bool(&get(ini, Some(VAULT), "continue_on_error"), true),
        force: parse_bool(&get(ini, Some(VAULT), "force"), false),
        skip_attachments: parse_bool(&get(ini, Some(VAULT), "skip_attachments"), false),
    }
}

fn read_appearance_section(ini: &Ini) -> AppearanceSection {
    let defaults = AppearanceSection::default();
    let mode = get(ini, Some(APPEARANCE), "mode");
    let preset = get(ini, Some(APPEARANCE), "preset");
    AppearanceSection {
        mode: if mode.is_empty() {
            defaults.mode
        } else {
            mode
        },
        preset: if preset.is_empty() {
            defaults.preset
        } else {
            preset
        },
    }
}

fn parse_bool(s: &str, default: bool) -> bool {
    match s.trim().to_ascii_lowercase().as_str() {
        "" => default,
        "1" | "true" | "yes" | "on" => true,
        "0" | "false" | "no" | "off" => false,
        _ => default,
    }
}

fn bool_str(v: bool) -> &'static str {
    if v { "true" } else { "false" }
}

/// Accept literal `\n` sequences from single-line INI values.
fn multiline_value(raw: &str) -> String {
    raw.replace("\\n", "\n")
}

fn escape_multiline(raw: &str) -> String {
    raw.replace('\n', "\\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn roundtrip_common_and_sections() {
        let mut form = Form {
            start_date: "2020-01-01".into(),
            end_date: "2021-01-01".into(),
            obfuscate: true,
            obfuscate_seed: "01234567".into(),
            owner_phones: "+15555550100\n+15555550101".into(),
            contacts: "/tmp/contacts.vcf".into(),
            output_format: OutputFormat::Eml,
            attachment_media: AttachmentMedia::Compress,
            media_max_resolution: MaxResolution::P720,
            media_max_fps: "24".into(),
            media_min_size: "10M".into(),
            media_skip_efficient: false,
            input: "/data/go".into(),
            output: "/data/go/out".into(),
            ..Form::default()
        };

        let mut state = ExportIniState {
            path: PathBuf::from("unused"),
            exporter: Exporter::GoSmsPro,
            sections: Default::default(),
            format: FormatSection::default(),
            vault: VaultSection {
                continue_on_error: true,
                ..VaultSection::default()
            },
            appearance: AppearanceSection::default(),
        };
        state.capture_form_section(&form);
        state.switch_exporter(Exporter::SmsBackupPlus, &mut form);
        form.input = "/data/plus".into();
        form.output = "/data/plus/out".into();
        form.owner_emails = "a@example.com".into();
        form.name_mapping = "/data/map.csv".into();
        state.capture_form_section(&form);

        let mut file = NamedTempFile::new().unwrap();
        state.path = file.path().to_path_buf();
        state.save(&form).unwrap();
        file.as_file_mut().flush().unwrap();

        let (loaded, loaded_form) = ExportIniState::load(file.path()).unwrap();
        assert_eq!(loaded.exporter, Exporter::SmsBackupPlus);
        assert_eq!(loaded_form.start_date, "2020-01-01");
        assert_eq!(loaded_form.obfuscate, true);
        assert_eq!(loaded_form.output_format, OutputFormat::Eml);
        assert_eq!(loaded_form.owner_phones, "+15555550100\n+15555550101");
        assert_eq!(loaded_form.input, "/data/plus");
        assert_eq!(loaded_form.owner_emails, "a@example.com");
        assert_eq!(loaded.section(Exporter::GoSmsPro).input, "/data/go");

        // Password must never appear in the file.
        let text = fs::read_to_string(file.path()).unwrap();
        assert!(!text.contains("backup_password"));
        assert!(
            text.contains("exporter=sms-backup-plus")
                || text.contains("exporter = sms-backup-plus")
        );
        assert!(text.contains("output_format=eml") || text.contains("output_format = eml"));
    }

    #[test]
    fn clear_active_section_empties_only_that_exporter() {
        let mut form = Form {
            input: "/data/go".into(),
            output: "/data/go/out".into(),
            owner_phones: "+15555550100".into(),
            start_date: "2020-01-01".into(),
            ..Form::default()
        };
        let mut state = ExportIniState {
            path: PathBuf::from("unused"),
            exporter: Exporter::GoSmsPro,
            sections: Default::default(),
            format: FormatSection::default(),
            vault: VaultSection {
                continue_on_error: true,
                ..VaultSection::default()
            },
            appearance: AppearanceSection::default(),
        };
        state.capture_form_section(&form);
        state.switch_exporter(Exporter::Imazing, &mut form);
        form.input = "/data/imazing".into();
        form.output = "/data/imazing/out".into();
        form.timezone = "UTC-05:00".into();
        state.capture_form_section(&form);

        state.clear_active_section(&mut form);
        assert!(form.input.is_empty());
        assert!(form.output.is_empty());
        assert!(form.timezone.is_empty());
        assert_eq!(form.owner_phones, "+15555550100");
        assert_eq!(form.start_date, "2020-01-01");
        assert!(state.section(Exporter::Imazing).input.is_empty());
        assert_eq!(state.section(Exporter::GoSmsPro).input, "/data/go");
    }

    #[test]
    fn imessage_maps_input_to_db_path() {
        let text = r#"
[common]
exporter = iphone-backup

[iphone-backup]
input = /Users/me/chat.db
output = /tmp/out
apple_platform = ios
"#;
        let mut file = NamedTempFile::new().unwrap();
        write!(file, "{text}").unwrap();
        let (state, form) = ExportIniState::load(file.path()).unwrap();
        assert_eq!(state.exporter, Exporter::Imessage);
        assert_eq!(form.db_path, "/Users/me/chat.db");
        assert!(form.input.is_empty());
        assert_eq!(form.apple_platform, ApplePlatform::Ios);
    }

    #[test]
    fn load_or_default_reports_invalid_existing_file() {
        let mut file = NamedTempFile::new().unwrap();
        write!(file, "[invalid").unwrap();

        let (state, form, error) = ExportIniState::load_or_default_at(file.path().to_path_buf());

        assert_eq!(state.path, file.path());
        assert_eq!(state.exporter, Exporter::default());
        assert!(form.input.is_empty());
        assert!(form.output.is_empty());
        assert!(error.is_some_and(|message| message.contains("Could not parse")));
    }

    #[test]
    fn load_or_default_accepts_missing_file() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join(EXPORT_INI_NAME);

        let (state, form, error) = ExportIniState::load_or_default_at(path.clone());

        assert_eq!(state.path, path);
        assert_eq!(state.exporter, Exporter::default());
        assert!(form.input.is_empty());
        assert!(form.output.is_empty());
        assert!(error.is_none());
    }

    #[test]
    fn persists_vault_api_token_roundtrip() {
        let form = Form::default();
        let mut state = ExportIniState {
            path: PathBuf::from("unused"),
            exporter: Exporter::GoSmsPro,
            sections: Default::default(),
            format: FormatSection::default(),
            vault: VaultSection {
                url: "http://127.0.0.1:8080".into(),
                username: "alice".into(),
                key: "secret-import-token".into(),
                input: "/data/jsonl".into(),
                import_format: "existing-archive".into(),
                continue_on_error: true,
                force: false,
                skip_attachments: false,
            },
            appearance: AppearanceSection {
                mode: "system".into(),
                preset: "ocean".into(),
            },
        };
        let file = NamedTempFile::new().unwrap();
        state.path = file.path().to_path_buf();
        state.save(&form).unwrap();

        let (loaded, _) = ExportIniState::load(file.path()).unwrap();
        assert_eq!(loaded.vault.url, "http://127.0.0.1:8080");
        assert_eq!(loaded.vault.username, "alice");
        assert_eq!(loaded.vault.key, "secret-import-token");
        assert_eq!(loaded.vault.input, "/data/jsonl");
        assert_eq!(loaded.vault.import_format, "existing-archive");
        assert_eq!(loaded.appearance.mode, "system");
        assert_eq!(loaded.appearance.preset, "ocean");
        let text = fs::read_to_string(file.path()).unwrap();
        assert!(text.contains("key=secret-import-token") || text.contains("key = secret-import-token"));
        assert!(text.contains("[appearance]"));
        assert!(text.contains("mode=system") || text.contains("mode = system"));
        assert!(text.contains("preset=ocean") || text.contains("preset = ocean"));
    }

    #[cfg(unix)]
    #[test]
    fn export_ini_written_owner_only() {
        use std::os::unix::fs::PermissionsExt;

        let form = Form::default();
        let mut state = ExportIniState {
            path: PathBuf::from("unused"),
            exporter: Exporter::GoSmsPro,
            sections: Default::default(),
            format: FormatSection::default(),
            vault: VaultSection {
                key: "token".into(),
                continue_on_error: true,
                ..VaultSection::default()
            },
            appearance: AppearanceSection::default(),
        };
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("export.ini");
        // Pre-create a world-readable file; save must tighten mode.
        fs::write(&path, "stale\n").unwrap();
        let mut loose = fs::metadata(&path).unwrap().permissions();
        loose.set_mode(0o644);
        fs::set_permissions(&path, loose).unwrap();

        state.path = path.clone();
        state.save(&form).unwrap();
        let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "export.ini must be owner read/write only");
    }

    #[test]
    fn loads_legacy_message_reexport_section_and_saves_as_format() {
        let text = r#"
[common]
exporter = go-sms-pro

[message-reexport]
input = /old/in
output = /old/out
output_format = csv
"#;
        let mut file = NamedTempFile::new().unwrap();
        write!(file, "{text}").unwrap();

        let (mut state, form) = ExportIniState::load(file.path()).unwrap();
        assert_eq!(state.format.input, "/old/in");
        assert_eq!(state.format.output, "/old/out");
        assert_eq!(state.format.output_format, OutputFormat::Csv);

        state.path = file.path().to_path_buf();
        state.save(&form).unwrap();
        let saved = fs::read_to_string(file.path()).unwrap();
        assert!(saved.contains("[format]"));
        assert!(!saved.contains("[message-reexport]"));
        assert!(saved.contains("input=/old/in") || saved.contains("input = /old/in"));
    }
}
