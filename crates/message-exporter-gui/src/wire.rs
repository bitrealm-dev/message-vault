//! Slint callback wiring for each workflow screen / chrome control.

use std::sync::{Arc, Mutex};

use chrono::{Datelike, Local, NaiveDate};
use message_exporter_core::VaultSection;
use slint::ComponentHandle;

use crate::browse;
use crate::options;
use crate::start;
use crate::state::{self, AppState};
use crate::sync;
use crate::wsl;
use crate::{
    AppWindow, ContactsAdapter, CredentialsAdapter, Date, ExtractAdapter, FormatAdapter,
    HomeAdapter, ImportAdapter, LogAdapter, VaultAdapter,
};

const DOCS_URL: &str = "https://bitrealm-dev.github.io/message-exporters/";

pub fn wire_all(ui: &AppWindow, state: Arc<Mutex<AppState>>) {
    wire_error_dismiss(ui, Arc::clone(&state));
    wire_navigate_back(ui);
    wire_home(ui);
    wire_credentials(ui, Arc::clone(&state));
    wire_import(ui, Arc::clone(&state));
    // Legacy adapters remain wired for reference until the deprecation pass.
    wire_contacts(ui, Arc::clone(&state));
    wire_extract(ui, Arc::clone(&state));
    wire_format(ui, Arc::clone(&state));
    wire_vault(ui, Arc::clone(&state));
    wire_log(ui, Arc::clone(&state));
}

fn wire_error_dismiss(ui: &AppWindow, state: Arc<Mutex<AppState>>) {
    let ui_weak = ui.as_weak();
    ui.on_error_dismissed(move || {
        let Some(ui) = ui_weak.upgrade() else {
            return;
        };
        let mut st = state.lock().expect("state lock");
        st.clear_errors();
        sync::push_chrome(&ui, &st);
    });
}

fn wire_navigate_back(ui: &AppWindow) {
    let ui_weak = ui.as_weak();
    ui.on_navigate_back(move || {
        let Some(ui) = ui_weak.upgrade() else {
            return;
        };
        let current = ui.get_workflow_screen();
        if current > state::screen::HOME {
            ui.set_workflow_screen(current - 1);
        }
    });
}

fn wire_home(ui: &AppWindow) {
    let ui_weak = ui.as_weak();
    ui.global::<HomeAdapter>().on_vault_import({
        let ui_weak = ui_weak.clone();
        move || {
            if let Some(ui) = ui_weak.upgrade() {
                ui.set_workflow_screen(state::screen::CREDENTIALS);
                ui.global::<CredentialsAdapter>().set_panel_tab(0);
            }
        }
    });
    // Convert messages is intentionally a no-op for this refactor phase.
    ui.global::<HomeAdapter>().on_convert_messages(move || {});
}

fn wire_credentials(ui: &AppWindow, state: Arc<Mutex<AppState>>) {
    let ui_weak = ui.as_weak();

    ui.global::<CredentialsAdapter>().on_open_help({
        let ui_weak = ui_weak.clone();
        let state = Arc::clone(&state);
        move || {
            if let Err(error) = wsl::open_url(DOCS_URL)
                && let Some(ui) = ui_weak.upgrade()
            {
                let mut st = state.lock().expect("state lock");
                start::report_errors(&ui, &mut st, vec![format!("Could not open help: {error}")]);
            }
        }
    });

    ui.global::<CredentialsAdapter>().on_verify({
        let ui_weak = ui_weak.clone();
        let state = Arc::clone(&state);
        move || start::start_guided_verify(&ui_weak, &state)
    });
}

fn wire_import(ui: &AppWindow, state: Arc<Mutex<AppState>>) {
    let ui_weak = ui.as_weak();

    ui.global::<ImportAdapter>().on_date_for_text(|value| {
        let date = NaiveDate::parse_from_str(value.trim(), "%Y-%m-%d")
            .unwrap_or_else(|_| Local::now().date_naive());
        Date {
            year: date.year(),
            month: i32::try_from(date.month()).expect("month fits in i32"),
            day: i32::try_from(date.day()).expect("day fits in i32"),
        }
    });

    ui.global::<ImportAdapter>().on_browse({
        let ui_weak = ui_weak.clone();
        move |field_id| {
            let kind = browse::browse_kind_for_field(&field_id);
            browse::pick_path(ui_weak.clone(), field_id.to_string(), kind);
        }
    });

    ui.global::<ImportAdapter>().on_open_help({
        let ui_weak = ui_weak.clone();
        let state = Arc::clone(&state);
        move || {
            if let Err(error) = wsl::open_url(DOCS_URL)
                && let Some(ui) = ui_weak.upgrade()
            {
                let mut st = state.lock().expect("state lock");
                start::report_errors(&ui, &mut st, vec![format!("Could not open help: {error}")]);
            }
        }
    });

    ui.global::<ImportAdapter>().on_media_changed({
        let ui_weak = ui_weak.clone();
        let state = Arc::clone(&state);
        move || {
            let Some(ui) = ui_weak.upgrade() else {
                return;
            };
            let mut st = state.lock().expect("state lock");
            sync::pull_import(&ui, &mut st);
            sync::push_import(&ui, &st);
        }
    });

    ui.global::<ImportAdapter>().on_run({
        let ui_weak = ui_weak.clone();
        let state = Arc::clone(&state);
        move || start::start_guided_import(&ui_weak, &state)
    });
}

