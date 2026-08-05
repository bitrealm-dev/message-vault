//! Job start helpers and the shared library-job event bridge.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

use chrono::Local;
use contacts::{ValidateMode, probe_contacts_input, validate_contacts_file};
use message_vault_io_core::{
    Exporter, JobError, ProcessEvent, ensure_output_dir, is_cancelled, spawn_job,
};
use message_reexport::run as run_format;
use phone::PhoneRegion;
use slint::ComponentHandle;
use vault_pull::{
    ProgressEvent as VaultPullProgressEvent, QueryStats, VaultPullConfig, compose_query,
    query_stats as run_vault_query_stats, run as run_vault_pull,
};
use vault_push::{
    ProgressEvent as VaultProgressEvent, VaultPushConfig, authenticate as vault_authenticate,
    run as run_vault_push,
};

use crate::AppWindow;
use crate::CredentialsAdapter;
use crate::VaultExportAdapter;
use crate::jobs::{LibraryJob, library_job_for_exporter, prepare_library_config, run_and_log};
use crate::options;
use crate::staging::{self, IPHONE_IOS_IMPORTER, MACOS_IMPORTER};
use crate::state::{self, AppState};
use crate::sync;

/// Optional action after a job finishes successfully.
#[derive(Clone)]
enum OnSuccess {
    None,
    GoToImportScreen,
    GoToExportScreen,
    /// Apply Vault Export query summary (written by the job before finish).
    VaultExportQuery(Arc<Mutex<Option<String>>>),
}

pub(crate) fn report_errors(ui: &AppWindow, state: &mut AppState, errors: Vec<String>) {
    state.set_errors(errors, ui.get_workflow_screen());
    sync::push_chrome(ui, state);
}

