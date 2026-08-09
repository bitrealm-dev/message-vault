import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { ExtractConfig, ExtractErrorEvent } from "./types";

export async function invokeExtract(config: ExtractConfig): Promise<void> {
  return invoke("extract", {
    source: config.source,
    path: config.path,
    outputDir: config.output_dir,
    backupPassword: config.backup_password ?? null,
    attachmentMedia: config.attachment_media ?? null,
    mediaMaxResolution: config.media_max_resolution ?? null,
    mediaMaxFps: config.media_max_fps ?? null,
    mediaMinSize: config.media_min_size ?? null,
    conversationFilter: config.conversation_filter ?? null,
    startDate: config.start_date ?? null,
    endDate: config.end_date ?? null,
    obfuscate: config.obfuscate ?? null,
  });
}

export async function invokeCancel(): Promise<void> {
  return invoke("cancel");
}

export interface FormatConfig {
  input_dir: string;
  output_dir: string;
  output_format: string;
}

export async function invokeFormat(config: FormatConfig): Promise<void> {
  return invoke("format", {
    inputDir: config.input_dir,
    outputDir: config.output_dir,
    outputFormat: config.output_format,
  });
}

export interface PushConfig {
  base_url: string;
  username: string;
  key: string;
  input_dir: string;
  mode: string;
  force: boolean;
  skip_attachments: boolean;
  trust_export: boolean;
  contact_name_mode?: string;
}

export async function invokePush(config: PushConfig): Promise<void> {
  return invoke("push", {
    baseUrl: config.base_url,
    username: config.username,
    key: config.key,
    inputDir: config.input_dir,
    mode: config.mode,
    force: config.force,
    skipAttachments: config.skip_attachments,
    trustExport: config.trust_export,
    contactNameMode: config.contact_name_mode ?? "fill_missing",
  });
}

export interface PullConfig {
  base_url: string;
  username: string;
  key: string;
  out_dir: string;
  query: string;
  skip_attachments: boolean;
}

export async function invokePull(config: PullConfig): Promise<void> {
  return invoke("pull", {
    baseUrl: config.base_url,
    username: config.username,
    key: config.key,
    outDir: config.out_dir,
    query: config.query,
    skipAttachments: config.skip_attachments,
  });
}

/**
 * Subscribes to the three extraction events emitted by the Rust backend:
 * `extract:log` (String log line), `extract:finished` (String summary),
 * `extract:error` ({ detail, user_message? }).
 * Returns a single unlisten function that tears down all three listeners.
 */
export interface FfmpegToolsProbe {
  ok: boolean;
  ffmpeg_path: string | null;
  ffprobe_path: string | null;
  error: string | null;
}

export async function probeFfmpegTools(dir: string | null): Promise<FfmpegToolsProbe> {
  return invoke("probe_ffmpeg_tools", { dir });
}

export async function setFfmpegToolsDir(dir: string | null): Promise<FfmpegToolsProbe> {
  return invoke("set_ffmpeg_tools_dir", { dir });
}

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
