import { getBaseUrl } from "../lib/api";
import type { MessageAttachment } from "../lib/types";

export default function AttachmentThumbnail({
  attachment,
  onClick,
}: {
  attachment: MessageAttachment;
  onClick: () => void;
}) {
  const url = attachment.sha256
    ? `${getBaseUrl()}/v1/assets/${attachment.sha256}`
    : null;

  const isVideo = attachment.mime_type?.startsWith("video/");
  const isImage = attachment.mime_type?.startsWith("image/");

  // No renderable asset (missing digest) or an unknown file type — show a file chip
  if (!url || (!isImage && !isVideo)) {
    return (
      <div style={{
        display: "flex", alignItems: "center", gap: "0.5rem",
        padding: "0.5rem", background: "var(--elevated)", borderRadius: "4px",
        marginTop: "0.375rem", fontSize: "0.813rem",
      }}>
        <span>📎</span>
        <span style={{ color: "var(--text)" }}>{attachment.original_name || "attachment"}</span>
      </div>
    );
  }

  return (
    <div
      onClick={onClick}
      style={{
        marginTop: "0.375rem", cursor: "pointer",
        maxWidth: "300px", borderRadius: "6px", overflow: "hidden",
        border: "1px solid var(--border)",
      }}
    >
      {isImage && (
        <img
          src={url}
          alt={attachment.original_name || "attachment"}
          loading="lazy"
          style={{ width: "100%", height: "auto", display: "block" }}
        />
      )}
      {isVideo && (
        <div style={{ position: "relative" }}>
          <img
            src={url}
            alt={attachment.original_name || "attachment"}
            loading="lazy"
            style={{ width: "100%", height: "auto", display: "block", opacity: 0.7 }}
          />
          <div style={{
            position: "absolute", inset: 0, display: "flex",
            alignItems: "center", justifyContent: "center",
          }}>
            <span style={{ fontSize: "2rem" }}>▶️</span>
          </div>
        </div>
      )}
    </div>
  );
}
