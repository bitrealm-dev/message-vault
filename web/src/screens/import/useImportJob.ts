import { useRef, useState } from "react";
import {
  completionTextFor,
  type ImportIssue,
  type ImportSummaryView,
} from "../../components/import/ImportSummaryPanel";
import { useTauriJob } from "../../hooks/useTauriJob";
import { apiClient, getBaseUrl } from "../../lib/api";
import { formatAttachmentProgress } from "../../lib/attachmentProgressCopy";
import { useAuth } from "../../lib/auth";
import { getDeviceId } from "../../lib/deviceId";
import { imessageExtractFields } from "../../lib/imessageExtractFields";
import { isImessageMethod } from "../../lib/imessageImport";
import {
  buildSourceFingerprint,
  discardImportSession,
  type ImportStage,
  setImportStage,
} from "../../lib/importSession";
import { saveImportSavedGroup } from "../../lib/savedGroups";
import { mediaExtractFields, sbrExtractFields } from "../../lib/sbrExtractFields";
import { resolveImportStagingDir } from "../../lib/system-settings";
import {
  invokeDeleteStaging,
  invokeExtract,
  invokePathStat,
  invokePush,
  invokeSummarizeStaging,
  invokeTranscodeStaging,
  type PushFinishedReport,
  probeFfmpegTools,
  type StagingConfig,
  type StagingSummary,
  type TauriJobResult,
  type TranscodeFinishedReport,
} from "../../lib/tauri";
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
import { gateDelta as computeGateDelta, type GateDelta } from "./gateDelta";
import { mediaJobVerb } from "./gateForecast";
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

/** Parse/attachments/prepare durations, fixed once extract finishes and read again at finish time. */
type ExtractDurations = {
  parseMs: number | null;
  attachmentsMs: number | null;
  prepareMs: number | null;
};

const EMPTY_DURATIONS: ExtractDurations = { parseMs: null, attachmentsMs: null, prepareMs: null };

/** Present-tense verb for the media step, following the mode so compress mode never says "Converting". */
function mediaVerb(mode: AttachmentMediaMode): string {
  return mode === "compress" ? "Compressing" : "Converting";
}

/** Sentence shown on the media step's row once the pass finishes. */
function mediaDoneDetail(mode: AttachmentMediaMode): string {
  return mode === "compress" ? "Compression complete" : "Conversion complete";
}

/**
 * True for the "canceled"/"cancelled" text a cancelled Tauri job's
 * `extract:error` carries. The media pass's own cancellation is spelled
 * "canceled" (one L, `transcode.rs`'s `check_cancel_now`); other layers of
 * the Rust side spell it "cancelled" (two L, `message-vault-io-core`'s
 * `check_cancel`) — matched case- and spelling-insensitively so this reads
 * either.
 */
function isCancellation(message: string): boolean {
  return /^cancell?ed$/i.test(message.trim());
}

/**
 * Extract stages originals regardless of the chosen media mode (ffmpeg is
 * only required once Gate 1 is approved, not up front) — convert and
 * compress run afterward, against the staged folder, via
 * `invokeTranscodeStaging`. Copy and skip pass through unchanged.
 */
function extractAttachmentMedia(mode: AttachmentMediaMode): AttachmentMediaMode {
  return mode === "convert" || mode === "compress" ? "copy" : mode;
}

/** The media fields `summarize_staging` and `transcode_staging` share, read from the submitted form. */
function stagingMediaFields(
  form: Pick<ImportJobFormValues, "attachmentMedia" | "maxResolution" | "maxFps" | "minSizeMb">,
): Pick<
  StagingConfig,
  "attachment_media" | "media_max_resolution" | "media_max_fps" | "media_min_size"
> {
  return mediaExtractFields({
    attachmentMedia: form.attachmentMedia,
    maxResolution: form.maxResolution,
    maxFps: form.maxFps,
    minSizeMb: form.minSizeMb,
  });
}

/** Present-tense verb for every step but `media` (which needs the mode —
 * see `mediaVerb`), keyed by step name so a step added to the wire union
 * without an entry here is a compile error rather than a silent fallback.
 */
const STEP_VERB: Record<Exclude<ImportProgressEvent["step"], "media">, string> = {
  parse: "Reading",
  attachments: "Copied",
  prepare: "Preparing",
  upload: "Uploading",
};

/**
 * Present-tense verb shown while a step is running. Falls back to a plain
 * verb for a step string this build doesn't recognise — the event comes
 * off the wire unvalidated.
 */
