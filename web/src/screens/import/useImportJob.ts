import { useRef, useState } from "react";
import {
  completionTextFor,
  type ImportIssue,
  type ImportSummaryView,
} from "../../components/import/ImportSummaryPanel";
import { useTauriJob } from "../../hooks/useTauriJob";
import { apiClient, getBaseUrl } from "../../lib/api";
import { formatAttachmentProgress } from "../../lib/attachmentProgressCopy";
import { attachmentStepCopy } from "../../lib/attachmentStepCopy";
import { useAuth } from "../../lib/auth";
import { imessageExtractFields } from "../../lib/imessageExtractFields";
import { isImessageMethod } from "../../lib/imessageImport";
import { saveImportSavedGroup } from "../../lib/savedGroups";
import { sbrExtractFields } from "../../lib/sbrExtractFields";
import { resolveImportStagingDir } from "../../lib/system-settings";
import { invokeExtract, invokePush, type TauriJobResult } from "../../lib/tauri";
import { isTauri } from "../../lib/tauri-check";
import type {
  AttachmentMediaMode,
  ContactNameMode,
  ImportIssueEvent,
  ImportProgressEvent,
} from "../../lib/types";
import { whatsappExtractFields } from "../../lib/whatsappExtractFields";
import { isWhatsappMethod } from "../../lib/whatsappImport";

export type ImportStep = {
  label: string;
  status: "pending" | "active" | "done" | "error";
  detail?: string;
  durationMs?: number | null;
};

export type ImportPhase = "form" | "progress" | "done";

export const PUSH_LOG_NAME = "vault-push.log";

type StageTiming = {
  extractStartedAt: number | null;
  parseStartedAt: number | null;
  parseEndedAt: number | null;
  attachmentsStartedAt: number | null;
  attachmentsEndedAt: number | null;
  prepareStartedAt: number | null;
  prepareEndedAt: number | null;
};

type AttachmentProgressCounts = {
  done: number;
  total: number;
  bytesDone: number;
  bytesTotal: number;
};

const EMPTY_TIMING: StageTiming = {
  extractStartedAt: null,
  parseStartedAt: null,
  parseEndedAt: null,
  attachmentsStartedAt: null,
  attachmentsEndedAt: null,
  prepareStartedAt: null,
  prepareEndedAt: null,
};

/** Four import steps shown in the progress view. */
function initialSteps(
  status: ImportStep["status"] = "pending",
  attachmentMedia: AttachmentMediaMode = "copy",
): ImportStep[] {
  const attachments = attachmentStepCopy(attachmentMedia);
  return [
    { label: "Parse backup", status, detail: status === "active" ? "Parsing backup…" : undefined },
    { label: attachments.label, status: "pending" },
    { label: "Preparing messages", status: "pending" },
    { label: "Upload to vault", status: "pending" },
  ];
}

/** Index of the progress step that matches this server event. */
function stepIndexFor(step: ImportProgressEvent["step"]): number {
  if (step === "parse") return 0;
  if (step === "attachments") return 1;
  if (step === "prepare") return 2;
  return 3;
}

/** Present-tense verb shown while a step is running. */
function progressVerb(step: ImportProgressEvent["step"]): string {
  if (step === "upload") return "Uploading";
  if (step === "prepare") return "Preparing";
  if (step === "attachments") return "Copied";
  return "Parsing";
}

/** Done-line for the attachment step, using the last live counts when present. */
function attachmentDoneDetail(
  mode: AttachmentMediaMode,
  counts: AttachmentProgressCounts | null,
  fallback: string,
): string {
  if (counts == null) return fallback;
  return formatAttachmentProgress({
    mode,
    done: counts.done,
    total: counts.total,
    bytesDone: counts.bytesDone,
    bytesTotal: counts.bytesTotal,
  });
}

