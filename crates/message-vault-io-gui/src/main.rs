//! Desktop GUI for converting phone backups and talking to a Message Vault server.
//!
//! A Message Vault server stores imported conversations behind an HTTP API.
//! This crate draws the window with Slint, a Rust UI toolkit.
//! Exporters run in the same process as libraries, not as separate programs.
//! Settings are saved in `export.ini` next to the binary or in the working directory.

// Release builds use the Windows GUI subsystem so launching message-vault-io.exe
// does not open a console window. Debug builds keep a console for logging and panics.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod browse;
mod jobs;
mod options;
mod session_log;
mod staging;
mod start;
mod state;
mod sync;
mod theme;
mod wire;
mod wsl;

use std::sync::{Arc, Mutex};

use state::AppState;

slint::include_modules!();

/// Create the window, load saved settings, and run the Slint event loop.
///
/// # Errors
///
/// Returns a platform error if the window cannot be created or the event loop fails.
fn main() -> Result<(), slint::PlatformError> {
    let ui = AppWindow::new()?;
    ui.set_app_title(format!("Message Vault {}", env!("CARGO_PKG_VERSION")).into());
    let state = Arc::new(Mutex::new(AppState::load()));

    sync::push_static_option_models(&ui);
    theme::push_option_models(&ui);
    {
        let mut st = state.lock().expect("state lock");
        let (mode, preset) = theme::appearance_from_section(&st.export_ini.appearance);
        st.export_ini.appearance.mode = mode.as_ini().to_string();
        st.export_ini.appearance.preset = preset.id.to_string();
        theme::apply_to_ui(&ui, mode, preset);
        sync::push_all(&ui, &mut st);
    }
    sync::clear_log_lines(&ui);

    wire::wire_all(&ui, Arc::clone(&state));

    // Save settings after the window closes.
    // Copy only the guided Import and Credentials fields back from the UI.
    // The older Extract, Format, and Vault screens are hidden.
    // Reading those screens here would replace the guided fields with leftover values.
    let result = ui.run();
    {
        let mut st = state.lock().expect("state lock");
        sync::pull_credentials(&ui, &mut st);
        sync::pull_import(&ui, &mut st);
        st.persist_on_exit();
    }
    result
}
