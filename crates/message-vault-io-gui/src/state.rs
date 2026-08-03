//! Application state: the same `ExportIniState` + `Form` the other GUIs use,
//! plus job control and the session log.

use message_vault_io_core::{ExportIniState, Exporter, Form, ProcessControl};
use std::path::PathBuf;

use crate::session_log::SessionLog;

/// Workflow screens for the guided Vault import UI.
pub mod screen {
    pub const HOME: i32 = 0;
    pub const CREDENTIALS: i32 = 1;
    pub const IMPORT: i32 = 2;
}

pub struct AppState {
    pub export_ini: ExportIniState,
    pub form: Form,
    pub exporter: Exporter,
    pub auto_save_export_ini: bool,
    pub validate_input: String,
    pub validate_usa: bool,
    pub running: bool,
    pub control: ProcessControl,
    pub session_log: Option<SessionLog>,
    pub errors: Vec<String>,
    /// Workflow screen that produced `errors`.
    pub error_source_screen: Option<i32>,
    pub vault_source_note: String,
    /// When true, delete the staging directory after a successful vault upload.
    /// Defaults to false so staging data is retained.
    pub delete_staging_after_success: bool,
    /// Last staging directory created by a guided import (for status/logging).
    pub last_staging_dir: Option<PathBuf>,
}

impl AppState {
    pub fn load() -> Self {
        let (export_ini, form, load_error) = ExportIniState::load_or_default();
        let exporter = export_ini.exporter;
        let error_source_screen = load_error.as_ref().map(|_| screen::HOME);
        Self {
            export_ini,
            form,
            exporter,
            auto_save_export_ini: load_error.is_none(),
            validate_input: String::new(),
            validate_usa: true,
            running: false,
            control: ProcessControl::default(),
            session_log: None,
            errors: load_error.into_iter().collect(),
            error_source_screen,
            vault_source_note: String::new(),
            delete_staging_after_success: false,
            last_staging_dir: None,
        }
    }

    pub fn save_export_ini(&mut self) -> Result<(), String> {
        if !self.auto_save_export_ini {
            return Ok(());
        }
        self.export_ini.exporter = self.exporter;
        self.export_ini.save(&self.form)
    }

    pub fn persist_on_exit(&mut self) {
        if let Err(error) = self.save_export_ini() {
            eprintln!("Could not save settings: {error}");
        }
    }

    pub fn set_errors(&mut self, errors: Vec<String>, source_screen: i32) {
        self.errors = errors;
        self.error_source_screen = Some(source_screen);
    }

    pub fn clear_errors(&mut self) {
        self.errors.clear();
        self.error_source_screen = None;
    }

    pub fn error_text(&self) -> String {
        self.errors.join("\n")
    }

    pub fn prefill_vault_input(&mut self) {
        if self.export_ini.vault.input.trim().is_empty() {
            let from_export = self.form.output.trim();
            if !from_export.is_empty() {
                self.export_ini.vault.input = from_export.to_string();
            }
        }
    }

    pub fn begin_session_log(&mut self) {
        if self.session_log.is_none() {
            let dir = self
                .export_ini
                .path
                .parent()
                .filter(|p| !p.as_os_str().is_empty())
                .map(|p| p.to_path_buf())
                .unwrap_or_else(|| PathBuf::from("."));
            self.session_log = Some(SessionLog::new(&dir));
        } else if let Some(log) = &self.session_log {
            log.truncate();
        }
    }

    pub fn append_session_log(&self, line: &str) {
        if let Some(log) = &self.session_log {
            log.append(line);
        }
    }

    pub fn session_log_name(&self) -> String {
        self.session_log
            .as_ref()
            .map(|l| l.name.clone())
            .unwrap_or_default()
    }

    pub fn status_text(&self) -> String {
        if self.running {
            "Running…".into()
        } else {
            format!("Settings: {}", self.export_ini.path.display())
        }
    }
}
