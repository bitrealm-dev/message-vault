use message_vault_io_core::{CancelFlag, ExportIniState, Form};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

pub struct AppState {
    pub cancel_flag: CancelFlag,
    pub ini: ExportIniState,
    pub form: Form,
    pub errors: Vec<String>,
}

impl AppState {
    pub fn new() -> Self {
        let (ini, form, load_error) = ExportIniState::load_or_default();
        Self {
            cancel_flag: Arc::new(AtomicBool::new(false)),
            ini,
            form,
            errors: load_error.into_iter().collect(),
        }
    }
}
