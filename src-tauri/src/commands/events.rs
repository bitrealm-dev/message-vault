//! Serializable payloads for Tauri events emitted by the commands.
//!
//! `ProcessEvent` in `message-vault-io-core` carries the same error fields but
//! derives only `Debug, Clone` — not `Serialize` — so Tauri cannot emit it
//! directly. These structs mirror the shapes the frontend subscribes to
//! (`web/src/lib/types.ts`).

use serde::Serialize;

/// Structured payload for `extract:progress`.
#[derive(Debug, Clone, Serialize)]
pub struct ExtractProgressEvent {
    pub step: String,
    pub done: usize,
    pub total: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
}

/// Payload of the `extract:error` event.
///
/// `user_message` is omitted from the JSON payload when absent
/// (`skip_serializing_if`), matching the frontend's `ExtractErrorEvent`
/// interface where it is optional.
#[derive(Debug, Clone, Serialize)]
pub struct ExtractErrorEvent {
    pub detail: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_message: Option<String>,
}
