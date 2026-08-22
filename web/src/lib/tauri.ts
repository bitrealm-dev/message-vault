import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  ExtractConfig,
  ExtractErrorEvent,
  ImportIssueEvent,
  ImportProgressEvent,
} from "./types";

/** Start extracting a phone backup on the desktop backend. */
export async function invokeExtract(config: ExtractConfig): Promise<void> {
  return invoke("extract", {
    args: {
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
    },
  });
}

/** Ask the desktop backend to stop the job that is currently running. */
export async function invokeCancel(): Promise<void> {
  return invoke("cancel");
}

export interface FormatConfig {
  input_dir: string;
  output_dir: string;
  output_format: string;
}

/** Convert an extracted folder into another file format. */
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

interface PushFinishedReport {
  ok: boolean;
  /** Older field: messages counted in successful HTTP requests. */
  messages: number;
  messages_attempted: number;
  messages_inserted: number;
  messages_deduped: number;
  messages_failed: number;
  assets_uploaded: number;
  assets_bytes: number;
  conversations_ok: number;
  conversations_total: number;
  conversations_failed: number;
  conversations_skipped: number;
  results: Array<{
    file: string;
    status: string;
    error?: string;
    messages: number;
    attachments: number;
  }>;
}

export interface TauriJobResult {
  summary: string;
  report?: PushFinishedReport;
  extraction?: {
    files_parsed: number;
    messages_parsed: number;
  };
}

/** Upload extracted conversations to a vault server. */
export async function invokePush(config: PushConfig): Promise<void> {
  return invoke("push", {
    args: {
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
    },
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

/** Download conversations from a vault server into a folder. */
export async function invokePull(config: PullConfig): Promise<void> {
  return invoke("pull", {
    args: {
      baseUrl: config.base_url,
      username: config.username,
      key: config.key,
      outDir: config.out_dir,
      query: config.query,
      skipAttachments: config.skip_attachments,
    },
  });
}

export interface FfmpegToolsProbe {
  ok: boolean;
  ffmpeg_path: string | null;
  ffprobe_path: string | null;
  error: string | null;
}

/** Check whether ffmpeg and ffprobe are available at this folder. */
export async function probeFfmpegTools(dir: string | null): Promise<FfmpegToolsProbe> {
  return invoke("probe_ffmpeg_tools", { dir });
}

/** Save the ffmpeg tools folder and check that the tools are there. */
export async function setFfmpegToolsDir(dir: string | null): Promise<FfmpegToolsProbe> {
  return invoke("set_ffmpeg_tools_dir", { dir });
}

export interface HomeDirInfo {
  path: string;
  os: string;
}

/** User home folder and operating system name from the desktop backend. */
export async function invokeHomeDir(): Promise<HomeDirInfo> {
  return invoke("home_dir");
}

/**
 * Listen for job events from the desktop backend (log lines, progress, errors).
 * Returns one function that removes every listener.
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
      for (const u of unlisteners) {
        u();
      }
    };
  });
}

/**
 * Run a desktop job and wait until it finishes.
 * Extract and push return as soon as the background thread starts, so callers
 * must use this instead of awaiting the invoke call alone.
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
            onError: (err) => reject(new Error(err.user_message ?? err.detail)),
          });
          await invokeFn();
        } catch (e: unknown) {
          reject(e instanceof Error ? e : new Error(String(e)));
        }
      })();
    });
  } finally {
    unlisten?.();
  }
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function isPushFinishedReport(value: unknown): value is PushFinishedReport {
  if (!isRecord(value)) return false;
  return (
    typeof value.ok === "boolean" &&
    typeof value.messages === "number" &&
    typeof value.messages_attempted === "number" &&
    typeof value.messages_inserted === "number" &&
    typeof value.messages_deduped === "number" &&
    typeof value.messages_failed === "number" &&
    typeof value.assets_uploaded === "number" &&
    typeof value.assets_bytes === "number" &&
    typeof value.conversations_ok === "number" &&
    typeof value.conversations_total === "number"
  );
}

/** Turn a finished-job summary string into a structured result when it is JSON. */
function parseTauriJobResult(summary: string): TauriJobResult {
  try {
    const parsed: unknown = JSON.parse(summary);
    if (!isRecord(parsed)) return { summary };

    if (
      typeof parsed.summary === "string" &&
      typeof parsed.files_parsed === "number" &&
      typeof parsed.messages_parsed === "number"
    ) {
      return {
        summary: parsed.summary,
        extraction: {
          files_parsed: parsed.files_parsed,
          messages_parsed: parsed.messages_parsed,
        },
      };
    }

    const summaryText = typeof parsed.summary === "string" ? parsed.summary : summary;

    if (isPushFinishedReport(parsed)) {
      return {
        summary: summaryText,
        report: parsed,
      };
    }
  } catch {
    // Extract jobs send a plain sentence, not JSON.
  }
  return { summary };
}
