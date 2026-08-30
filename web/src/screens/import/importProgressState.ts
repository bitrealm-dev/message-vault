import { formatAttachmentProgress } from "../../lib/attachmentProgressCopy";
import type { AttachmentMediaMode, ImportProgressEvent } from "../../lib/types";

export type ImportStep = {
  label: string;
  status: "pending" | "active" | "done" | "error";
  detail?: string;
  durationMs?: number | null;
};

export type ImportPhase = "form" | "progress" | "done";

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
 * Index of the progress step that matches this server event, in this mode's
 * step list. Writing conversation files (`prepare`) lands on the staging
 * step, not a row of its own. Returns -1 for a step with no row in this
 * mode (`media` under copy/skip) — callers must treat that as "no row to
 * update", not index with it.
 */
export function stepIndexFor(step: ImportProgressEvent["step"], mode: AttachmentMediaMode): number {
  switch (step) {
    case "parse":
      return 0;
    case "attachments":
    case "prepare":
      return 1;
    case "media":
      return hasMediaStep(mode) ? 2 : -1;
    case "upload":
      return hasMediaStep(mode) ? 3 : 2;
    default: {
      const _exhaustive: never = step;
      return _exhaustive;
    }
  }
}

/**
 * The heading for wherever the import currently is: the present-tense form
 * of the active step's label, falling back to the first step when nothing
 * is active yet (one render frame before the first event arrives), or a
 * fixed heading once the import is done.
 */
export function progressHeading(steps: ImportStep[], phase: ImportPhase): string {
  if (phase === "done") return DONE_HEADING;
  const current = steps.find((step) => step.status === "active") ?? steps[0];
  if (!current) return DONE_HEADING;
  return HEADING_BY_LABEL.get(current.label) ?? current.label;
}

/** Whether a progress event should mark its step done. Attachments stay active until prepare. */
export function isProgressStepComplete(
  step: ImportProgressEvent["step"],
  done: number,
  total: number,
): boolean {
  if (step === "attachments") return false;
  return total > 0 && done >= total;
}

/** Done-line for the attachment step, using the last live counts when present. */
export function attachmentDoneDetail(
  mode: AttachmentMediaMode,
  counts: AttachmentProgressCounts | null,
  _fallback: string,
): string {
  return formatAttachmentProgress({
    mode,
    done: counts?.done ?? 0,
    total: counts?.total ?? 0,
    bytesDone: counts?.bytesDone ?? 0,
    bytesTotal: counts?.bytesTotal ?? 0,
  });
}