function progressVerb(step: ImportProgressEvent["step"], mode: AttachmentMediaMode): string {
  if (step === "media") return mediaVerb(mode);
  return STEP_VERB[step] ?? "Working";
}

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
  const [gateSummary, setGateSummary] = useState<StagingSummary | null>(null);
  const [gateDeltaState, setGateDeltaState] = useState<GateDelta | null>(null);
  const [mediaToolsMissing, setMediaToolsMissing] = useState(false);
  // True only while a not-cancellable summarize call is in flight (Decision:
  // the gate screens render once the summary resolves; until then the
  // progress view stays up with its Cancel disabled, since there is nothing
  // for it to stop).
  const [computingSummary, setComputingSummary] = useState(false);
  const activeStepRef = useRef<ImportIssue["step"]>("parse");
  const issuesRef = useRef<ImportIssue[]>([]);
  const countsRef = useRef<{
    filesParsed?: number;
    messagesParsed?: number;
  }>({});
  const timingRef = useRef({ ...EMPTY_TIMING });
  const durationsRef = useRef<ExtractDurations>({ ...EMPTY_DURATIONS });
  const importStartedAtRef = useRef(0);
  const formRef = useRef<ImportJobFormValues | null>(null);
  const attachmentModeRef = useRef<AttachmentMediaMode>("copy");
  // What extract is actually doing to attachments right now — "copy" under
  // convert/compress too, since extract only stages originals (ruling 3);
  // the media step (index 2), not this row, tells the convert/compress
  // story. Kept separate from attachmentModeRef, which stays the mode the
  // user chose and drives the media step's own wording and the step-list
  // layout.
  const extractMediaModeRef = useRef<AttachmentMediaMode>("copy");
  const lastAttachmentProgressRef = useRef<AttachmentProgressCounts | null>(null);
  // Guards approveGate/declineGate against a double click doing the work
  // twice — the same in-flight-ref pattern ImportScreen.tsx uses for its
  // resume actions.
  const gateActionRef = useRef(false);

  function returnToForm(): void {
    setPhase("form");
    setSummaryView(null);
    setStagingDir(null);
    setImportSessionId(null);
    setGateSummary(null);
    setGateDeltaState(null);
    setMediaToolsMissing(false);
    setComputingSummary(false);
  }

  function applyProgress(event: ImportProgressEvent): void {
    const now = performance.now();

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
    // No row for this step in the current mode (or an unrecognised step off
    // the wire) — leave activeStepRef pointing at whatever step actually has
    // a row, so a dropped event here never mislabels the next error.
    if (stepIndex < 0) return;
    activeStepRef.current = event.step;

    let rawDetail = `${event.done}/${event.total}`;
    if (event.status) {
      rawDetail = `${event.done}/${event.total} (${event.status})`;
    }

    const lastAttachment = lastAttachmentProgressRef.current;
    const detail =
      event.step === "attachments"
        ? formatAttachmentProgress({
            mode: extractMediaModeRef.current,
            done: event.done,
            total: event.total,
            bytesDone: event.bytes_done ?? lastAttachment?.bytesDone ?? 0,
            bytesTotal: event.bytes_total ?? lastAttachment?.bytesTotal ?? 0,
          })
        : `${progressVerb(event.step, attachmentModeRef.current)} ${rawDetail}`;
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

  /**
   * Move a live session to another stage, carrying the summary the user just
   * approved when there is one — `approvedPlan` is simply forwarded, undefined
   * and all. `setImportStage` posts `{ stage, summary: approvedPlan }`, and
   * `JSON.stringify` drops an `undefined`-valued property outright, so an
   * omitted plan and an explicit `undefined` reach the server identically:
   * no `summary` key at all, leaving whatever plan is already stored
   * untouched.
   */
  async function moveStage(
    sessionId: number,
    stage: ImportStage,
    approvedPlan?: StagingSummary,
  ): Promise<void> {
    await setImportStage(sessionId, stage, approvedPlan).catch(() => {});
  }

  /**
   * Build the finished-import summary, record it, and — usually — post
   * `/complete`, the terminal step for every path except one (a failure
   * before either gate, a failed media pass, or a push that ran to
   * completion or failed all complete normally).
   *
   * `canceled` overrides `importOutcome`'s verdict outright: the user asked
   * for this, so it is never read as a failure.
   *
   * `skipComplete` is that one exception: decision 36 routes a cancellation
   * mid-`transcode` to the same recovery as a crash at that stage, and
   * decision 37 says only an explicit discard ends a waiting session —
   * `/complete` is what ends one. Posting it here would free the
   * one-active-session slot and drop the session out of
   * `GET /v1/imports/active`, stranding the staged folder (and the time
   * already spent on it) with no session left to resume it through. The
   * caller sets this only for a cancelled *media pass*; a cancelled
   * extract has nothing approved yet, so the spec sends it to restart
   * regardless and it completes normally, same as a failed pass does (a
   * broken ffmpeg must not lock the account out of importing — the failed
   * run still frees the slot).
   */
  async function finishImport(args: {
    sessionId: number | null;
    form: ImportJobFormValues;
    threw: boolean;
    canceled?: boolean;
    pushReport: PushFinishedReport | null;
    uploadMs: number | null;
    skipComplete?: boolean;
    // The plan the user approved at their last gate — Gate 2's recomputed
    // summary when there was a media pass, Gate 1's otherwise (Decision 15).
    // Only `runPush` has one to offer; every other call into this function
    // ends in `pushReport: null`, which fails the outcome regardless of
    // `approved`, so leaving it undefined there is a no-op, not a gap.
    approved?: StagingSummary;
  }): Promise<void> {
    const { sessionId, form, threw, canceled, pushReport, uploadMs, skipComplete, approved } = args;
    const { parseMs, attachmentsMs, prepareMs } = durationsRef.current;
    const durationMs = performance.now() - importStartedAtRef.current;
    const outcome: ImportSummaryView["status"] = canceled
      ? "canceled"
      : importOutcome({
          report: pushReport ?? undefined,
          threw,
          issues: issuesRef.current,
          approved,
        });
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
    if (sessionId && !skipComplete) {
      try {
        await apiClient.post(`/v1/imports/${String(sessionId)}/complete`, {
          ok: outcome !== "failed" && outcome !== "canceled",
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

  /**
   * Upload to the vault and record the outcome — the tail end shared by a
   * resumed session (jumps straight here), Gate 1's approval when there is
   * no media step, and Gate 2's approval. Never throws: a push failure is
   * folded into the finished summary via `finishImport`, exactly like any
   * other terminal outcome.
   */
  async function runPush(
    form: ImportJobFormValues,
    sessionId: number,
    outputDir: string,
    approvedPlan?: StagingSummary,
  ): Promise<void> {
    setRunning(true);
    setPhase("progress");
    activeStepRef.current = "upload";
    setSteps((current) =>
      current.map((step, i) =>
        i === current.length - 1
          ? { ...step, status: "active", detail: "Uploading to vault…" }
          : step,
      ),
    );
    await moveStage(sessionId, "pushing", approvedPlan);

    const uploadStartedAt = performance.now();
    let pushResult: TauriJobResult | null = null;
    let threw = false;
    try {
      const baseUrl = getBaseUrl();
      if (!token) throw new Error("Not authenticated");
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
            // Extract (or the media pass) just wrote these files. Matching
            // size_bytes lets vault-push skip a second full-file hash.
            trust_export: true,
            contact_name_mode: form.contactNameMode,
            import_id: sessionId,
          }),
        { onProgress: applyProgress, onIssue: recordIssue },
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
    }
    const uploadMs = performance.now() - uploadStartedAt;
    if (!threw) {
      setSteps((current) =>
        current.map((step, i) =>
          i === current.length - 1
            ? { ...step, status: "done", detail: "Upload complete", durationMs: uploadMs }
            : step,
        ),
      );
    }

    await finishImport({
      sessionId,
      form,
      threw,
      pushReport: pushResult?.report ?? null,
      uploadMs,
      approved: approvedPlan,
    });
  }

  /**
   * Convert or compress the staged files after Gate 1 approves them, then
   * recompute the summary against the folder as it now stands (Decision 39:
   * the folder is the truth, not the last estimate) and move on to Gate 2.
   * A failed pass ends the import the same way a failed push does — never a
   * silent fall-through to upload.
   */
  async function runMediaPass(
    form: ImportJobFormValues,
    sessionId: number,
    outputDir: string,
    approvedSummary: StagingSummary,
  ): Promise<void> {
    setRunning(true);
    setPhase("progress");
    const mediaIndex = stepIndexFor("media", form.attachmentMedia);
    activeStepRef.current = "media";
    setSteps((current) =>
      current.map((step, i) =>
        i === mediaIndex
          ? { ...step, status: "active", detail: `${mediaVerb(form.attachmentMedia)}…` }
          : step,
      ),
    );

    // Carries the plan approved at Gate 1 even on this stage — a crash
    // mid-pass must not leave `summary_json` null with no baseline for a
    // later resume to diff against.
    await moveStage(sessionId, "transcode", approvedSummary);

    const mediaStartedAt = performance.now();
    let transcodeReport: TranscodeFinishedReport | undefined;
    let threw = false;
    let canceled = false;
    try {
      const result = await runTauriJob(
        () =>
          invokeTranscodeStaging({
            staging_dir: outputDir,
            ...stagingMediaFields(form),
          }),
        { onProgress: applyProgress, onIssue: recordIssue },
      );
      transcodeReport = result.transcode;
    } catch (e: unknown) {
      const msg = e instanceof Error ? e.message : String(e);
      if (isCancellation(msg)) {
        // The user asked for this — not an error, so no issue row for it.
        canceled = true;
      } else {
        threw = true;
        issuesRef.current = [
          ...issuesRef.current,
          { kind: "error", step: activeStepRef.current, item: "Import", reason: msg },
        ];
      }
    }
    const mediaMs = performance.now() - mediaStartedAt;

    if (threw || canceled) {
      setSteps((current) =>
        current.map((step) => (step.status === "active" ? { ...step, status: "error" } : step)),
      );
      // Neither path writes another stage: the session stays at `transcode`,
      // which is exactly where the run actually got to. A cancellation also
      // skips `/complete` outright — see finishImport's doc comment — so the
      // session stays `running` and resumable instead of completing and
      // freeing the slot out from under a staged folder nobody can reach
      // anymore. A failed pass still completes normally: the account must
      // not be locked out of importing by a broken ffmpeg.
      await finishImport({
        sessionId,
        form,
        threw,
        canceled,
        pushReport: null,
        uploadMs: null,
        skipComplete: canceled,
      });
      return;
    }

    setSteps((current) =>
      current.map((step, i) =>
        i === mediaIndex
          ? {
              ...step,
              status: "done",
              detail: mediaDoneDetail(form.attachmentMedia),
              durationMs: mediaMs,
            }
          : step,
      ),
    );

    setComputingSummary(true);
    try {
      const actual = await invokeSummarizeStaging({
        staging_dir: outputDir,
        ...stagingMediaFields(form),
      });
      const delta = computeGateDelta(approvedSummary, actual, transcodeReport);
      setGateSummary(actual);
      setGateDeltaState(delta);
      await moveStage(sessionId, "awaiting_gate_2", approvedSummary);
      setComputingSummary(false);
      setRunning(false);
      setPhase("gate_2");
    } catch (e: unknown) {
      // The pass itself succeeded; only the recompute after it failed. Still
      // a failed import, not an unhandled rejection on a frozen gate screen
      // — and still no later stage written, so the session stays at
      // `transcode`.
      const msg = e instanceof Error ? e.message : String(e);
      issuesRef.current = [
        ...issuesRef.current,
        { kind: "error", step: "media", item: "Import", reason: msg },
      ];
      setComputingSummary(false);
      await finishImport({ sessionId, form, threw: true, pushReport: null, uploadMs: null });
    }
  }

  async function startImport(form: ImportJobFormValues, resume?: ResumePush): Promise<void> {
    if (!isTauri()) return;
    importStartedAtRef.current = performance.now();
    activeStepRef.current = "parse";
    issuesRef.current = [];
    countsRef.current = {};
    timingRef.current = { ...EMPTY_TIMING };
    durationsRef.current = { ...EMPTY_DURATIONS };
    lastAttachmentProgressRef.current = null;
    attachmentModeRef.current = form.attachmentMedia;
    extractMediaModeRef.current = extractAttachmentMedia(form.attachmentMedia);
    formRef.current = form;
    setRunning(true);
    setPhase("progress");
    setSummaryView(null);
    setStagingDir(null);
    setImportSessionId(null);
    setGateSummary(null);
    setGateDeltaState(null);
    setMediaToolsMissing(false);
    setComputingSummary(false);
    setSteps(initialSteps("active", form.attachmentMedia));

    let sessionId: number | null = null;

    try {
      if (!token) throw new Error("Not authenticated");

      if (resume) {
        // The staging folder is already complete, so there is nothing to
        // resolve, no new session to create (the account already has this
        // one), no extract to run, and — because nothing was ever gated —
        // nothing approved to carry forward. Straight to the push.
        const outputDir = resume.stagingDir;
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

        await runPush(form, sessionId, outputDir);
        return;
      }

      const outputDir = await resolveImportStagingDir(form.backupPath, form.source);
      setStagingDir(outputDir);

      const backupStat = await invokePathStat(form.backupPath).catch(() => null);
      const importSession = await apiClient.post<{ id: number }>("/v1/imports", {
        ...importSessionCreateBody(form.source),
        stage: "parse",
        staging_dir: outputDir,
        device_id: getDeviceId(),
        form: formSnapshot(form),
        source_fingerprint: backupStat ? buildSourceFingerprint(form.backupPath, backupStat) : null,
      });
      sessionId = importSession.id;
      setImportSessionId(sessionId);

      setSteps((current) =>
        current.map((step, i) => (i === 0 ? { ...step, detail: "Extracting…" } : step)),
      );

      await moveStage(sessionId, "write");

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
                  attachmentMedia: extractAttachmentMedia(form.attachmentMedia),
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
                  attachmentMedia: extractAttachmentMedia(form.attachmentMedia),
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
                  attachmentMedia: extractAttachmentMedia(form.attachmentMedia),
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
      const { parseMs, attachmentsMs, prepareMs } = stageDurations(
        timingRef.current,
        extractFinishedAt,
      );
      durationsRef.current = { parseMs, attachmentsMs, prepareMs };
      // What extract actually did ("Copied", not "Converted", under
      // convert/compress too) — see extractMediaModeRef's comment.
      const attachmentDoneLine = attachmentDoneDetail(
        extractAttachmentMedia(form.attachmentMedia),
        lastAttachmentProgressRef.current,
      );
      // The staging row folds both the attachment copy and the
      // conversation-file write ("prepare") into one duration — from the
      // user's side that is all part of staging, not two separate steps.
      const stagingMs = attachmentsMs + prepareMs;

      const extractedTemplate = stepsFor(form.attachmentMedia);
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
          // Media (Convert/Compress) and Upload rows: not run yet — Gate 1
          // has to approve staging first.
          return step;
        }),
      );

      setComputingSummary(true);
      await moveStage(sessionId, "awaiting_gate_1");
      const summary = await invokeSummarizeStaging({
        staging_dir: outputDir,
        ...stagingMediaFields(form),
      });

      let toolsMissing = false;
      if (mediaJobVerb(form.attachmentMedia) !== null) {
        try {
          const probe = await probeFfmpegTools(null);
          toolsMissing = !probe.ok;
        } catch {
          toolsMissing = true;
        }
      }

      setGateSummary(summary);
      setMediaToolsMissing(toolsMissing);
      setComputingSummary(false);
      setRunning(false);
      setPhase("gate_1");
    } catch (e: unknown) {
      const msg = e instanceof Error ? e.message : String(e);
      issuesRef.current = [
        ...issuesRef.current,
        { kind: "error", step: activeStepRef.current, item: "Import", reason: msg },
      ];
      setSteps((current) =>
        current.map((step) => (step.status === "active" ? { ...step, status: "error" } : step)),
      );
      setComputingSummary(false);
      await finishImport({ sessionId, form, threw: true, pushReport: null, uploadMs: null });
    }
  }

  async function approveGate(): Promise<void> {
    if (!isTauri()) return;
    if (gateActionRef.current) return;
    const form = formRef.current;
    const sessionId = importSessionId;
    const outputDir = stagingDir;
    const approvedSummary = gateSummary;
    if (!form || sessionId == null || outputDir == null || approvedSummary == null) return;

    gateActionRef.current = true;
    try {
      if (phase === "gate_1" && mediaJobVerb(form.attachmentMedia) !== null) {
        await runMediaPass(form, sessionId, outputDir, approvedSummary);
      } else {
        await runPush(form, sessionId, outputDir, approvedSummary);
      }
    } finally {
      gateActionRef.current = false;
    }
  }

  async function declineGate(): Promise<void> {
    if (gateActionRef.current) return;
    gateActionRef.current = true;
    try {
      const sessionId = importSessionId;
      const outputDir = stagingDir;
      // Both halves run regardless of the other's outcome: a live session
      // with no folder blocks the next import, and a folder with no session
      // is litter nothing will ever clean up.
      await Promise.allSettled([
        sessionId != null ? discardImportSession(sessionId) : Promise.resolve(),
        outputDir != null ? invokeDeleteStaging({ staging_dir: outputDir }) : Promise.resolve(),
      ]);
    } finally {
      gateActionRef.current = false;
    }
    returnToForm();
  }

  return {
    phase,
    steps,
    running,
    summaryView,
    stagingDir,
    importSessionId,
    gateSummary,
    gateDelta: gateDeltaState,
    // The mode the gate screens actually approved, not whatever the (hidden,
    // and in practice unchanged) form fields currently hold — read from the
    // submitted form so the gates never depend on live form state.
    gateAttachmentMedia: formRef.current?.attachmentMedia ?? "copy",
    mediaToolsMissing,
    computingSummary,
    completionText: phase === "done" ? completionTextFor(summaryView?.status) : undefined,
    startImport,
    approveGate,
    declineGate,
    cancel,
    returnToForm,
  };
}
