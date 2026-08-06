import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { ExtractConfig, ExtractErrorEvent } from "./types";

export async function invokeExtract(config: ExtractConfig): Promise<void> {
  return invoke("extract", {
    source: config.source,
    path: config.path,
    outputDir: config.output_dir,
  });
}

export async function invokeCancel(): Promise<void> {
  return invoke("cancel");
}

/**
 * Subscribes to the three extraction events emitted by the Rust backend:
 * `extract:log` (String log line), `extract:finished` (String summary),
 * `extract:error` ({ detail, user_message? }).
 * Returns a single unlisten function that tears down all three listeners.
 */
export function onExtractEvents(callbacks: {
  onLog: (line: string) => void;
  onFinished: (summary: string) => void;
  onError: (err: ExtractErrorEvent) => void;
}): Promise<UnlistenFn> {
  return Promise.all([
    listen<string>("extract:log", (e) => callbacks.onLog(e.payload)),
    listen<string>("extract:finished", (e) => callbacks.onFinished(e.payload)),
    listen<ExtractErrorEvent>("extract:error", (e) => callbacks.onError(e.payload)),
  ]).then((unlisteners) => {
    return () => {
      unlisteners.forEach((u) => u());
    };
  });
}
