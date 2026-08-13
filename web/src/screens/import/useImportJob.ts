import { useRef, useState } from "react";
import { useAuth } from "../../lib/auth";
import { apiClient, getBaseUrl } from "../../lib/api";
import {
  invokeExtract,
  invokePush,
  type TauriJobResult,
} from "../../lib/tauri";
import { useTauriJob } from "../../hooks/useTauriJob";
import { isTauri } from "../../lib/tauri-check";
import { resolveImportStagingDir } from "../../lib/system-settings";
import { saveImportSavedGroup } from "../../lib/savedGroups";
import type {
  AttachmentMediaMode,
  ContactNameMode,
  ImportIssueEvent,
  ImportProgressEvent,
} from "../../lib/types";
import {
  completionTextFor,
  type ImportIssue,
  type ImportSummaryView,
} from "../../components/import/ImportSummaryPanel";

export type ImportStep = {
  label: string;
  status: "pending" | "active" | "done" | "error";
  detail?: string;
  pathLink?: string;
  durationMs?: number | null;
};

export type ImportPhase = "form" | "progress" | "done";

export const PUSH_LOG_NAME = "vault-push.log";

type StageTiming = {
  extractStartedAt: number | null;
  parseStartedAt: number | null;
  parseEndedAt: number | null;
  convertStartedAt: number | null;
  convertEndedAt: number | null;
};

const EMPTY_TIMING: StageTiming = {
  extractStartedAt: null,
  parseStartedAt: null,
  parseEndedAt: null,
  convertStartedAt: null,
  convertEndedAt: null,
};

/** Three import steps shown in the progress view. */
function initialSteps(status: ImportStep["status"] = "pending"): ImportStep[] {
  return [
    { label: "Parse backup", status, detail: status === "active" ? "Parsing backup…" : undefined },
    { label: "Convert attachments", status: "pending" },
    { label: "Upload to vault", status: "pending" },
  ];
}

/** Index of the progress step that matches this server event. */
function stepIndexFor(step: ImportProgressEvent["step"]): number {
  if (step === "parse") return 0;
  if (step === "convert") return 1;
  return 2;
}

/** Present-tense verb shown while a step is running. */
function progressVerb(step: ImportProgressEvent["step"]): string {
  if (step === "upload") return "Uploading";
  if (step === "convert") return "Converting";
  return "Parsing";
}

/** Parse and convert durations from timestamps recorded during extract. */
function stageDurations(
  timing: StageTiming,
  extractFinishedAt: number,
): { parseMs: number; convertMs: number } {
  const parseStart =
    timing.parseStartedAt ?? timing.extractStartedAt ?? extractFinishedAt;
  if (timing.convertStartedAt != null) {
    return {
      parseMs: Math.max(0, (timing.parseEndedAt ?? timing.convertStartedAt) - parseStart),
      convertMs: Math.max(0, extractFinishedAt - timing.convertStartedAt),
    };
  }
  return {
    parseMs: Math.max(0, extractFinishedAt - (timing.extractStartedAt ?? parseStart)),
    convertMs: 0,
  };
}

export type ImportJobFormValues = {
  source: string;
  backupPath: string;
  backupPassword: string;
  attachmentMedia: AttachmentMediaMode;
  maxResolution: string;
  maxFps: string;
  minSizeMb: string;
  contactNameMode: ContactNameMode;
  conversationFilter: string;
  startDate: string;
  endDate: string;
  force: boolean;
  obfuscate: boolean;
  isIos: boolean;
};

