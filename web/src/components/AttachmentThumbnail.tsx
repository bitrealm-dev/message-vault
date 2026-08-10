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
      <div className="mt-1.5 flex items-center gap-2 rounded bg-elevated px-2 py-2 text-[0.813rem]">
        <span>📎</span>
        <span className="text-text">{attachment.original_name || "attachment"}</span>
      </div>
    );
  }

  if (error) {
    return (
      <div className="mt-1.5 flex items-center gap-2 rounded bg-elevated px-2 py-2 text-[0.813rem] text-muted">
        <span>📎</span>
        <span>{attachment.original_name || "attachment"} (failed to load)</span>
      </div>
    );
  }

  if (loading || !url) {
    return (
      <div className="mt-1.5 flex h-[120px] max-w-[300px] items-center justify-center rounded-md bg-elevated text-[0.75rem] text-muted">
        Loading…
      </div>
    );
  }

  return (
    <div
      onClick={onClick}
      className="mt-1.5 max-w-[300px] cursor-pointer overflow-hidden rounded-md border border-border"
    >
      {isImage && (
        <img
          src={url}
          alt={attachment.original_name || "attachment"}
          loading="lazy"
          className="block h-auto w-full"
        />
      )}
      {isVideo && (
        <div className="relative">
          <img
            src={url}
            alt={attachment.original_name || "attachment"}
            loading="lazy"
            className="block h-auto w-full opacity-70"
          />
          <div className="absolute inset-0 flex items-center justify-center">
            <span className="text-[2rem]">▶️</span>
          </div>
        </div>
      )}
    </div>
  );
}
