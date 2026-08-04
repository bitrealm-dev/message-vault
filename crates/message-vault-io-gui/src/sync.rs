//! Push `AppState` into Slint adapters and pull adapter values back into `Form`
//! / `ExportIniState` before validation and save.

use media::ffmpeg_available;
use message_vault_io_core::{
    ApplePlatform, AttachmentMedia, Exporter, OutputFormat, WhatsappPlatform,
    contacts_kind_from_path,
};
use slint::{ComponentHandle, SharedString};
use vault_push::detect_source as vault_detect_source;

use crate::options;
use crate::state::AppState;
use crate::{
    AppWindow, ContactsAdapter, CredentialsAdapter, ExtractAdapter, FormatAdapter, HomeAdapter,
    ImportAdapter, LogAdapter, VaultAdapter,
};

pub fn push_static_option_models(ui: &AppWindow) {
    let extract = ui.global::<ExtractAdapter>();
    extract.set_exporter_options(options::exporter_options());
    extract.set_exporter_separator_before_index(options::exporter_separator_before_index());
    extract.set_attachment_media_options(options::attachment_media_options());
    extract.set_max_resolution_options(options::max_resolution_options());
    extract.set_apple_platform_options(options::apple_platform_options());
    extract.set_whatsapp_platform_options(options::whatsapp_platform_options());
    extract.set_timezone_options(options::timezone_options());

    let format = ui.global::<FormatAdapter>();
    format.set_output_format_options(options::output_format_options());
    format.set_attachment_media_options(options::attachment_media_options());
    format.set_max_resolution_options(options::max_resolution_options());

    ui.global::<ContactsAdapter>()
        .set_region_options(options::region_options());

    let import = ui.global::<ImportAdapter>();
    import.set_format_options(options::guided_import_format_options());
    import.set_attachment_media_options(options::attachment_media_options());
    import.set_max_resolution_options(options::max_resolution_options());
}

pub fn push_all(ui: &AppWindow, state: &mut AppState) {
    push_contacts(ui, state);
    push_extract(ui, state);
    push_format(ui, state);
    push_vault(ui, state);
    push_credentials(ui, state);
    push_import(ui, state);
    push_chrome(ui, state);
}

pub fn push_chrome(ui: &AppWindow, state: &AppState) {
    ui.set_error_text(SharedString::from(state.error_text()));
    ui.set_error_source_screen(state.error_source_screen.unwrap_or(-1));
    ui.set_status_text(SharedString::from(state.status_text()));
    ui.set_workflow_enabled(!state.running);
    ui.global::<HomeAdapter>().set_enabled(!state.running);
    ui.global::<CredentialsAdapter>()
        .set_enabled(!state.running);
    ui.global::<ImportAdapter>().set_enabled(!state.running);

    let extract = ui.global::<ExtractAdapter>();
    extract.set_enabled(!state.running);
    ui.global::<ContactsAdapter>().set_enabled(!state.running);
    ui.global::<FormatAdapter>().set_enabled(!state.running);
    ui.global::<VaultAdapter>().set_enabled(!state.running);
    ui.global::<LogAdapter>().set_running(state.running);
    ui.global::<LogAdapter>()
        .set_session_log_name(SharedString::from(state.session_log_name()));
}

pub fn push_contacts(ui: &AppWindow, state: &AppState) {
    let contacts = ui.global::<ContactsAdapter>();
    contacts.set_input(SharedString::from(state.validate_input.as_str()));
    contacts.set_region_index(if state.validate_usa { 0 } else { 1 });
}

pub fn pull_contacts(ui: &AppWindow, state: &mut AppState) {
    let contacts = ui.global::<ContactsAdapter>();
    state.validate_input = contacts.get_input().to_string();
    state.validate_usa = contacts.get_region_index() == 0;
}

pub fn push_credentials(ui: &AppWindow, state: &AppState) {
    let credentials = ui.global::<CredentialsAdapter>();
    let v = &state.export_ini.vault;
    credentials.set_url(SharedString::from(v.url.as_str()));
    credentials.set_key(SharedString::from(v.key.as_str()));
}

pub fn pull_credentials(ui: &AppWindow, state: &mut AppState) {
    let credentials = ui.global::<CredentialsAdapter>();
    state.export_ini.vault.url = credentials.get_url().to_string();
    state.export_ini.vault.key = credentials.get_key().to_string();
}

