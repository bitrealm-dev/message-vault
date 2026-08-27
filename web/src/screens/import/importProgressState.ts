import { formatAttachmentProgress } from "../../lib/attachmentProgressCopy";
import type { AttachmentMediaMode, ImportProgressEvent } from "../../lib/types";

export type AttachmentProgressCounts = {
  done: number;
  total: number;
  bytesDone: number;
  bytesTotal: number;
};

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