fn wire_contacts(ui: &AppWindow, state: Arc<Mutex<AppState>>) {
    let ui_weak = ui.as_weak();
    ui.global::<ContactsAdapter>().on_browse({
        let ui_weak = ui_weak.clone();
        move |field_id| {
            let kind = browse::browse_kind_for_field(&field_id);
            browse::pick_path(ui_weak.clone(), field_id.to_string(), kind);
        }
    });

    ui.global::<ContactsAdapter>().on_check({
        let ui_weak = ui_weak.clone();
        let state = Arc::clone(&state);
        move || start::start_validate(&ui_weak, &state, true)
    });
    ui.global::<ContactsAdapter>().on_update({
        let ui_weak = ui_weak.clone();
        let state = Arc::clone(&state);
        move || start::start_validate(&ui_weak, &state, false)
    });
}

fn wire_extract(ui: &AppWindow, state: Arc<Mutex<AppState>>) {
    let ui_weak = ui.as_weak();

    ui.global::<ExtractAdapter>().on_date_for_text(|value| {
        let date = NaiveDate::parse_from_str(value.trim(), "%Y-%m-%d")
            .unwrap_or_else(|_| Local::now().date_naive());
        Date {
            year: date.year(),
            month: i32::try_from(date.month()).expect("month fits in i32"),
            day: i32::try_from(date.day()).expect("day fits in i32"),
        }
    });

    ui.global::<ExtractAdapter>().on_browse({
        let ui_weak = ui_weak.clone();
        move |field_id| {
            let kind = browse::browse_kind_for_field(&field_id);
            browse::pick_path(ui_weak.clone(), field_id.to_string(), kind);
        }
    });

    ui.global::<ExtractAdapter>().on_open_product_url({
        let ui_weak = ui_weak.clone();
        let state = Arc::clone(&state);
        move || {
            let url = state
                .lock()
                .expect("state lock")
                .exporter
                .product_url()
                .to_string();
            if let Err(error) = open::that(&url)
                && let Some(ui) = ui_weak.upgrade()
            {
                let mut st = state.lock().expect("state lock");
                start::report_errors(&ui, &mut st, vec![format!("Could not open link: {error}")]);
            }
        }
    });

    ui.global::<ExtractAdapter>().on_exporter_changed({
        let ui_weak = ui_weak.clone();
        let state = Arc::clone(&state);
        move |index| {
            let Some(ui) = ui_weak.upgrade() else {
                return;
            };
            let mut st = state.lock().expect("state lock");
            sync::pull_extract(&ui, &mut st);
            let next = options::exporter_at(index);
            if next != st.exporter {
                let AppState {
                    export_ini,
                    form,
                    exporter,
                    ..
                } = &mut *st;
                export_ini.switch_exporter(next, form);
                *exporter = next;
                form.advanced = false;
                st.clear_errors();
                if let Err(error) = st.save_export_ini() {
                    start::report_errors(&ui, &mut st, vec![error]);
                }
            }
            // Refresh visibility helpers after attachment / platform changes too.
            sync::push_extract(&ui, &st);
            sync::push_chrome(&ui, &st);
        }
    });

    ui.global::<ExtractAdapter>().on_run({
        let ui_weak = ui_weak.clone();
        let state = Arc::clone(&state);
        move || start::start_extract(&ui_weak, &state)
    });
    ui.global::<ExtractAdapter>().on_clear({
        let ui_weak = ui_weak.clone();
        let state = Arc::clone(&state);
        move || {
            let Some(ui) = ui_weak.upgrade() else {
                return;
            };
            let mut st = state.lock().expect("state lock");
            sync::pull_extract(&ui, &mut st);
            {
                let AppState {
                    export_ini, form, ..
                } = &mut *st;
                export_ini.clear_active_section(form);
            }
            st.clear_errors();
            if let Err(error) = st.save_export_ini() {
                start::report_errors(&ui, &mut st, vec![error]);
            }
            sync::push_extract(&ui, &st);
            sync::push_chrome(&ui, &st);
        }
    });
}

