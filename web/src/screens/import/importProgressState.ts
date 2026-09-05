import { formatAttachmentProgress } from "../../lib/attachmentProgressCopy";
import type { AttachmentMediaMode, ImportProgressEvent } from "../../lib/types";

export type ImportStep = {
  label: string;
  status: "pending" | "active" | "done" | "error";
  detail?: string;
  durationMs?: number | null;
};

export type ImportPhase = "form" | "identity_stop" | "progress" | "gate_1" | "gate_2" | "done";

export type AttachmentProgressCounts = {
  done: number;
  total: number;
  bytesDone: number;
  bytesTotal: number;
};

/** A step's label and its present-tense heading, declared together so the two cannot drift. */
type StepDescriptor = { label: string; heading: string };

const READ_BACKUP: StepDescriptor = { label: "Read backup", heading: "Reading your backup" };
const COPY_TO_STAGING: StepDescriptor = {
  label: "Copy to staging",
  heading: "Copying to staging",
};
const CONVERT_MEDIA: StepDescriptor = { label: "Convert media", heading: "Converting media" };
const COMPRESS_MEDIA: StepDescriptor = {
  label: "Compress media",
  heading: "Compressing media",
};
const UPLOAD_TO_VAULT: StepDescriptor = {
  label: "Upload to vault",
  heading: "Uploading to your vault",
};

/** Heading shown once the import has finished, regardless of outcome. */
const DONE_HEADING = "Import finished";

/** Present-tense heading for every step label this screen can show. */
const HEADING_BY_LABEL = new Map(
  [READ_BACKUP, COPY_TO_STAGING, CONVERT_MEDIA, COMPRESS_MEDIA, UPLOAD_TO_VAULT].map((d) => [
    d.label,
    d.heading,
  ]),
);

/** Whether this attachment media mode runs a separate media (convert/compress) step. */
function hasMediaStep(mode: AttachmentMediaMode): boolean {
  return mode === "convert" || mode === "compress";
}

/** The media step's descriptor, or null when this mode has no media step. */
function mediaStepDescriptor(mode: AttachmentMediaMode): StepDescriptor | null {
  if (mode === "convert") return CONVERT_MEDIA;
  if (mode === "compress") return COMPRESS_MEDIA;
  return null;
}

/**
 * The progress steps shown for this attachment media mode (Decision 8): four
 * under convert/compress, three under copy/skip. There is no media step in
 * copy/skip, so a greyed-out row would be promising work that will never run.
 */
export function stepsFor(mode: AttachmentMediaMode): ImportStep[] {
  const media = mediaStepDescriptor(mode);
  const descriptors = [READ_BACKUP, COPY_TO_STAGING, ...(media ? [media] : []), UPLOAD_TO_VAULT];
  return descriptors.map((d) => ({ label: d.label, status: "pending" }));
}

/**
 * Row index for every step this build knows, keyed by the step name — a
 * `Record` over `ImportProgressEvent["step"]` so a step added to that union
 * without a row here is a compile error, not a silent runtime fallback.
 */
const STEP_ROW_INDEX: Record<ImportProgressEvent["step"], (mode: AttachmentMediaMode) => number> = {
  // Setup steps (decrypting a backup, caching tables) are part of reading
  // the backup, so they narrate the first row rather than one of their own.
  setup: () => 0,
  parse: () => 0,
  // Writing conversation files ("prepare") lands on the staging step, not
  // a row of its own.
  attachments: () => 1,
  prepare: () => 1,
  media: (mode) => (hasMediaStep(mode) ? 2 : -1),
  upload: (mode) => (hasMediaStep(mode) ? 3 : 2),
};

/**
 * Index of the progress step that matches this server event, in this mode's
 * step list. Returns -1 for a step with no row in this mode (`media` under
 * copy/skip), or for a step string this build does not recognise at all —
 * the event comes off the wire unvalidated, so a lookup miss must resolve
 * to "no row", never `undefined`. Callers must treat -1 as "no row to
 * update", not index with it.
 */
export function stepIndexFor(step: ImportProgressEvent["step"], mode: AttachmentMediaMode): number {
  return STEP_ROW_INDEX[step]?.(mode) ?? -1;
}

/**
 * The heading for wherever the import currently is: the present-tense form
 * of the active step's label, falling back to the first step when nothing
 * is active yet (one render frame before the first event arrives), or a
 * fixed heading once the import is done.
 *
 * An empty step list is unreachable in practice (`stepsFor` never returns
 * one), but it must not read as "Import finished" if it ever happens —
 * that would claim a completion that never occurred.
 */
export function progressHeading(steps: ImportStep[], phase: ImportPhase): string {
  if (phase === "done") return DONE_HEADING;
  const current = steps.find((step) => step.status === "active") ?? steps[0];
  if (!current) return "";
  return HEADING_BY_LABEL.get(current.label) ?? current.label;
}

/**
 * Whether a progress event should mark its step done. Attachments stay active
 * until prepare, and a setup event never completes the read row: its counts
 * are "step 5 of 5", and the messages are still to be read once that lands.
 */
export function isProgressStepComplete(
  step: ImportProgressEvent["step"],
  done: number,
  total: number,
): boolean {
  if (step === "attachments" || step === "setup") return false;
  return total > 0 && done >= total;
}

/**
 * Detail line for a setup event: the step's label with its position, so a
 * long decrypt reads as "Deriving backup keys (1/5)" rather than a frozen
 * "Reading backup…".
 */
export function setupDetail(event: Pick<ImportProgressEvent, "done" | "total" | "status">): string {
  const label = event.status ?? "Preparing";
  return event.total > 0 ? `${label} (${event.done}/${event.total})` : label;
}

/** Done-line for the attachment step, using the last live counts when present. */
export function attachmentDoneDetail(
  mode: AttachmentMediaMode,
  counts: AttachmentProgressCounts | null,
): string {
  return formatAttachmentProgress({
    mode,
    done: counts?.done ?? 0,
    total: counts?.total ?? 0,
    bytesDone: counts?.bytesDone ?? 0,
    bytesTotal: counts?.bytesTotal ?? 0,
  });
}