/** Run extract then upload for one import, and keep step progress for the UI. */
export function useImportJob() {
  const { token } = useAuth();
  const { run: runTauriJob, cancel } = useTauriJob();
  const [running, setRunning] = useState(false);
  const [steps, setSteps] = useState<ImportStep[]>(() => initialSteps());
  const [phase, setPhase] = useState<ImportPhase>("form");
  const [summaryView, setSummaryView] = useState<ImportSummaryView | null>(null);
  const [stagingDir, setStagingDir] = useState<string | null>(null);
  const activeStepRef = useRef<ImportIssue["step"]>("parse");
  const issuesRef = useRef<ImportIssue[]>([]);
  const countsRef = useRef<{
    filesParsed?: number;
    messagesParsed?: number;
  }>({});
  const timingRef = useRef({ ...EMPTY_TIMING });

  function returnToForm(): void {
    setPhase("form");
    setSummaryView(null);
    setStagingDir(null);
  }

  function applyProgress(event: ImportProgressEvent): void {
    const now = performance.now();
    activeStepRef.current = event.step;

    if (event.step === "parse") {
      timingRef.current.parseStartedAt ??= now;
      countsRef.current.messagesParsed =
        event.total > 0 && event.done >= event.total ? event.total : event.done;
    } else if (event.step === "convert" && timingRef.current.convertStartedAt == null) {
      timingRef.current.parseEndedAt ??= now;
      timingRef.current.convertStartedAt = now;
    }

    const stepIndex = stepIndexFor(event.step);
    let rawDetail = `${event.done}/${event.total}`;
    if (event.status === "included_in_extract") {
      rawDetail = "Included in extract";
    } else if (event.status) {
      rawDetail = `${event.done}/${event.total} (${event.status})`;
    }

    const detail =
      event.status === "included_in_extract" && event.step === "convert"
        ? rawDetail
        : `${progressVerb(event.step)} ${rawDetail}`;
    const done = event.total > 0 && event.done >= event.total;

    setSteps((current) =>
      current.map((step, index) => {
        if (index < stepIndex) {
          return { ...step, status: "done" };
        }
        if (index > stepIndex) return step;
        return {
          ...step,
          status: done ? "done" : "active",
          detail,
        };
      }),
    );
  }

  function recordIssue(issue: ImportIssueEvent): void {
    issuesRef.current = [...issuesRef.current, issue];
  }

  async function startImport(form: ImportJobFormValues): Promise<void> {
    if (!isTauri()) return;
    const importStartedAt = performance.now();
    activeStepRef.current = "parse";
    issuesRef.current = [];
    countsRef.current = {};
    timingRef.current = { ...EMPTY_TIMING };
    setRunning(true);
    setPhase("progress");
    setSummaryView(null);
    setStagingDir(null);
    setSteps(initialSteps("active"));

    let importSessionId: number | null = null;
    let importCompleted = false;
    let parseMs: number | null = null;
    let convertMs: number | null = null;
    let uploadMs: number | null = null;
    let pushResult: TauriJobResult | null = null;
    let outputDir = "";
    try {
      const baseUrl = getBaseUrl();
      if (!token) throw new Error("Not authenticated");

      const importSession = await apiClient.post<{ id: number }>("/v1/imports", {
        source: form.source,
        tool: "message-vault-io",
        mode: "append",
      });
      importSessionId = importSession.id;

      outputDir = await resolveImportStagingDir(form.backupPath, form.source);
      setStagingDir(outputDir);
      setSteps((current) =>
        current.map((step, i) =>
          i === 0 ? { ...step, pathLink: outputDir, detail: "Extracting…" } : step,
        ),
      );

      timingRef.current.extractStartedAt = performance.now();
      const extractResult = await runTauriJob(
        () =>
          invokeExtract({
            source: form.source,
            path: form.backupPath,
            output_dir: outputDir,
            ...(form.isIos
              ? {
                  backup_password: form.backupPassword || undefined,
                  attachment_media: form.attachmentMedia,
                  media_max_resolution: form.maxResolution,
                  media_max_fps: form.maxFps,
                  media_min_size: `${form.minSizeMb.trim() || "20"}M`,
                  conversation_filter: form.conversationFilter || undefined,
                  start_date: form.startDate || undefined,
                  end_date: form.endDate || undefined,
                  obfuscate: form.obfuscate,
                }
              : {}),
          }),
        { onProgress: applyProgress, onIssue: recordIssue },
      );
      if (extractResult.extraction) {
        countsRef.current.filesParsed = extractResult.extraction.files_parsed;
        countsRef.current.messagesParsed = extractResult.extraction.messages_parsed;
      }

      const extractFinishedAt = performance.now();
      timingRef.current.convertEndedAt = extractFinishedAt;
      ({ parseMs, convertMs } = stageDurations(timingRef.current, extractFinishedAt));

      setSteps([
        {
          label: "Parse backup",
          status: "done",
          pathLink: outputDir,
          detail: "Extraction complete",
          durationMs: parseMs,
        },
        {
          label: "Convert attachments",
          status: "done",
          detail: "Attachments processed",
          durationMs: convertMs,
        },
        {
          label: "Upload to vault",
          status: "active",
          detail: "Uploading to vault…",
        },
      ]);

      activeStepRef.current = "upload";
      const uploadStartedAt = performance.now();
      pushResult = await runTauriJob(
        () =>
          invokePush({
            base_url: baseUrl,
            username: "",
            key: token,
            input_dir: outputDir,
            mode: "append",
            force: form.force,
            continue_on_error: true,
            skip_attachments: false,
            trust_export: false,
            contact_name_mode: form.contactNameMode,
            import_id: importSession.id,
          }),
        { onProgress: applyProgress, onIssue: recordIssue },
      );
      uploadMs = performance.now() - uploadStartedAt;
      importCompleted = true;

      setSteps((current) =>
        current.map((step, i) =>
          i === 2
            ? { ...step, status: "done", detail: "Upload complete", durationMs: uploadMs }
            : step,
        ),
      );
    } catch (e: unknown) {
      const msg = e instanceof Error ? e.message : String(e);
      issuesRef.current = [
        ...issuesRef.current,
        { kind: "error", step: activeStepRef.current, item: "Import", reason: msg },
      ];
      setSteps((current) =>
        current.map((step) =>
          step.status === "active" ? { ...step, status: "error" } : step,
        ),
      );
    } finally {
      const durationMs = performance.now() - importStartedAt;
      const pushReport = pushResult?.report;
      const finalSummary: ImportSummaryView = {
        status: importCompleted ? "completed" : "failed",
        ...countsRef.current,
        filesTotal: pushReport?.conversations_total ?? countsRef.current.filesParsed,
        filesSucceeded: pushReport?.conversations_ok,
        filesFailed: pushReport?.conversations_failed,
        filesSkipped: pushReport?.conversations_skipped,
        messagesAttempted: pushReport?.messages_attempted,
        messagesInserted: pushReport?.messages_inserted,
        messagesDeduped: pushReport?.messages_deduped,
        messagesFailed: pushReport?.messages_failed,
        parseMs,
        convertMs,
        uploadMs,
        durationMs,
        issues: issuesRef.current,
      };
      const durations = [parseMs, convertMs, uploadMs];
      setSteps((current) =>
        current.map((step, index) => {
          const duration = durations[index];
          if (duration == null) return step;
          return { ...step, durationMs: duration };
        }),
      );
      if (importSessionId) {
        try {
          await apiClient.post(`/v1/imports/${String(importSessionId)}/complete`, {
            ok: importCompleted,
            message_count: pushReport?.messages_inserted,
            attachment_count: pushReport?.assets_uploaded,
            bytes_uploaded: pushReport?.assets_bytes,
            parse_ms: parseMs,
            convert_ms: convertMs,
            upload_ms: uploadMs,
            duration_ms: durationMs,
            summary: {
              files_total: finalSummary.filesTotal,
              files_succeeded: finalSummary.filesSucceeded,
              files_failed: finalSummary.filesFailed,
              files_skipped: finalSummary.filesSkipped,
              messages_parsed: finalSummary.messagesParsed,
              messages_attempted: finalSummary.messagesAttempted,
              messages_inserted: finalSummary.messagesInserted,
              messages_deduped: finalSummary.messagesDeduped,
              messages_failed: finalSummary.messagesFailed,
            },
            issues: finalSummary.issues,
          });
        } catch {
          // Completing the session on the server is optional. The summary still shows local results.
        }
      }
      if (importSessionId != null) {
        saveImportSavedGroup({
          importSessionId,
          source: form.source,
          messagesInserted: pushReport?.messages_inserted,
        });
      }
      setSummaryView(finalSummary);
      setPhase("done");
      setRunning(false);
    }
  }

  return {
    phase,
    steps,
    running,
    summaryView,
    stagingDir,
    completionText: phase === "done" ? completionTextFor(summaryView?.status) : undefined,
    startImport,
    cancel,
    returnToForm,
  };
}
