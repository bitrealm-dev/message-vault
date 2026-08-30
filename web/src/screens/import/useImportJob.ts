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
import { getDeviceId } from "../../lib/deviceId";
import { imessageExtractFields } from "../../lib/imessageExtractFields";
import { isImessageMethod } from "../../lib/imessageImport";
import { buildSourceFingerprint, setImportStage } from "../../lib/importSession";
import { saveImportSavedGroup } from "../../lib/savedGroups";
import { sbrExtractFields } from "../../lib/sbrExtractFields";
import { resolveImportStagingDir } from "../../lib/system-settings";
import { invokeExtract, invokePathStat, invokePush, type TauriJobResult } from "../../lib/tauri";
import { isTauri } from "../../lib/tauri-check";
import type {
  AttachmentMediaMode,
  ContactNameMode,
  ImportIssueEvent,
  ImportProgressEvent,
} from "../../lib/types";
import { importSessionCreateBody } from "../../lib/vaultSource";
import { whatsappExtractFields } from "../../lib/whatsappExtractFields";
import { isWhatsappMethod } from "../../lib/whatsappImport";
import { importOutcome } from "./importOutcome";
import {
  type AttachmentProgressCounts,
  attachmentDoneDetail,
  type ImportPhase,
  type ImportStep,
  isProgressStepComplete,
  stepIndexFor,
  stepsFor,
} from "./importProgressState";

export type { ImportPhase, ImportStep } from "./importProgressState";

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

const EMPTY_TIMING: StageTiming = {
  extractStartedAt: null,
  parseStartedAt: null,
  parseEndedAt: null,
  attachmentsStartedAt: null,
  attachmentsEndedAt: null,
  prepareStartedAt: null,
  prepareEndedAt: null,
};

/** Progress steps for this mode (Decision 8), with the first step optionally marked active. */
function initialSteps(
  status: ImportStep["status"] = "pending",
  attachmentMedia: AttachmentMediaMode = "copy",
): ImportStep[] {
  const steps = stepsFor(attachmentMedia);
  const first = steps[0];
  if (status === "active" && first) {
    steps[0] = { ...first, status, detail: "Reading backup…" };
  }
  return steps;
}

