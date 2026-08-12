import type { MessageAttachment } from "./types";

export function attachmentDisplayName(attachment: MessageAttachment): string {
  if (attachment.original_name?.trim()) return attachment.original_name.trim();
  const path = attachment.path?.trim();
  if (path) {
    const parts = path.split(/[/\\]/);
    const base = parts[parts.length - 1];
    if (base) return base;
  }
  return "attachment";
}

function missingWhy(reason: string | null | undefined): string {
  if (reason === "too_large") return "missing — too large";
  if (reason === "file_missing") return "missing — file not found";
  return "missing";
}

/** Chip copy for attachments imported without bytes. */
export function missingAttachmentChipLabel(attachment: MessageAttachment): string {
  const name = attachmentDisplayName(attachment);
  const mime = attachment.mime_type?.trim();
  const why = missingWhy(attachment.missing_reason);
  if (mime) return `${name} · ${mime} (${why})`;
  return `${name} (${why})`;
}
