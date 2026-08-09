import type { MessageAttachment } from "../lib/types";
import { useAssetObjectUrl } from "../hooks/useAssetObjectUrl";

export default function AttachmentThumbnail({
  attachment,
  source,
  onClick,
}: {
  attachment: MessageAttachment;
  source: string;
  onClick: () => void;
}) {
  const isVideo = attachment.mime_type?.startsWith("video/");
  const isImage = attachment.mime_type?.startsWith("image/");
  const wantsMedia = Boolean(attachment.sha256 && (isImage || isVideo));
  const { url, loading, error } = useAssetObjectUrl(
    wantsMedia ? attachment.sha256 : null,
    wantsMedia ? source : null,
  );

  // No renderable asset (missing digest) or an unknown file type — show a file chip
  if (!attachment.sha256 || (!isImage && !isVideo)) {
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

  if (error) {
    return (
      <div style={{
        display: "flex", alignItems: "center", gap: "0.5rem",
        padding: "0.5rem", background: "var(--elevated)", borderRadius: "4px",
        marginTop: "0.375rem", fontSize: "0.813rem", color: "var(--muted)",
      }}>
        <span>📎</span>
        <span>{attachment.original_name || "attachment"} (failed to load)</span>
      </div>
    );
  }

  if (loading || !url) {
    return (
      <div style={{
        marginTop: "0.375rem", maxWidth: "300px", height: "120px",
        background: "var(--elevated)", borderRadius: "6px",
        display: "flex", alignItems: "center", justifyContent: "center",
        fontSize: "0.75rem", color: "var(--muted)",
      }}>
        Loading…
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