/** Parse, attachment, and prepare durations from timestamps recorded during extract. */
function stageDurations(
  timing: StageTiming,
  extractFinishedAt: number,
): { parseMs: number; attachmentsMs: number; prepareMs: number } {
  const parseStart = timing.parseStartedAt ?? timing.extractStartedAt ?? extractFinishedAt;
  const attachmentsEnd = timing.attachmentsEndedAt ?? timing.prepareStartedAt ?? extractFinishedAt;
  const prepareEnd = timing.prepareEndedAt ?? extractFinishedAt;
  return {
    parseMs: Math.max(
      0,
      (timing.parseEndedAt ?? timing.attachmentsStartedAt ?? extractFinishedAt) - parseStart,
    ),
    attachmentsMs:
      timing.attachmentsStartedAt != null
        ? Math.max(0, attachmentsEnd - timing.attachmentsStartedAt)
        : 0,
    prepareMs:
      timing.prepareStartedAt != null ? Math.max(0, prepareEnd - timing.prepareStartedAt) : 0,
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
  ownerPhones: string[];
  force: boolean;
  obfuscate: boolean;
  isSbr: boolean;
  attachmentRoot: string;
  appleContacts: string;
  whatsappKey: string;
  whatsappWa: string;
  whatsappMedia: string;
  whatsappDb: string;
  whatsappBusiness: boolean;
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
  const attachmentModeRef = useRef<AttachmentMediaMode>("copy");
  const lastAttachmentProgressRef = useRef<AttachmentProgressCounts | null>(null);

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
    } else if (event.step === "attachments") {
      timingRef.current.parseEndedAt ??= now;
      timingRef.current.attachmentsStartedAt ??= now;
      lastAttachmentProgressRef.current = {
        done: event.done,
        total: event.total,
        bytesDone: event.bytes_done ?? 0,
        bytesTotal: event.bytes_total ?? 0,
      };
    } else if (event.step === "prepare") {
      timingRef.current.attachmentsEndedAt ??= now;
      timingRef.current.prepareStartedAt ??= now;
    }

    const stepIndex = stepIndexFor(event.step);
    let rawDetail = `${event.done}/${event.total}`;
    if (event.status) {
      rawDetail = `${event.done}/${event.total} (${event.status})`;
    }

    const attachments = attachmentStepCopy(attachmentModeRef.current);
    const lastAttachment = lastAttachmentProgressRef.current;
    const detail =
      event.step === "attachments"
        ? formatAttachmentProgress({
            mode: attachmentModeRef.current,
            done: event.done,
            total: event.total,
            bytesDone: event.bytes_done ?? lastAttachment?.bytesDone ?? 0,
            bytesTotal: event.bytes_total ?? lastAttachment?.bytesTotal ?? 0,
          })
        : `${progressVerb(event.step)} ${rawDetail}`;
    const done = event.total > 0 && event.done >= event.total;
    const attachmentLabel = event.step === "attachments" ? attachments.label : undefined;

    setSteps((current) =>
      current.map((step, index) => {
        if (index < stepIndex) {
          return { ...step, status: "done" };
        }
        if (index > stepIndex) return step;
        return {
          ...step,
          ...(attachmentLabel ? { label: attachmentLabel } : {}),
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
    lastAttachmentProgressRef.current = null;
    attachmentModeRef.current = form.attachmentMedia;
    setRunning(true);
    setPhase("progress");
    setSummaryView(null);
    setStagingDir(null);
    setSteps(initialSteps("active", form.attachmentMedia));

    let importSessionId: number | null = null;
    let importCompleted = false;
    let parseMs: number | null = null;
    let attachmentsMs: number | null = null;
    let prepareMs: number | null = null;
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
        current.map((step, i) => (i === 0 ? { ...step, detail: "Extracting…" } : step)),
      );

      timingRef.current.extractStartedAt = performance.now();
      const extractResult = await runTauriJob(
        () =>
          invokeExtract({
            source: form.source,
            path: form.backupPath,
            output_dir: outputDir,
            ...(isImessageMethod(form.source)
              ? imessageExtractFields({
                  source: form.source,
                  backupPassword: form.backupPassword,
                  attachmentMedia: form.attachmentMedia,
                  maxResolution: form.maxResolution,
                  maxFps: form.maxFps,
                  minSizeMb: form.minSizeMb,
                  obfuscate: form.obfuscate,
                  attachmentRoot: form.attachmentRoot,
                  appleContacts: form.appleContacts,
                })
              : {}),
            ...(isWhatsappMethod(form.source)
              ? whatsappExtractFields({
                  source: form.source,
                  attachmentMedia: form.attachmentMedia,
                  maxResolution: form.maxResolution,
                  maxFps: form.maxFps,
                  minSizeMb: form.minSizeMb,
                  key: form.whatsappKey,
                  wa: form.whatsappWa,
                  media: form.whatsappMedia,
                  db: form.whatsappDb,
                  business: form.whatsappBusiness,
                })
              : {}),
            ...(form.isSbr
              ? sbrExtractFields({
                  attachmentMedia: form.attachmentMedia,
                  maxResolution: form.maxResolution,
                  maxFps: form.maxFps,
                  minSizeMb: form.minSizeMb,
                  ownerPhones: form.ownerPhones,
                  obfuscate: form.obfuscate,
                })
              : {}),
          }),
        { onProgress: applyProgress, onIssue: recordIssue },
      );
      if (extractResult.extraction) {
        countsRef.current.filesParsed = extractResult.extraction.files_parsed;
        countsRef.current.messagesParsed = extractResult.extraction.messages_parsed;
      }

      const extractFinishedAt = performance.now();
      timingRef.current.prepareEndedAt = extractFinishedAt;
      timingRef.current.attachmentsEndedAt ??=
        timingRef.current.prepareStartedAt ?? extractFinishedAt;
      ({ parseMs, attachmentsMs, prepareMs } = stageDurations(
        timingRef.current,
        extractFinishedAt,
      ));
      const attachments = attachmentStepCopy(form.attachmentMedia);
      const attachmentDoneLine = attachmentDoneDetail(
        form.attachmentMedia,
        lastAttachmentProgressRef.current,
        attachments.doneDetail,
      );

      setSteps([
        {
          label: "Parse backup",
          status: "done",
          detail: "Extraction complete",
          durationMs: parseMs,
        },
        {
          label: attachments.label,
          status: "done",
          detail: attachmentDoneLine,
          durationMs: attachmentsMs,
        },
        {
          label: "Preparing messages",
          status: "done",
          detail: "Preparation complete",
          durationMs: prepareMs,
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
            // Extract just wrote these files. Matching size_bytes lets
            // vault-push skip a second full-file hash. Media remaps clear
            // digest and size, so a transcoded file is hashed during extract
            // and then trusted here. Applies to every desktop source, not
            // only SMS Backup & Restore.
            trust_export: true,
            contact_name_mode: form.contactNameMode,
            import_id: importSession.id,
          }),
        { onProgress: applyProgress, onIssue: recordIssue },
      );
      uploadMs = performance.now() - uploadStartedAt;
      importCompleted = true;

      setSteps((current) =>
        current.map((step, i) =>
          i === 3
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
        current.map((step) => (step.status === "active" ? { ...step, status: "error" } : step)),
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
        attachmentsMs,
        prepareMs,
        uploadMs,
        durationMs,
        issues: issuesRef.current,
      };
      const durations = [parseMs, attachmentsMs, prepareMs, uploadMs];
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
            attachments_ms: attachmentsMs,
            prepare_ms: prepareMs,
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
