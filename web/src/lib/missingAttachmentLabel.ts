import type { MessageAttachment } from "./types";

/** File name shown on a missing-attachment chip. */
function attachmentDisplayName(attachment: MessageAttachment): string {
  if (attachment.original_name?.trim()) return attachment.original_name.trim();
  const path = attachment.path?.trim();
  if (path) {
    const parts = path.split(/[/\\]/);
    const base = parts[parts.length - 1];
    if (base) return base;
  }
  return "attachment";
}

/** Short reason shown in parentheses on a missing-attachment chip. */
function missingWhy(reason: string | null | undefined): string {
  if (!reason || reason === "no_path") return "missing";
  if (reason === "too_large") return "missing — too large";
  if (reason === "file_missing") return "missing — file not found";
  // Chosen on import ("Do not copy"), so the file is absent by request, not
  // lost. Writers say "not_copied"; older exports stored "skipped" (shared
  // exporters) or "embed_disabled" (iMessage).
  if (reason === "not_copied" || reason === "skipped" || reason === "embed_disabled") {
    return "skipped";
  }
  if (reason.startsWith("convert_failed: ")) {
    return `could not be converted — ${reason.slice("convert_failed: ".length)}`;
  }
  if (reason.startsWith("unknown: ")) {
    return `could not be imported — ${reason.slice("unknown: ".length)}`;
  }
  // Keep an unrecognized reason visible and reportable, never uniform.
  return `missing — ${reason}`;
}

/** Label for an attachment that was imported without the file bytes. */
export function missingAttachmentChipLabel(attachment: MessageAttachment): string {
  const name = attachmentDisplayName(attachment);
  const mime = attachment.mime_type?.trim();
  const why = missingWhy(attachment.missing_reason);
  if (mime) return `${name} · ${mime} (${why})`;
  return `${name} (${why})`;
}
