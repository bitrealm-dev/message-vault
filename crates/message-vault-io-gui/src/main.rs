//! Slint desktop GUI for message-vault-io.
//!
//! In-process exporter libraries and `export.ini` persistence.

// Release builds use the Windows GUI subsystem so launching message-vault-io.exe
// does not open a console window. Debug builds keep a console for logging/panics.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod browse;
mod jobs;
mod options;
mod session_log;
mod staging;
mod start;
mod state;
mod sync;
mod wire;
mod wsl;

use std::sync::{Arc, Mutex};

use state::AppState;

slint::include_modules!();

fn main() -> Result<(), slint::PlatformError> {
    let ui = AppWindow::new()?;
    ui.set_app_title(format!("Message Vault {}", env!("CARGO_PKG_VERSION")).into());
    let state = Arc::new(Mutex::new(AppState::load()));

    sync::push_static_option_models(&ui);
    {
        let mut st = state.lock().expect("state lock");
        sync::push_all(&ui, &mut st);
    }
    sync::clear_log_lines(&ui);

    wire::wire_all(&ui, Arc::clone(&state));

    // Persist when the process exits after `run()` returns.
    // Pull guided workflow fields only — legacy adapters are not shown and
    // would overwrite Import / Credentials edits with stale values.
    let result = ui.run();
    {
        let mut st = state.lock().expect("state lock");
        sync::pull_credentials(&ui, &mut st);
        sync::pull_import(&ui, &mut st);
        st.persist_on_exit();
    }
    result
}
