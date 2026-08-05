use message_vault_io_core::{CancelFlag, ExportIniState};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

pub struct AppState {
    pub cancel_flag: CancelFlag,
    pub ini: ExportIniState,
}

impl AppState {
    pub fn new() -> Self {
        // load_or_default returns (state, form, error_message)
        let (ini, _form, _load_error) = ExportIniState::load_or_default();
        Self {
            cancel_flag: Arc::new(AtomicBool::new(false)),
            ini,
        }
    }
}
