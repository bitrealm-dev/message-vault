export interface ExtractConfig {
  source: string;
  path: string;
  output_dir: string;
}

/**
 * Payload of the `extract:error` Tauri event emitted by the Rust backend.
 * Matches the Rust `ExtractErrorEvent` struct: `user_message` is optional and
 * omitted from the JSON payload when absent (serde skip_serializing_if).
 */
export interface ExtractErrorEvent {
  detail: string;
  user_message?: string;
}
