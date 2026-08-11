import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  ExtractConfig,
  ExtractErrorEvent,
  ImportIssueEvent,
  ImportProgressEvent,
} from "./types";

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
  continue_on_error: boolean;
  skip_attachments: boolean;
  trust_export: boolean;
  contact_name_mode?: string;
  import_id?: number;
}

export interface PushFinishedReport {
  ok: boolean;
  messages: number;
  assets_uploaded: number;
  assets_bytes: number;
  conversations_ok: number;
  conversations_total: number;
}

export interface TauriJobResult {
  summary: string;
  report?: PushFinishedReport;
}

export async function invokePush(config: PushConfig): Promise<void> {
  return invoke("push", {
    baseUrl: config.base_url,
    username: config.username,
    key: config.key,
    inputDir: config.input_dir,
    mode: config.mode,
    force: config.force,
    continueOnError: config.continue_on_error,
    skipAttachments: config.skip_attachments,
    trustExport: config.trust_export,
    contactNameMode: config.contact_name_mode ?? "fill_missing",
    importId: config.import_id ?? null,
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

export interface HomeDirInfo {
  path: string;
  os: string;
}

export async function invokeHomeDir(): Promise<HomeDirInfo> {
  return invoke("home_dir");
}

/**
 * Subscribes to the extraction events emitted by the Rust backend:
 * `extract:log` (String log line), `extract:progress` (structured progress),
 * `extract:issue` (structured error or skip), `extract:finished` (String summary),
 * `extract:error` ({ detail, user_message? }).
 * Returns a single unlisten function that tears down all listeners.
 */
export function onExtractEvents(callbacks: {
  onLog: (line: string) => void;
  onProgress?: (event: ImportProgressEvent) => void;
  onIssue?: (event: ImportIssueEvent) => void;
  onFinished: (summary: string) => void;
  onError: (err: ExtractErrorEvent) => void;
}): Promise<UnlistenFn> {
  return Promise.all([
    listen<string>("extract:log", (e) => callbacks.onLog(e.payload)),
    listen<ImportProgressEvent>("extract:progress", (e) => callbacks.onProgress?.(e.payload)),
    listen<ImportIssueEvent>("extract:issue", (e) => callbacks.onIssue?.(e.payload)),
    listen<string>("extract:finished", (e) => callbacks.onFinished(e.payload)),
    listen<ExtractErrorEvent>("extract:error", (e) => callbacks.onError(e.payload)),
  ]).then((unlisteners) => {
    return () => {
      unlisteners.forEach((u) => u());
    };
  });
}

/**
 * Run a Tauri job that streams `extract:*` events. Subscribes before invoke,
 * resolves on `extract:finished`, rejects on `extract:error` or invoke failure.
 * Extract/push commands return as soon as the background thread starts, so
 * callers must use this (not bare `await invoke…`) to wait for completion.
 */
export async function awaitTauriJob(
  invokeFn: () => Promise<void>,
  onLog?: (line: string) => void,
  onProgress?: (event: ImportProgressEvent) => void,
  onIssue?: (event: ImportIssueEvent) => void,
): Promise<TauriJobResult> {
  let unlisten: UnlistenFn | undefined;
  try {
    return await new Promise<TauriJobResult>((resolve, reject) => {
      void (async () => {
        try {
          unlisten = await onExtractEvents({
            onLog: (line) => onLog?.(line),
            onProgress,
            onIssue,
            onFinished: (summary) => resolve(parseTauriJobResult(summary)),
            onError: (err) =>
              reject(new Error(err.user_message ?? err.detail)),
          });
          await invokeFn();
        } catch (e) {
          reject(e instanceof Error ? e : new Error(String(e)));
        }
      })();
    });
  } finally {
    unlisten?.();
  }
}

function parseTauriJobResult(summary: string): TauriJobResult {
  try {
    const report = JSON.parse(summary) as Partial<PushFinishedReport> & { summary?: unknown };
    if (
      typeof report.ok === "boolean" &&
      typeof report.messages === "number" &&
      typeof report.assets_uploaded === "number" &&
      typeof report.assets_bytes === "number" &&
      typeof report.conversations_ok === "number" &&
      typeof report.conversations_total === "number"
    ) {
      return {
        summary: typeof report.summary === "string" ? report.summary : summary,
        report: report as PushFinishedReport,
      };
    }
  } catch {
    // Extract jobs emit their human-readable summary directly.
  }
  return { summary };
}
