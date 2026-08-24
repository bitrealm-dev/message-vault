//! JSON shapes sent to the UI as Tauri events.
//!
//! The core library has a similar error type, but it cannot be sent through
//! Tauri because it is not serializable. These structs match the TypeScript
//! types in `web/src/lib/types.ts`.

use serde::Serialize;

/// Progress numbers the UI uses to update the progress bar.
#[derive(Debug, Clone, Serialize)]
pub struct ExtractProgressEvent {
    /// Current pipeline stage, for example `parse`, `convert`, or `upload`.
    pub step: String,
    /// Number of items finished so far.
    pub done: usize,
    /// Total items, or 0 when the total is unknown.
    pub total: usize,
    /// Extra step status the UI shows, for example `included_in_extract`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
}

/// Failure details for the `extract:error` event.
///
/// When `user_message` is missing, it is left out of the JSON so the
/// TypeScript type can treat it as optional.
#[derive(Debug, Clone, Serialize)]
pub struct ExtractErrorEvent {
    /// Full error chain, for logs and the advanced-details view.
    pub detail: String,
    /// Friendlier message for the UI, when one is known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_message: Option<String>,
}