fn wire_format(ui: &AppWindow, state: Arc<Mutex<AppState>>) {
    let ui_weak = ui.as_weak();

    ui.global::<FormatAdapter>().on_browse({
        let ui_weak = ui_weak.clone();
        move |field_id| {
            let kind = browse::browse_kind_for_field(&field_id);
            browse::pick_path(ui_weak.clone(), field_id.to_string(), kind);
        }
    });

    ui.global::<FormatAdapter>().on_media_changed({
        let ui_weak = ui_weak.clone();
        let state = Arc::clone(&state);
        move || {
            let Some(ui) = ui_weak.upgrade() else {
                return;
            };
            let mut st = state.lock().expect("state lock");
            sync::pull_format(&ui, &mut st);
            sync::push_format(&ui, &st);
        }
    });

    ui.global::<FormatAdapter>().on_run({
        let ui_weak = ui_weak.clone();
        let state = Arc::clone(&state);
        move || start::start_format(&ui_weak, &state)
    });
    ui.global::<FormatAdapter>().on_clear({
        let ui_weak = ui_weak.clone();
        let state = Arc::clone(&state);
        move || {
            let Some(ui) = ui_weak.upgrade() else {
                return;
            };
            let mut st = state.lock().expect("state lock");
            st.export_ini.format = Default::default();
            st.clear_errors();
            if let Err(error) = st.save_export_ini() {
                start::report_errors(&ui, &mut st, vec![error]);
            }
            sync::push_format(&ui, &st);
            sync::push_chrome(&ui, &st);
        }
    });
}

fn wire_vault(ui: &AppWindow, state: Arc<Mutex<AppState>>) {
    let ui_weak = ui.as_weak();

    ui.global::<VaultAdapter>().on_browse({
        let ui_weak = ui_weak.clone();
        move |field_id| {
            let kind = browse::browse_kind_for_field(&field_id);
            browse::pick_path(ui_weak.clone(), field_id.to_string(), kind);
        }
    });

    ui.global::<VaultAdapter>().on_authenticate({
        let ui_weak = ui_weak.clone();
        let state = Arc::clone(&state);
        move || start::start_vault_auth(&ui_weak, &state)
    });
    ui.global::<VaultAdapter>().on_import({
        let ui_weak = ui_weak.clone();
        let state = Arc::clone(&state);
        move || start::start_vault_import(&ui_weak, &state)
    });
    ui.global::<VaultAdapter>().on_clear({
        let ui_weak = ui_weak.clone();
        let state = Arc::clone(&state);
        move || {
            let Some(ui) = ui_weak.upgrade() else {
                return;
            };
            let mut st = state.lock().expect("state lock");
            st.export_ini.vault = VaultSection {
                continue_on_error: true,
                ..Default::default()
            };
            st.vault_source_note.clear();
            st.clear_errors();
            if let Err(error) = st.save_export_ini() {
                start::report_errors(&ui, &mut st, vec![error]);
            }
            sync::push_vault(&ui, &mut st);
            sync::push_chrome(&ui, &st);
        }
    });
}

fn wire_log(ui: &AppWindow, state: Arc<Mutex<AppState>>) {
    let ui_weak = ui.as_weak();
    ui.global::<LogAdapter>().on_cancel({
        let ui_weak = ui_weak.clone();
        let state = Arc::clone(&state);
        move || {
            let Some(ui) = ui_weak.upgrade() else {
                return;
            };
            let mut st = state.lock().expect("state lock");
            match st.control.cancel() {
                Ok(()) => {
                    st.append_session_log("Cancellation requested…");
                    sync::append_log_line(&ui, "Cancellation requested…");
                }
                Err(error) => {
                    sync::append_log_line(&ui, &format!("Could not request cancellation: {error}"));
                    start::report_errors(&ui, &mut st, vec![error]);
                }
            }
        }
    });
    ui.global::<LogAdapter>().on_clear_view({
        let ui_weak = ui_weak.clone();
        move || {
            if let Some(ui) = ui_weak.upgrade() {
                sync::clear_log_lines(&ui);
            }
        }
    });
}