pub fn push_import(ui: &AppWindow, state: &AppState) {
    let import = ui.global::<ImportAdapter>();
    let form = &state.form;

    import.set_format_index(0);
    import.set_backup_path(SharedString::from(form.db_path.as_str()));
    import.set_backup_password(SharedString::from(form.backup_password.as_str()));
    import.set_attachment_media_index(options::attachment_media_index(form.attachment_media));
    import.set_max_resolution_index(options::max_resolution_index(form.media_max_resolution));
    import.set_media_max_fps(SharedString::from(form.media_max_fps.as_str()));
    import.set_media_min_size(SharedString::from(form.media_min_size.as_str()));
    import.set_advanced(form.advanced);
    import.set_conversation_filter(SharedString::from(form.conversation_filter.as_str()));
    import.set_start_date(SharedString::from(form.start_date.as_str()));
    import.set_end_date(SharedString::from(form.end_date.as_str()));
    import.set_obfuscate(form.obfuscate);
    import.set_continue_on_error(state.export_ini.vault.continue_on_error);
    import.set_force(state.export_ini.vault.force);

    let obfuscate_active = form.obfuscate || !form.obfuscate_seed.trim().is_empty();
    import.set_show_ffmpeg_warning(
        !obfuscate_active && form.attachment_media.needs_ffmpeg() && !ffmpeg_available(),
    );
    import.set_show_compress_options(form.attachment_media == AttachmentMedia::Compress);
}

pub fn pull_import(ui: &AppWindow, state: &mut AppState) {
    let import = ui.global::<ImportAdapter>();
    let form = &mut state.form;

    state.exporter = Exporter::Imessage;
    form.output_format = OutputFormat::Jsonl;
    form.apple_platform = ApplePlatform::Ios;
    form.db_path = import.get_backup_path().to_string();
    form.backup_password = import.get_backup_password().to_string();
    form.attachment_media = options::attachment_media_at(import.get_attachment_media_index());
    form.media_max_resolution = options::max_resolution_at(import.get_max_resolution_index());
    form.media_max_fps = import.get_media_max_fps().to_string();
    form.media_min_size = import.get_media_min_size().to_string();
    form.advanced = import.get_advanced();
    form.conversation_filter = import.get_conversation_filter().to_string();
    form.start_date = import.get_start_date().to_string();
    form.end_date = import.get_end_date().to_string();
    form.obfuscate = import.get_obfuscate();
    state.export_ini.vault.continue_on_error = import.get_continue_on_error();
    state.export_ini.vault.force = import.get_force();
}

pub fn push_extract(ui: &AppWindow, state: &AppState) {
    let form = &state.form;
    let exporter = state.exporter;
    let extract = ui.global::<ExtractAdapter>();

    extract.set_exporter_key(SharedString::from(exporter.ini_key()));
    extract.set_exporter_index(options::exporter_index(exporter));
    extract.set_product_link_label(SharedString::from(exporter.link_label()));
    extract.set_product_url(SharedString::from(exporter.product_url()));

    extract.set_input(SharedString::from(form.input.as_str()));
    extract.set_output(SharedString::from(form.output.as_str()));
    extract.set_db_path(SharedString::from(form.db_path.as_str()));
    extract.set_contacts(SharedString::from(form.contacts.as_str()));
    extract.set_owner_phones(SharedString::from(form.owner_phones.as_str()));
    extract.set_owner_emails(SharedString::from(form.owner_emails.as_str()));
    extract.set_name_mapping(SharedString::from(form.name_mapping.as_str()));
    extract.set_timezone_index(options::timezone_index(&form.timezone));

    extract.set_attachment_media_index(options::attachment_media_index(form.attachment_media));
    extract.set_max_resolution_index(options::max_resolution_index(form.media_max_resolution));
    extract.set_media_max_fps(SharedString::from(form.media_max_fps.as_str()));
    extract.set_media_min_size(SharedString::from(form.media_min_size.as_str()));
    extract.set_media_skip_efficient(form.media_skip_efficient);

    extract.set_start_date(SharedString::from(form.start_date.as_str()));
    extract.set_end_date(SharedString::from(form.end_date.as_str()));
    extract.set_obfuscate(form.obfuscate);
    extract.set_obfuscate_seed(SharedString::from(form.obfuscate_seed.as_str()));
    extract.set_advanced(form.advanced);

    extract.set_whatsapp_platform_index(options::whatsapp_platform_index(form.whatsapp_platform));
    extract.set_whatsapp_backup(SharedString::from(form.whatsapp_backup.as_str()));
    extract.set_whatsapp_wa(SharedString::from(form.whatsapp_wa.as_str()));
    extract.set_whatsapp_key(SharedString::from(form.whatsapp_key.as_str()));
    extract.set_whatsapp_media(SharedString::from(form.whatsapp_media.as_str()));
    extract.set_whatsapp_db(SharedString::from(form.whatsapp_db.as_str()));
    extract.set_whatsapp_business(form.whatsapp_business);

    extract.set_apple_platform_index(options::apple_platform_index(form.apple_platform));
    extract.set_backup_password(SharedString::from(form.backup_password.as_str()));
    extract.set_apple_contacts(SharedString::from(form.apple_contacts.as_str()));
    extract.set_attachment_root(SharedString::from(form.attachment_root.as_str()));
    extract.set_conversation_filter(SharedString::from(form.conversation_filter.as_str()));

    let is_whatsapp = exporter == Exporter::Whatsapp;
    let is_imessage = exporter == Exporter::Imessage;
    let is_imazing = exporter == Exporter::Imazing;
    let is_sms_backup_plus = exporter == Exporter::SmsBackupPlus;
    let needs_owner_phones = matches!(
        exporter,
        Exporter::GoSmsPro | Exporter::SmsBackupRestore | Exporter::SmsBackupPlus
    );
    let whatsapp_is_ios = form.whatsapp_platform == WhatsappPlatform::Ios;
    let show_contacts = !is_imessage && !is_whatsapp;
    let obfuscate_active = form.obfuscate || !form.obfuscate_seed.trim().is_empty();
    let show_ffmpeg_warning =
        !obfuscate_active && form.attachment_media.needs_ffmpeg() && !ffmpeg_available();
    let show_compress_options = form.attachment_media == AttachmentMedia::Compress;
    let input_label = if exporter == Exporter::SmsBackupPlus {
        "Input file or folder"
    } else {
        "Input directory"
    };

    extract.set_is_whatsapp(is_whatsapp);
    extract.set_is_imessage(is_imessage);
    extract.set_is_imazing(is_imazing);
    extract.set_is_sms_backup_plus(is_sms_backup_plus);
    extract.set_needs_owner_phones(needs_owner_phones);
    extract.set_whatsapp_is_ios(whatsapp_is_ios);
    extract.set_show_contacts(show_contacts);
    extract.set_show_ffmpeg_warning(show_ffmpeg_warning);
    extract.set_show_compress_options(show_compress_options);
    extract.set_input_label(SharedString::from(input_label));
}