/// Start a library job and bridge its events onto the Slint UI thread.
fn start_library_job(
    ui_weak: &slint::Weak<AppWindow>,
    state: &Arc<Mutex<AppState>>,
    label: String,
    job: LibraryJob,
    on_success: OnSuccess,
) {
    let Some(ui) = ui_weak.upgrade() else {
        return;
    };
    let source_screen = ui.get_workflow_screen();
    let (tx, rx) = mpsc::channel::<ProcessEvent>();
    {
        let mut st = state.lock().expect("state lock");
        if st.running {
            return;
        }
        st.begin_session_log();
        st.clear_errors();
        st.running = true;
        spawn_job(st.control.clone(), tx, label, job);
    }
    sync::show_embedded_log(&ui);
    sync::clear_log_lines(&ui);
    sync::push_chrome(&ui, &state.lock().expect("state lock"));

    let ui_weak = ui_weak.clone();
    let state_for_done = Arc::clone(state);
    // Coalesce UI flushes: session log is written immediately; the on-screen
    // buffer is updated at most once per outstanding event-loop callback.
    let pending_ui = Arc::new(PendingUiLog::default());
    std::thread::spawn(move || {
        while let Ok(first) = rx.recv() {
            let mut batch = vec![first];
            let deadline = Instant::now() + Duration::from_millis(50);
            loop {
                let remaining = deadline.saturating_duration_since(Instant::now());
                if remaining.is_zero() {
                    break;
                }
                match rx.recv_timeout(remaining) {
                    Ok(more) => {
                        let terminal = matches!(
                            more,
                            ProcessEvent::Finished(_) | ProcessEvent::Error { .. }
                        );
                        batch.push(more);
                        if terminal {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }

            let mut lines = Vec::with_capacity(batch.len());
            let mut finished = false;
            let mut banner: Option<String> = None;
            for event in batch {
                match event {
                    ProcessEvent::Started(s) => lines.push(format!("$ {s}")),
                    ProcessEvent::Log(s) => lines.push(s),
                    ProcessEvent::Finished(s) => {
                        lines.push(s);
                        finished = true;
                    }
                    ProcessEvent::Error {
                        detail,
                        user_message,
                    } => {
                        lines.push(detail.clone());
                        banner = Some(user_message.unwrap_or(detail));
                        finished = true;
                    }
                }
            }

            {
                let st = state_for_done.lock().expect("state lock");
                for line in &lines {
                    st.append_session_log(line);
                }
            }

            let chunk = lines.join("\n");
            {
                let mut pending = pending_ui.text.lock().expect("pending ui log");
                if !pending.is_empty() {
                    pending.push('\n');
                }
                pending.push_str(&chunk);
            }

            // Always flush terminal events; otherwise only schedule if idle.
            let schedule = finished
                || !pending_ui.scheduled.swap(true, Ordering::AcqRel);
            if schedule {
                let state_clone = Arc::clone(&state_for_done);
                let pending_ui = Arc::clone(&pending_ui);
                let on_success = on_success.clone();
                let _ = ui_weak.upgrade_in_event_loop(move |ui| {
                    let text = {
                        let mut pending = pending_ui.text.lock().expect("pending ui log");
                        pending_ui.scheduled.store(false, Ordering::Release);
                        std::mem::take(&mut *pending)
                    };
                    if !text.is_empty() {
                        sync::append_log_text(&ui, &text);
                    }
                    if finished {
                        let mut st = state_clone.lock().expect("state lock");
                        st.running = false;
                        if let Some(banner) = banner {
                            st.set_errors(vec![banner], source_screen);
                        } else if matches!(on_success, OnSuccess::GoToImportScreen) {
                            ui.set_workflow_screen(state::screen::IMPORT);
                            ui.global::<crate::ImportAdapter>().set_panel_tab(0);
                            sync::push_import(&ui, &st);
                        } else if matches!(on_success, OnSuccess::GoToExportScreen) {
                            ui.set_workflow_screen(state::screen::EXPORT);
                            ui.global::<crate::VaultExportAdapter>().set_panel_tab(0);
                            sync::push_vault_export(&ui);
                        } else if let OnSuccess::VaultExportQuery(slot) = &on_success {
                            if let Some(summary) = slot.lock().expect("query summary").take() {
                                let export = ui.global::<VaultExportAdapter>();
                                export.set_query_summary(summary.into());
                                export.set_query_ready(true);
                            }
                        }
                        sync::push_chrome(&ui, &st);
                    }
                });
            }
            if finished {
                break;
            }
        }
    });
}

#[derive(Default)]
struct PendingUiLog {
    text: Mutex<String>,
    scheduled: AtomicBool,
}

pub(crate) fn start_validate(
    ui_weak: &slint::Weak<AppWindow>,
    state: &Arc<Mutex<AppState>>,
    check_only: bool,
) {
    let Some(ui) = ui_weak.upgrade() else {
        return;
    };
    {
        let mut st = state.lock().expect("state lock");
        if st.running {
            return;
        }
        sync::pull_contacts(&ui, &mut st);
        let input = st.validate_input.trim();
        if input.is_empty() {
            report_errors(
                &ui,
                &mut st,
                vec!["Choose a contacts CSV or VCF file.".into()],
            );
            return;
        }
        let path = PathBuf::from(input);
        if let Err(error) = probe_contacts_input(&path) {
            report_errors(&ui, &mut st, vec![error.message]);
            return;
        }
        let region = if st.validate_usa {
            PhoneRegion::Usa
        } else {
            PhoneRegion::International
        };
        let mode = if check_only {
            ValidateMode::Check
        } else {
            ValidateMode::Update
        };
        let label = if check_only {
            "contacts-validate --check (library)".to_string()
        } else {
            "contacts-validate (library)".to_string()
        };
        let job: LibraryJob =
            Box::new(
                move |_cancel, tx| match validate_contacts_file(&path, region, mode) {
                    Ok(report) => {
                        for line in report.log_lines {
                            let _ = tx.send(ProcessEvent::Log(line));
                        }
                        Ok(())
                    }
                    Err(error) => Err(JobError::detail(format!("{error:#}"))),
                },
            );
        drop(st);
        start_library_job(ui_weak, state, label, job, OnSuccess::None);
    }
}

pub(crate) fn start_extract(ui_weak: &slint::Weak<AppWindow>, state: &Arc<Mutex<AppState>>) {
    let Some(ui) = ui_weak.upgrade() else {
        return;
    };
    let job_and_label = {
        let mut st = state.lock().expect("state lock");
        if st.running {
            return;
        }
        sync::pull_extract(&ui, &mut st);
        st.export_ini.exporter = st.exporter;
        if let Err(error) = st.save_export_ini() {
            report_errors(&ui, &mut st, vec![error]);
            return;
        }
        let result = st.form.to_config(st.exporter);
        let config = match result {
            Ok(config) => config,
            Err(errors) => {
                report_errors(&ui, &mut st, errors);
                return;
            }
        };
        if let Err(error) = ensure_output_dir(&config.output) {
            report_errors(&ui, &mut st, vec![error]);
            return;
        }
        let label = format!("{} (library)", st.exporter.binary());
        let job = library_job_for_exporter(st.exporter, config);
        Some((label, job))
    };
    if let Some((label, job)) = job_and_label {
        start_library_job(ui_weak, state, label, job, OnSuccess::None);
    }
}

pub(crate) fn start_format(ui_weak: &slint::Weak<AppWindow>, state: &Arc<Mutex<AppState>>) {
    let Some(ui) = ui_weak.upgrade() else {
        return;
    };
    let job_and_label = {
        let mut st = state.lock().expect("state lock");
        if st.running {
            return;
        }
        sync::pull_format(&ui, &mut st);
        if let Err(error) = st.save_export_ini() {
            report_errors(&ui, &mut st, vec![error]);
            return;
        }
        let result = st.form.to_format_config(
            &st.export_ini.format.input,
            &st.export_ini.format.output,
            st.export_ini.format.output_format,
        );
        let config = match result {
            Ok(config) => config,
            Err(errors) => {
                report_errors(&ui, &mut st, errors);
                return;
            }
        };
        if let Err(error) = ensure_output_dir(&config.output) {
            report_errors(&ui, &mut st, vec![error]);
            return;
        }
        let label = "Format (library)".to_string();
        let job: LibraryJob = Box::new(move |cancel, tx| {
            let config = prepare_library_config(config, cancel, &tx);
            run_and_log(run_format(&config), tx)
        });
        Some((label, job))
    };
    if let Some((label, job)) = job_and_label {
        start_library_job(ui_weak, state, label, job, OnSuccess::None);
    }
}

pub(crate) fn start_vault_auth(ui_weak: &slint::Weak<AppWindow>, state: &Arc<Mutex<AppState>>) {
    let Some(ui) = ui_weak.upgrade() else {
        return;
    };
    let job_and_label = {
        let mut st = state.lock().expect("state lock");
        if st.running {
            return;
        }
        sync::pull_vault(&ui, &mut st);
        let url = st.export_ini.vault.url.trim().to_string();
        let key = st.export_ini.vault.key.trim().to_string();
        let mut errors = Vec::new();
        if url.is_empty() {
            errors.push("Enter the URL for your Message Vault.".into());
        }
        if key.is_empty() {
            errors.push("Enter your Message Vault API key.".into());
        }
        if !errors.is_empty() {
            report_errors(&ui, &mut st, errors);
            return;
        }
        if let Err(error) = st.save_export_ini() {
            st.begin_session_log();
            st.append_session_log(&error);
            report_errors(
                &ui,
                &mut st,
                vec![
                    "Could not save your Vault URL and API key. Check that the app can write to its settings folder.".into(),
                ],
            );
            return;
        }
        let label = "vault-push auth".to_string();
        let job: LibraryJob = Box::new(move |_cancel, tx| {
            let _ = tx.send(ProcessEvent::Log(format!("Authenticating {url}…")));
            match vault_authenticate(&url, &key, "") {
                Ok(auth) => {
                    let name = auth
                        .username
                        .clone()
                        .filter(|s| !s.trim().is_empty())
                        .unwrap_or_else(|| auth.account_id.clone());
                    let _ = tx.send(ProcessEvent::Log(format!(
                        "Authenticated as {name} ({})",
                        auth.account_id
                    )));
                    Ok(())
                }
                Err(e) => Err(JobError::with_user_message(e.detail(), e.user_message())),
            }
        });
        Some((label, job))
    };
    if let Some((label, job)) = job_and_label {
        start_library_job(ui_weak, state, label, job, OnSuccess::None);
    }
}

/// Verify credentials from the guided workflow, then advance to Import or Export.
pub(crate) fn start_guided_verify(ui_weak: &slint::Weak<AppWindow>, state: &Arc<Mutex<AppState>>) {
    let Some(ui) = ui_weak.upgrade() else {
        return;
    };
    let operation = ui.global::<CredentialsAdapter>().get_operation_index();
    let on_success = if operation == 1 {
        OnSuccess::GoToExportScreen
    } else {
        OnSuccess::GoToImportScreen
    };
    let job_and_label = {
        let mut st = state.lock().expect("state lock");
        if st.running {
            return;
        }
        sync::pull_credentials(&ui, &mut st);
        let url = st.export_ini.vault.url.trim().to_string();
        let key = st.export_ini.vault.key.trim().to_string();
        let mut errors = Vec::new();
        if url.is_empty() {
            errors.push("Enter the URL for your Message Vault.".into());
        }
        if key.is_empty() {
            errors.push("Enter your Message Vault API key.".into());
        }
        if !errors.is_empty() {
            report_errors(&ui, &mut st, errors);
            return;
        }
        if let Err(error) = st.save_export_ini() {
            st.begin_session_log();
            st.append_session_log(&error);
            report_errors(
                &ui,
                &mut st,
                vec![
                    "Could not save your Vault URL and API key. Check that the app can write to its settings folder.".into(),
                ],
            );
            return;
        }
        let label = "vault-push auth".to_string();
        let job: LibraryJob = Box::new(move |_cancel, tx| {
            let _ = tx.send(ProcessEvent::Log(format!("Authenticating {url}…")));
            match vault_authenticate(&url, &key, "") {
                Ok(auth) => {
                    let name = auth
                        .username
                        .clone()
                        .filter(|s| !s.trim().is_empty())
                        .unwrap_or_else(|| auth.account_id.clone());
                    let _ = tx.send(ProcessEvent::Log(format!(
                        "Authenticated as {name} ({})",
                        auth.account_id
                    )));
                    Ok(())
                }
                Err(e) => Err(JobError::with_user_message(e.detail(), e.user_message())),
            }
        });
        Some((label, job))
    };
    if let Some((label, job)) = job_and_label {
        start_library_job(ui_weak, state, label, job, on_success);
    }
}

pub(crate) fn start_vault_export_query(
    ui_weak: &slint::Weak<AppWindow>,
    state: &Arc<Mutex<AppState>>,
) {
    let Some(ui) = ui_weak.upgrade() else {
        return;
    };
    let (label, job, on_success) = {
        let mut st = state.lock().expect("state lock");
        if st.running {
            return;
        }
        sync::pull_credentials(&ui, &mut st);
        let export = ui.global::<VaultExportAdapter>();
        export.set_query_ready(false);
        export.set_query_summary("".into());
        let url = st.export_ini.vault.url.trim().to_string();
        let key = st.export_ini.vault.key.trim().to_string();
        let search = export.get_search_query().trim().to_string();
        let start = export.get_start_date().trim().to_string();
        let end = export.get_end_date().trim().to_string();
        let mut errors = Vec::new();
        if url.is_empty() {
            errors.push("Vault URL is required. Open Credentials or Vault Import and set it.".into());
        }
        if key.is_empty() {
            errors.push("Vault key is required. Open Credentials or Vault Import and set it.".into());
        }
        if !errors.is_empty() {
            report_errors(&ui, &mut st, errors);
            return;
        }
        if let Err(error) = st.save_export_ini() {
            report_errors(&ui, &mut st, vec![error]);
            return;
        }
        let query = compose_query(
            &search,
            (!start.is_empty()).then_some(start.as_str()),
            (!end.is_empty()).then_some(end.as_str()),
        );
        let summary_slot = Arc::new(Mutex::new(None::<String>));
        let summary_for_job = Arc::clone(&summary_slot);
        let label = "vault-pull query (library)".to_string();
        let job: LibraryJob = Box::new(move |cancel, tx| {
            let cfg = VaultPullConfig {
                out_dir: PathBuf::new(),
                base_url: url,
                username: String::new(),
                key,
                query,
                after: None,
                before: None,
                source: None,
                skip_attachments: true,
                page_limit: vault_pull::DEFAULT_PAGE_LIMIT,
                cancel: Some(cancel),
            };
            let mut on_progress = |event: VaultPullProgressEvent| match event {
                VaultPullProgressEvent::Log(line) => {
                    let _ = tx.send(ProcessEvent::Log(line));
                }
                VaultPullProgressEvent::Auth {
                    account_id,
                    username,
                } => {
                    let _ = tx.send(ProcessEvent::Log(format!(
                        "Authenticated as {username} ({account_id})"
                    )));
                }
                VaultPullProgressEvent::Page {
                    messages,
                    total_so_far,
                } => {
                    let _ = tx.send(ProcessEvent::Log(format!(
                        "Page: {messages} message(s) ({total_so_far} total)"
                    )));
                }
                VaultPullProgressEvent::Done(_) => {}
            };
            match run_vault_query_stats(&cfg, Some(&mut on_progress)) {
                Ok(stats) => {
                    let summary = format_query_summary(&stats);
                    let _ = tx.send(ProcessEvent::Log(summary.clone()));
                    *summary_for_job.lock().expect("query summary") = Some(summary);
                    Ok(())
                }
                Err(e) => Err(JobError::detail(format!("{e:#}"))),
            }
        });
        (
            label,
            job,
            OnSuccess::VaultExportQuery(summary_slot),
        )
    };
    start_library_job(ui_weak, state, label, job, on_success);
}

pub(crate) fn start_vault_export(ui_weak: &slint::Weak<AppWindow>, state: &Arc<Mutex<AppState>>) {
    let Some(ui) = ui_weak.upgrade() else {
        return;
    };
    let job_and_label = {
        let mut st = state.lock().expect("state lock");
        if st.running {
            return;
        }
        sync::pull_credentials(&ui, &mut st);
        let export = ui.global::<VaultExportAdapter>();
        if !export.get_query_ready() {
            report_errors(
                &ui,
                &mut st,
                vec!["Run Query first to preview matching messages.".into()],
            );
            return;
        }
        let url = st.export_ini.vault.url.trim().to_string();
        let key = st.export_ini.vault.key.trim().to_string();
        let parent_raw = export.get_output().trim().to_string();
        let type_slug = options::vault_export_type_slug(export.get_exporter_index()).to_string();
        let search = export.get_search_query().trim().to_string();
        let start = export.get_start_date().trim().to_string();
        let end = export.get_end_date().trim().to_string();
        let skip_attachments = export.get_skip_attachments();
        let mut errors = Vec::new();
        if url.is_empty() {
            errors.push("Vault URL is required. Open Credentials or Vault Import and set it.".into());
        }
        if key.is_empty() {
            errors.push("Vault key is required. Open Credentials or Vault Import and set it.".into());
        }
        if !errors.is_empty() {
            report_errors(&ui, &mut st, errors);
            return;
        }
        let parent = if parent_raw.is_empty() {
            staging::default_export_parent()
        } else {
            PathBuf::from(&parent_raw)
        };
        let out = staging::export_dir_path(&parent, &type_slug, Local::now());
        if parent_raw.is_empty() {
            export.set_output(parent.display().to_string().into());
        }
        if let Err(error) = st.save_export_ini() {
            report_errors(&ui, &mut st, vec![error]);
            return;
        }
        let query = compose_query(
            &search,
            (!start.is_empty()).then_some(start.as_str()),
            (!end.is_empty()).then_some(end.as_str()),
        );
        let label = "vault-pull (library)".to_string();
        let job: LibraryJob = Box::new(move |cancel, tx| {
            let _ = tx.send(ProcessEvent::Log(format!(
                "Exporting to {}",
                out.display()
            )));
            let cfg = VaultPullConfig {
                out_dir: out,
                base_url: url,
                username: String::new(),
                key,
                query,
                after: None,
                before: None,
                source: None,
                skip_attachments,
                page_limit: vault_pull::DEFAULT_PAGE_LIMIT,
                cancel: Some(cancel),
            };
            let mut on_progress = |event: VaultPullProgressEvent| match event {
                VaultPullProgressEvent::Log(line) => {
                    let _ = tx.send(ProcessEvent::Log(line));
                }
                VaultPullProgressEvent::Auth {
                    account_id,
                    username,
                } => {
                    let _ = tx.send(ProcessEvent::Log(format!(
                        "Authenticated as {username} ({account_id})"
                    )));
                }
                VaultPullProgressEvent::Page {
                    messages,
                    total_so_far,
                } => {
                    let _ = tx.send(ProcessEvent::Log(format!(
                        "Page: {messages} message(s) ({total_so_far} total)"
                    )));
                }
                VaultPullProgressEvent::Done(report) => {
                    let _ = tx.send(ProcessEvent::Log(format!(
                        "Done: {} conversation(s), {} message(s), {} attachment(s) → {}",
                        report.conversations,
                        report.messages,
                        report.attachments_downloaded,
                        report.out_dir
                    )));
                }
            };
            match run_vault_pull(&cfg, Some(&mut on_progress)) {
                Ok(report) if report.ok => Ok(()),
                Ok(_) => Err(JobError::detail("Vault export finished with errors.")),
                Err(e) => Err(JobError::detail(format!("{e:#}"))),
            }
        });
        Some((label, job))
    };
    if let Some((label, job)) = job_and_label {
        start_library_job(ui_weak, state, label, job, OnSuccess::None);
    }
}

fn format_query_summary(stats: &QueryStats) -> String {
    format!(
        "{} messages · {} attachments · {}",
        stats.messages,
        stats.attachments,
        format_bytes_human(stats.total_bytes)
    )
}

fn format_bytes_human(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;
    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{} KB", bytes / KB)
    } else {
        format!("{bytes} B")
    }
}

pub(crate) fn start_vault_import(ui_weak: &slint::Weak<AppWindow>, state: &Arc<Mutex<AppState>>) {
    let Some(ui) = ui_weak.upgrade() else {
        return;
    };
    let job_and_label = {
        let mut st = state.lock().expect("state lock");
        if st.running {
            return;
        }
        sync::pull_vault(&ui, &mut st);
        st.prefill_vault_input();
        let url = st.export_ini.vault.url.trim().to_string();
        let key = st.export_ini.vault.key.trim().to_string();
        let input = st.export_ini.vault.input.trim().to_string();
        let mut errors = Vec::new();
        if url.is_empty() {
            errors.push("Vault URL is required.".into());
        }
        if key.is_empty() {
            errors.push("Vault key is required.".into());
        }
        if input.is_empty() {
            errors.push("Input directory is required.".into());
        }
        if !errors.is_empty() {
            report_errors(&ui, &mut st, errors);
            return;
        }
        if let Err(error) = st.save_export_ini() {
            report_errors(&ui, &mut st, vec![error]);
            return;
        }
        let continue_on_error = st.export_ini.vault.continue_on_error;
        let force = st.export_ini.vault.force;
        let skip_attachments = st.export_ini.vault.skip_attachments;
        let label = "vault-push (library)".to_string();
        let job: LibraryJob = Box::new(move |cancel, tx| {
            run_vault_upload(
                VaultUploadArgs {
                    input: PathBuf::from(input),
                    url,
                    key,
                    continue_on_error,
                    force,
                    skip_attachments,
                },
                cancel,
                &tx,
            )
        });
        Some((label, job))
    };
    if let Some((label, job)) = job_and_label {
        start_library_job(ui_weak, state, label, job, OnSuccess::None);
    }
}

/// Extract an iPhone backup into a staging directory, then upload it to Message Vault.
/// For Existing Archive (.jsonl), upload the selected folder with no extract step.
pub(crate) fn start_guided_import(ui_weak: &slint::Weak<AppWindow>, state: &Arc<Mutex<AppState>>) {
    let Some(ui) = ui_weak.upgrade() else {
        return;
    };
    let job_and_label = {
        let mut st = state.lock().expect("state lock");
        if st.running {
            return;
        }
        sync::pull_import(&ui, &mut st);
        sync::pull_credentials(&ui, &mut st);

        let url = st.export_ini.vault.url.trim().to_string();
        let key = st.export_ini.vault.key.trim().to_string();
        let is_archive =
            st.guided_import_format == crate::options::GuidedImportFormat::ExistingArchive;
        let archive = st.export_ini.vault.input.trim().to_string();
        let backup = st.form.db_path.trim().to_string();
        let is_macos =
            st.form.apple_platform == message_vault_io_core::ApplePlatform::MacOs;
        let mut errors = Vec::new();
        if url.is_empty() {
            errors.push("Vault URL is required. Go back and verify credentials.".into());
        }
        if key.is_empty() {
            errors.push("API Key is required. Go back and verify credentials.".into());
        }
        if is_archive {
            if archive.is_empty() {
                errors.push("Archive Directory is required.".into());
            }
        } else if backup.is_empty() {
            errors.push(if is_macos {
                "iMessage database path is required.".into()
            } else {
                "iPhone Backup Directory is required.".into()
            });
        }
        if !errors.is_empty() {
            report_errors(&ui, &mut st, errors);
            return;
        }

        if is_archive {
            if let Err(error) = st.save_export_ini() {
                report_errors(&ui, &mut st, vec![error]);
                return;
            }
            let continue_on_error = st.export_ini.vault.continue_on_error;
            let force = st.export_ini.vault.force;
            let skip_attachments = st.export_ini.vault.skip_attachments;
            let label = "vault import (existing archive)".to_string();
            let input = PathBuf::from(archive);
            let job: LibraryJob = Box::new(move |cancel, tx| {
                send_step_banner(&tx, "Uploading existing archive to Message Vault…");
                run_vault_upload(
                    VaultUploadArgs {
                        input,
                        url,
                        key,
                        continue_on_error,
                        force,
                        skip_attachments,
                    },
                    cancel,
                    &tx,
                )
            });
            Some((label, job))
        } else {
            let importer = if is_macos {
                MACOS_IMPORTER
            } else {
                IPHONE_IOS_IMPORTER
            };
            let staging = staging::staging_dir_path(&st.export_ini.path, importer, Local::now());
            st.form.output = staging.display().to_string();
            st.form.output_format = message_vault_io_core::OutputFormat::Jsonl;
            // apple_platform already set by pull_import from Import Format.
            st.exporter = Exporter::Imessage;
            st.export_ini.exporter = Exporter::Imessage;
            st.last_staging_dir = Some(staging.clone());

            if let Err(error) = st.save_export_ini() {
                report_errors(&ui, &mut st, vec![error]);
                return;
            }

            let result = st.form.to_config(Exporter::Imessage);
            let config = match result {
                Ok(config) => config,
                Err(errors) => {
                    report_errors(&ui, &mut st, errors);
                    return;
                }
            };
            if let Err(error) = ensure_output_dir(&config.output) {
                report_errors(&ui, &mut st, vec![error]);
                return;
            }

            let continue_on_error = st.export_ini.vault.continue_on_error;
            let force = st.export_ini.vault.force;
            let skip_attachments = st.export_ini.vault.skip_attachments;
            let label = "vault import (extract + upload)".to_string();
            let job: LibraryJob = Box::new(move |cancel, tx| {
                let _ = tx.send(ProcessEvent::Log(format!(
                    "Staging directory: {}",
                    staging.display()
                )));
                send_step_banner(
                    &tx,
                    if is_macos {
                        "Step 1/2: Extracting macOS Messages…"
                    } else {
                        "Step 1/2: Extracting iPhone backup…"
                    },
                );

                let extract_job = library_job_for_exporter(Exporter::Imessage, config);
                if let Err(error) = extract_job(cancel.clone(), tx.clone()) {
                    let _ = tx.send(ProcessEvent::Log(format!(
                        "Extraction failed; staging retained at {}",
                        staging.display()
                    )));
                    return Err(error);
                }

                if is_cancelled(Some(&cancel)) {
                    let _ = tx.send(ProcessEvent::Log(format!(
                        "Cancelled after extraction; staging retained at {}",
                        staging.display()
                    )));
                    return Err("cancelled".into());
                }

                send_step_banner(&tx, "Step 2/2: Uploading staging data to Message Vault…");
                match run_vault_upload(
                    VaultUploadArgs {
                        input: staging.clone(),
                        url,
                        key,
                        continue_on_error,
                        force,
                        skip_attachments,
                    },
                    cancel,
                    &tx,
                ) {
                    Ok(()) => {
                        let _ = tx.send(ProcessEvent::Log(format!(
                            "Staging data retained at {}",
                            staging.display()
                        )));
                        Ok(())
                    }
                    Err(error) => {
                        let _ = tx.send(ProcessEvent::Log(format!(
                            "Upload failed; staging retained at {}",
                            staging.display()
                        )));
                        Err(error)
                    }
                }
            });
            Some((label, job))
        }
    };
    if let Some((label, job)) = job_and_label {
        start_library_job(ui_weak, state, label, job, OnSuccess::None);
    }
}

fn send_step_banner(tx: &mpsc::Sender<ProcessEvent>, title: &str) {
    let _ = tx.send(ProcessEvent::Log("==========".into()));
    let _ = tx.send(ProcessEvent::Log(title.to_string()));
    let _ = tx.send(ProcessEvent::Log("==========".into()));
}

struct VaultUploadArgs {
    input: PathBuf,
    url: String,
    key: String,
    continue_on_error: bool,
    force: bool,
    skip_attachments: bool,
}

fn run_vault_upload(
    args: VaultUploadArgs,
    cancel: message_vault_io_core::CancelFlag,
    tx: &mpsc::Sender<ProcessEvent>,
) -> Result<(), JobError> {
    let cfg = VaultPushConfig {
        input: args.input,
        base_url: args.url,
        username: String::new(),
        key: args.key,
        mode: "append".into(),
        continue_on_error: args.continue_on_error,
        force: args.force,
        skip_attachments: args.skip_attachments,
        verify_digests: false,
        max_retries: 3,
        batch_size: vault_push::DEFAULT_BATCH_SIZE,
        // A bit higher than the CLI default: guided import often has many small assets.
        asset_upload_workers: vault_push::DEFAULT_ASSET_UPLOAD_WORKERS,
        asset_multipart_threshold: vault_push::MAX_PROXY_BODY_BYTES,
        asset_max_bytes: vault_push::DEFAULT_ASSET_MAX_BYTES,
        report_path: None,
        log_path: None,
        journal_path: None,
        cancel: Some(cancel),
    };
    let mut on_progress = |event: VaultProgressEvent| match event {
        VaultProgressEvent::Log(line) => {
            let _ = tx.send(ProcessEvent::Log(line));
        }
        VaultProgressEvent::Auth {
            account_id,
            username,
        } => {
            let _ = tx.send(ProcessEvent::Log(format!(
                "Authenticated as {username} ({account_id})"
            )));
        }
        VaultProgressEvent::FileStart {
            index,
            total,
            file,
        } => {
            // Sparse heartbeat so a long attachment upload doesn't look hung.
            if index == 1 || index == total || index.is_multiple_of(10) {
                let _ = tx.send(ProcessEvent::Log(format!(
                    "Preparing {index}/{total}: {file}"
                )));
            }
        }
        VaultProgressEvent::FileDone { .. } => {}
        VaultProgressEvent::Finished(report) => {
            for line in vault_push::format_push_summary(&report).lines() {
                let _ = tx.send(ProcessEvent::Log(line.to_string()));
            }
            let _ = tx.send(ProcessEvent::Log(String::new()));
        }
    };
    match run_vault_push(&cfg, Some(&mut on_progress)) {
        Ok(report) if report.ok => Ok(()),
        Ok(report) => Err(JobError::detail(format!(
            "import completed with failures (failed={})",
            report.conversations_failed
        ))),
        Err(e) => Err(JobError::detail(format!("{e:#}"))),
    }
}
