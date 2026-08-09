import type { MessageAttachment } from "../lib/types";
import { useAssetObjectUrl } from "../hooks/useAssetObjectUrl";

export default function VideoPlayer({
  attachment,
  source,
}: {
  attachment: MessageAttachment;
  source: string;
}) {
  const { url, loading, error } = useAssetObjectUrl(attachment.sha256, source);
  if (!attachment.sha256) return null;
  if (error) {
    return (
      <div style={{ marginTop: "0.375rem", fontSize: "0.75rem", color: "var(--muted)" }}>
        Video failed to load
      </div>
    );
  }
  if (loading || !url) {
    return (
      <div style={{
        marginTop: "0.375rem", maxWidth: "400px", height: "160px",
        background: "var(--elevated)", borderRadius: "6px",
        display: "flex", alignItems: "center", justifyContent: "center",
        fontSize: "0.75rem", color: "var(--muted)",
      }}>
        Loading video…
      </div>
    );
  }

  return (
    <div style={{ marginTop: "0.375rem", maxWidth: "400px" }}>
      <video
        controls
        preload="metadata"
        style={{ width: "100%", borderRadius: "6px" }}
      >
        <source src={url} type={attachment.mime_type || undefined} />
      </video>
    </div>
  );
}