pub fn pull_extract(ui: &AppWindow, state: &mut AppState) {
    let extract = ui.global::<ExtractAdapter>();
    let form = &mut state.form;
    state.exporter = options::exporter_at(extract.get_exporter_index());

    form.output_format = OutputFormat::Jsonl;
    form.input = extract.get_input().to_string();
    form.output = extract.get_output().to_string();
    form.db_path = extract.get_db_path().to_string();
    form.contacts = extract.get_contacts().to_string();
    form.contacts_kind = contacts_kind_from_path(&form.contacts);
    form.owner_phones = extract.get_owner_phones().to_string();
    form.owner_emails = extract.get_owner_emails().to_string();
    form.name_mapping = extract.get_name_mapping().to_string();
    form.timezone = options::timezone_at(extract.get_timezone_index());

    form.attachment_media = options::attachment_media_at(extract.get_attachment_media_index());
    form.media_max_resolution = options::max_resolution_at(extract.get_max_resolution_index());
    form.media_max_fps = extract.get_media_max_fps().to_string();
    form.media_min_size = extract.get_media_min_size().to_string();
    form.media_skip_efficient = extract.get_media_skip_efficient();

    form.start_date = extract.get_start_date().to_string();
    form.end_date = extract.get_end_date().to_string();
    form.obfuscate = extract.get_obfuscate();
    form.obfuscate_seed = extract.get_obfuscate_seed().to_string();
    form.advanced = extract.get_advanced();

    form.whatsapp_platform = options::whatsapp_platform_at(extract.get_whatsapp_platform_index());
    form.whatsapp_backup = extract.get_whatsapp_backup().to_string();
    form.whatsapp_wa = extract.get_whatsapp_wa().to_string();
    form.whatsapp_key = extract.get_whatsapp_key().to_string();
    form.whatsapp_media = extract.get_whatsapp_media().to_string();
    form.whatsapp_db = extract.get_whatsapp_db().to_string();
    form.whatsapp_business = extract.get_whatsapp_business();

    form.apple_platform = options::apple_platform_at(extract.get_apple_platform_index());
    form.backup_password = extract.get_backup_password().to_string();
    form.apple_contacts = extract.get_apple_contacts().to_string();
    form.attachment_root = extract.get_attachment_root().to_string();
    form.conversation_filter = extract.get_conversation_filter().to_string();
}

