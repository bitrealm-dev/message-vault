use message_vault_io_core::CancelFlag;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

pub struct AppState {
    pub cancel_flag: CancelFlag,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            cancel_flag: Arc::new(AtomicBool::new(false)),
        }
    }
}
