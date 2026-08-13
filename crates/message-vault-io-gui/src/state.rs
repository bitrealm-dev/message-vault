//! In-memory state for the desktop window.
//!
//! `ExportIniState` is the on-disk settings file (`export.ini`).
//! `Form` is the Extract Messages field set shared with the other GUIs.
//! This module also tracks the running job and the session log file.

use message_vault_io_core::{ExportIniState, Exporter, Form, ProcessControl};
use std::path::PathBuf;

use crate::options::GuidedImportFormat;
use crate::session_log::SessionLog;
use crate::staging;

/// Integer ids for the guided workflow screens.
///
/// Slint stores the current screen as an `int`. These constants are the values
/// written to `workflow_screen` on the window.
pub mod screen {
    /// Home: choose Import, Extract, or Backup.
    pub const HOME: i32 = 0;
    /// Vault URL and API token, then Import or Export.
    pub const CREDENTIALS: i32 = 1;
    /// Guided import from an iPhone backup, macOS Messages, or an existing archive.
    pub const IMPORT: i32 = 2;
    /// Download matching messages from the vault.
    pub const EXPORT: i32 = 3;
    /// Download the entire vault account.
    pub const BACKUP: i32 = 4;
    /// Older Extract Messages form (full exporter list and field population).
    pub const EXTRACT: i32 = 5;
}

/// Mutable GUI state held behind a mutex and shared with background jobs.
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
    /// Last staging directory created by a guided import (for status and logging).
    pub last_staging_dir: Option<PathBuf>,
    /// Guided Import Messages format (iOS, macOS, or an existing archive).
    pub guided_import_format: GuidedImportFormat,
    /// Output directory for account backup (saved in `export.ini`).
    pub backup_output: String,
}

impl AppState {
    /// Load `export.ini` if present, otherwise start from defaults.
    ///
    /// A load failure is stored in `errors` and shown on the Home screen.
    /// Auto-save stays off until a later save succeeds, so a broken file is not overwritten.
    pub fn load() -> Self {
        let (export_ini, form, load_error) = ExportIniState::load_or_default();
        let exporter = export_ini.exporter;
        let guided_import_format = GuidedImportFormat::parse(&export_ini.vault.import_format)
            .unwrap_or_else(|| GuidedImportFormat::from_platform(form.apple_platform));
        let error_source_screen = load_error.as_ref().map(|_| screen::HOME);
        let backup_output = export_ini.backup.output.clone();
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
            last_staging_dir: None,
            guided_import_format,
            backup_output,
        }
    }

    /// Write the current form and exporter into `export.ini` when auto-save is on.
    ///
    /// # Errors
    ///
    /// Returns a message if the settings file cannot be written.
    pub fn save_export_ini(&mut self) -> Result<(), String> {
        if !self.auto_save_export_ini {
            return Ok(());
        }
        self.export_ini.exporter = self.exporter;
        self.export_ini.backup.output = self.backup_output.clone();
        self.export_ini.save(&self.form)
    }

    /// Save settings when the window closes. Print a message if the write fails.
    pub fn persist_on_exit(&mut self) {
        if let Err(error) = self.save_export_ini() {
            eprintln!("Could not save settings: {error}");
        }
    }

    /// Replace the error banner text and remember which screen produced it.
    pub fn set_errors(&mut self, errors: Vec<String>, source_screen: i32) {
        self.errors = errors;
        self.error_source_screen = Some(source_screen);
    }

    /// Clear the error banner and its source screen.
    pub fn clear_errors(&mut self) {
        self.errors.clear();
        self.error_source_screen = None;
    }

    /// Join error lines with newlines for the Slint error banner.
    pub fn error_text(&self) -> String {
        self.errors.join("\n")
    }

    /// If the Vault input path is empty, copy the Extract Messages output path into it.
    pub fn prefill_vault_input(&mut self) {
        if self.export_ini.vault.input.trim().is_empty() {
            let from_export = self.form.output.trim();
            if !from_export.is_empty() {
                self.export_ini.vault.input = from_export.to_string();
            }
        }
    }

    /// Open a new session log, or empty the existing one so a new job starts clean.
    pub fn begin_session_log(&mut self) {
        if self.session_log.is_none() {
            let dir = staging::export_ini_parent_dir(&self.export_ini.path);
            self.session_log = Some(SessionLog::new(&dir));
        } else if let Some(log) = &mut self.session_log {
            log.truncate();
        }
    }

    /// Append one line to the session log when a log file is open.
    pub fn append_session_log(&mut self, line: &str) {
        if let Some(log) = &mut self.session_log {
            log.append(line);
        }
    }

    /// Write any buffered session log bytes to disk.
    pub fn flush_session_log(&mut self) {
        if let Some(log) = &mut self.session_log {
            log.flush();
        }
    }

    /// File name of the current session log, or an empty string if none is open.
    pub fn session_log_name(&self) -> String {
        self.session_log
            .as_ref()
            .map(|log| log.name.clone())
            .unwrap_or_default()
    }

    /// Status bar text: "Running…" while a job is active, otherwise the settings path.
    pub fn status_text(&self) -> String {
        if self.running {
            "Running…".into()
        } else {
            format!("Settings: {}", self.export_ini.path.display())
        }
    }
}