pub fn push_format(ui: &AppWindow, state: &AppState) {
    let format = ui.global::<FormatAdapter>();
    let form = &state.form;
    format.set_input(SharedString::from(state.export_ini.format.input.as_str()));
    format.set_output(SharedString::from(state.export_ini.format.output.as_str()));
    format.set_output_format_index(options::output_format_index(
        state.export_ini.format.output_format,
    ));
    format.set_attachment_media_index(options::attachment_media_index(form.attachment_media));
    format.set_max_resolution_index(options::max_resolution_index(form.media_max_resolution));
    format.set_media_max_fps(SharedString::from(form.media_max_fps.as_str()));
    format.set_media_min_size(SharedString::from(form.media_min_size.as_str()));
    format.set_media_skip_efficient(form.media_skip_efficient);
    format.set_obfuscate(form.obfuscate);
    format.set_obfuscate_seed(SharedString::from(form.obfuscate_seed.as_str()));

    let obfuscate_active = form.obfuscate || !form.obfuscate_seed.trim().is_empty();
    format.set_show_ffmpeg_warning(
        !obfuscate_active && form.attachment_media.needs_ffmpeg() && !ffmpeg_available(),
    );
    format.set_show_compress_options(form.attachment_media == AttachmentMedia::Compress);
}

pub fn pull_format(ui: &AppWindow, state: &mut AppState) {
    let format = ui.global::<FormatAdapter>();
    state.export_ini.format.input = format.get_input().to_string();
    state.export_ini.format.output = format.get_output().to_string();
    state.export_ini.format.output_format =
        options::output_format_at(format.get_output_format_index());
    state.form.attachment_media = options::attachment_media_at(format.get_attachment_media_index());
    state.form.media_max_resolution = options::max_resolution_at(format.get_max_resolution_index());
    state.form.media_max_fps = format.get_media_max_fps().to_string();
    state.form.media_min_size = format.get_media_min_size().to_string();
    state.form.media_skip_efficient = format.get_media_skip_efficient();
    state.form.obfuscate = format.get_obfuscate();
    state.form.obfuscate_seed = format.get_obfuscate_seed().to_string();
}

pub fn push_vault(ui: &AppWindow, state: &mut AppState) {
    state.prefill_vault_input();
    let vault = ui.global::<VaultAdapter>();
    let v = &state.export_ini.vault;
    vault.set_url(SharedString::from(v.url.as_str()));
    vault.set_key(SharedString::from(v.key.as_str()));
    vault.set_input(SharedString::from(v.input.as_str()));
    vault.set_continue_on_error(v.continue_on_error);
    vault.set_force(v.force);
    vault.set_skip_attachments(v.skip_attachments);

    let note = if !state.vault_source_note.is_empty() {
        state.vault_source_note.clone()
    } else {
        vault_detect_source(std::path::Path::new(v.input.trim()))
            .ok()
            .flatten()
            .map(|s| format!("Detected source: {s}"))
            .unwrap_or_default()
    };
    vault.set_source_note(SharedString::from(note));
}

pub fn pull_vault(ui: &AppWindow, state: &mut AppState) {
    let vault = ui.global::<VaultAdapter>();
    state.export_ini.vault.url = vault.get_url().to_string();
    state.export_ini.vault.key = vault.get_key().to_string();
    state.export_ini.vault.input = vault.get_input().to_string();
    state.export_ini.vault.continue_on_error = vault.get_continue_on_error();
    state.export_ini.vault.force = vault.get_force();
    state.export_ini.vault.skip_attachments = vault.get_skip_attachments();
}

/// Soft cap for on-screen log text. The session log file keeps the full stream.
const UI_LOG_MAX_CHARS: usize = 256 * 1024;

pub fn set_log_lines(ui: &AppWindow, lines: &[String]) {
    ui.global::<LogAdapter>()
        .set_text(SharedString::from(trim_ui_log(&lines.join("\n"))));
}

pub fn append_log_line(ui: &AppWindow, line: &str) {
    append_log_text(ui, line);
}

pub fn append_log_text(ui: &AppWindow, text: &str) {
    if text.is_empty() {
        return;
    }
    let log = ui.global::<LogAdapter>();
    let current = log.get_text();
    let next = if current.is_empty() {
        text.to_string()
    } else {
        format!("{current}\n{text}")
    };
    log.set_text(SharedString::from(trim_ui_log(&next)));
}

fn trim_ui_log(text: &str) -> String {
    if text.len() <= UI_LOG_MAX_CHARS {
        return text.to_string();
    }
    let cut = text.len() - UI_LOG_MAX_CHARS;
    let cut = text[cut..]
        .find('\n')
        .map(|i| cut + i + 1)
        .unwrap_or(cut);
    format!("… (earlier log truncated)\n{}", &text[cut..])
}

pub fn clear_log_lines(ui: &AppWindow) {
    set_log_lines(ui, &[]);
}

pub fn show_embedded_log(ui: &AppWindow) {
    match ui.get_workflow_screen() {
        crate::state::screen::CREDENTIALS => {
            ui.global::<CredentialsAdapter>().set_panel_tab(1);
        }
        crate::state::screen::IMPORT => {
            ui.global::<ImportAdapter>().set_panel_tab(1);
        }
        _ => {}
    }
}
