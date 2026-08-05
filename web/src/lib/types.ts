export interface ExtractConfig {
  source: string;
  path: string;
  output_dir: string;
  media: MediaConfig;
}

export interface MediaConfig {
  mode: "copy" | "convert" | "compress" | "none";
  convert_resolution?: number;
  convert_fps?: number;
}

export interface ProgressEvent {
  kind: string;
  message: string;
  current: number;
  total?: number;
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
