import { getBaseUrl } from "../lib/api";
import type { MessageAttachment } from "../lib/types";

export default function VideoPlayer({ attachment }: { attachment: MessageAttachment }) {
  const url = attachment.sha256
    ? `${getBaseUrl()}/v1/assets/${attachment.sha256}`
    : null;
  if (!url) return null;

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