/** Present-tense verb shown while a step is running. */
function progressVerb(step: ImportProgressEvent["step"]): string {
  switch (step) {
    case "upload":
      return "Uploading";
    case "prepare":
      return "Preparing";
    case "attachments":
      return "Copied";
    case "media":
      return "Converting";
    case "parse":
      return "Reading";
    default: {
      const _exhaustive: never = step;
      return _exhaustive;
    }
  }
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

/** Pick up a session whose staging folder is already complete. */
export type ResumePush = {
  sessionId: number;
  stagingDir: string;
};

const ATTACHMENT_MEDIA_MODES: readonly AttachmentMediaMode[] = [
  "copy",
  "convert",
  "compress",
  "skip",
];
const CONTACT_NAME_MODES: readonly ContactNameMode[] = ["fill_missing", "overwrite", "as_is"];

function isStringArray(value: unknown): value is string[] {
  return Array.isArray(value) && value.every((item) => typeof item === "string");
}

/**
 * Rebuild form values from a session's stored snapshot.
 *
 * The snapshot omits `backupPassword` and `whatsappKey`, defaulted to ""
 * here: the resume path never re-runs extract, and the push only reads
 * `force`, `contactNameMode`, and `attachmentMedia`, all present in the
 * snapshot.
 *
 * The snapshot came from the database, not from this session's own state,
 * so its shape is checked field by field rather than trusted. Returns
 * null for anything that doesn't match, instead of throwing.
 */
export function restoreFormFromSnapshot(raw: unknown): ImportJobFormValues | null {
  if (typeof raw !== "object" || raw === null) return null;
  const r = raw as Record<string, unknown>;
  if (typeof r.source !== "string") return null;
  if (typeof r.backupPath !== "string") return null;
  if (
    typeof r.attachmentMedia !== "string" ||
    !ATTACHMENT_MEDIA_MODES.includes(r.attachmentMedia as AttachmentMediaMode)
  ) {
    return null;
  }
  if (typeof r.maxResolution !== "string") return null;
  if (typeof r.maxFps !== "string") return null;
  if (typeof r.minSizeMb !== "string") return null;
  if (
    typeof r.contactNameMode !== "string" ||
    !CONTACT_NAME_MODES.includes(r.contactNameMode as ContactNameMode)
  ) {
    return null;
  }
  if (!isStringArray(r.ownerPhones)) return null;
  if (typeof r.force !== "boolean") return null;
  if (typeof r.obfuscate !== "boolean") return null;
  if (typeof r.isSbr !== "boolean") return null;
  if (typeof r.attachmentRoot !== "string") return null;
  if (typeof r.appleContacts !== "string") return null;
  if (typeof r.whatsappWa !== "string") return null;
  if (typeof r.whatsappMedia !== "string") return null;
  if (typeof r.whatsappDb !== "string") return null;
  if (typeof r.whatsappBusiness !== "boolean") return null;

  return {
    source: r.source,
    backupPath: r.backupPath,
    backupPassword: "",
    attachmentMedia: r.attachmentMedia as AttachmentMediaMode,
    maxResolution: r.maxResolution,
    maxFps: r.maxFps,
    minSizeMb: r.minSizeMb,
    contactNameMode: r.contactNameMode as ContactNameMode,
    ownerPhones: r.ownerPhones,
    force: r.force,
    obfuscate: r.obfuscate,
    isSbr: r.isSbr,
    attachmentRoot: r.attachmentRoot,
    appleContacts: r.appleContacts,
    whatsappKey: "",
    whatsappWa: r.whatsappWa,
    whatsappMedia: r.whatsappMedia,
    whatsappDb: r.whatsappDb,
    whatsappBusiness: r.whatsappBusiness,
  };
}

/** Run extract then upload for one import, and keep step progress for the UI. */
export function useImportJob() {
  const { token } = useAuth();
  const { run: runTauriJob, cancel } = useTauriJob();
  const [running, setRunning] = useState(false);
  const [steps, setSteps] = useState<ImportStep[]>(() => initialSteps());
  const [phase, setPhase] = useState<ImportPhase>("form");
  const [summaryView, setSummaryView] = useState<ImportSummaryView | null>(null);
  const [stagingDir, setStagingDir] = useState<string | null>(null);
  const [importSessionId, setImportSessionId] = useState<number | null>(null);
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
    setImportSessionId(null);
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

    const stepIndex = stepIndexFor(event.step, attachmentModeRef.current);
    if (stepIndex === -1) return; // No row for this step in the current mode.

    let rawDetail = `${event.done}/${event.total}`;
    if (event.status) {
      rawDetail = `${event.done}/${event.total} (${event.status})`;
    }

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
    const done = isProgressStepComplete(event.step, event.done, event.total);

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

  /** Form snapshot for the session record, without the secrets. */
  function formSnapshot(form: ImportJobFormValues): Record<string, unknown> {
    const { backupPassword: _backupPassword, whatsappKey: _whatsappKey, ...rest } = form;
    return rest;
  }

  async function startImport(form: ImportJobFormValues, resume?: ResumePush): Promise<void> {
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
    setImportSessionId(null);
    setSteps(initialSteps("active", form.attachmentMedia));

    let sessionId: number | null = null;
    let threw = false;
    let parseMs: number | null = null;
    let attachmentsMs: number | null = null;
    let prepareMs: number | null = null;
    let uploadMs: number | null = null;
    let pushResult: TauriJobResult | null = null;
    let outputDir = "";
    try {
      const baseUrl = getBaseUrl();
      if (!token) throw new Error("Not authenticated");

      if (resume) {
        // The staging folder is already complete, so there is nothing to
        // resolve, no new session to create (the account already has this
        // one), and no extract to run.
        outputDir = resume.stagingDir;
        setStagingDir(outputDir);
        sessionId = resume.sessionId;
        setImportSessionId(sessionId);

        const resumeTemplate = stepsFor(form.attachmentMedia);
        const resumeLastIndex = resumeTemplate.length - 1;
        setSteps(
          resumeTemplate.map((step, i) =>
            i === resumeLastIndex
              ? { ...step, status: "active", detail: "Uploading to vault…" }
              : { ...step, status: "done", detail: "Already staged" },
          ),
        );
      } else {
        outputDir = await resolveImportStagingDir(form.backupPath, form.source);
        setStagingDir(outputDir);

        const backupStat = await invokePathStat(form.backupPath).catch(() => null);
        const importSession = await apiClient.post<{ id: number }>("/v1/imports", {
          ...importSessionCreateBody(form.source),
          stage: "parse",
          staging_dir: outputDir,
          device_id: getDeviceId(),
          form: formSnapshot(form),
          source_fingerprint: backupStat
            ? buildSourceFingerprint(form.backupPath, backupStat)
            : null,
        });
        sessionId = importSession.id;
        setImportSessionId(sessionId);

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
        const attachmentDoneLine = attachmentDoneDetail(
          form.attachmentMedia,
          lastAttachmentProgressRef.current,
          attachmentStepCopy(form.attachmentMedia).doneDetail,
        );
        // The staging row folds both the attachment copy and the
        // conversation-file write ("prepare") into one duration — from the
        // user's side that is all part of staging, not two separate steps.
        const stagingMs = attachmentsMs + prepareMs;

        const extractedTemplate = stepsFor(form.attachmentMedia);
        const extractedLastIndex = extractedTemplate.length - 1;
        setSteps(
          extractedTemplate.map((step, i) => {
            if (i === 0) {
              return {
                ...step,
                status: "done" as const,
                detail: "Extraction complete",
                durationMs: parseMs,
              };
            }
            if (i === 1) {
              return {
                ...step,
                status: "done" as const,
                detail: attachmentDoneLine,
                durationMs: stagingMs,
              };
            }
            if (i === extractedLastIndex) {
              return { ...step, status: "active" as const, detail: "Uploading to vault…" };
            }
            // A media step (Convert/Compress) row: not run by this job yet.
            return step;
          }),
        );
      }

      activeStepRef.current = "upload";
      if (sessionId != null) {
        // Best effort: a stale stage costs a slower resume, never a wrong
        // one — resume correctness is recomputed from the folder.
        await setImportStage(sessionId, "pushing").catch(() => {});
      }
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
            import_id: sessionId ?? undefined,
          }),
        { onProgress: applyProgress, onIssue: recordIssue },
      );
      uploadMs = performance.now() - uploadStartedAt;

      setSteps((current) =>
        current.map((step, i) =>
          i === current.length - 1
            ? { ...step, status: "done", detail: "Upload complete", durationMs: uploadMs }
            : step,
        ),
      );
    } catch (e: unknown) {
      threw = true;
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
      const outcome = importOutcome({ report: pushReport, threw, issues: issuesRef.current });
      const finalSummary: ImportSummaryView = {
        status: outcome,
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
      // Keyed by label, not index: the staging row folds attachments and
      // prepare into one duration, and a mode with no media step has fewer
      // rows than one with it — see stepsFor.
      const finalStagingMs =
        attachmentsMs != null || prepareMs != null ? (attachmentsMs ?? 0) + (prepareMs ?? 0) : null;
      const durationByLabel = new Map<string, number | null>([
        ["Read backup", parseMs],
        ["Copy to staging", finalStagingMs],
        ["Upload to vault", uploadMs],
      ]);
      setSteps((current) =>
        current.map((step) => {
          const duration = durationByLabel.get(step.label);
          if (duration == null) return step;
          return { ...step, durationMs: duration };
        }),
      );
      if (sessionId) {
        try {
          await apiClient.post(`/v1/imports/${String(sessionId)}/complete`, {
            ok: outcome !== "failed",
            status: outcome,
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
      if (sessionId != null) {
        saveImportSavedGroup({
          importSessionId: sessionId,
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
    importSessionId,
    completionText: phase === "done" ? completionTextFor(summaryView?.status) : undefined,
    startImport,
    cancel,
    returnToForm,
  };
}
