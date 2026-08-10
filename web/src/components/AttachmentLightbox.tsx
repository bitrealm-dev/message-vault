import {
  Dialog,
  Modal,
  ModalOverlay,
} from "react-aria-components";
import type { MessageAttachment } from "../lib/types";
import { useAssetObjectUrl } from "../hooks/useAssetObjectUrl";

export type LightboxItem = {
  attachment: MessageAttachment;
  source: string;
};

export default function AttachmentLightbox({
  items,
  currentIndex,
  onClose,
  onPrev,
  onNext,
}: {
  items: LightboxItem[];
  currentIndex: number;
  onClose: () => void;
  onPrev: () => void;
  onNext: () => void;
}) {
  const item = items[currentIndex];
  const attachment = item?.attachment;
  const { url, loading, error } = useAssetObjectUrl(
    attachment?.sha256,
    item?.source,
  );

  if (!attachment) return null;

  return (
    <ModalOverlay
      isOpen
      onOpenChange={() => onClose()}
      isDismissable
      className="fixed inset-0 z-[200] flex items-center justify-center bg-[rgba(0,0,0,0.9)]"
    >
      <Modal className="flex min-h-0 w-full items-center justify-center outline-none">
        <Dialog
          aria-label="Attachment viewer"
          className="flex items-center justify-center outline-none"
        >
          <div
            className="flex items-center justify-center outline-none"
            onKeyDown={(e) => {
              if (e.key === "ArrowLeft") onPrev();
              else if (e.key === "ArrowRight") onNext();
            }}
          >
          {items.length > 1 && (
            <button
              type="button"
              onClick={onPrev}
              aria-label="Previous attachment"
              className="absolute left-4 top-1/2 flex h-12 w-12 -translate-y-1/2 cursor-pointer items-center justify-center rounded-full border-none bg-[rgba(255,255,255,0.2)] text-[2rem] text-white"
            >
              ‹
            </button>
          )}

          {error ? (
            <div className="text-[0.875rem] text-white">
              Failed to load attachment
            </div>
          ) : loading || !url ? (
            <div className="text-[0.875rem] text-white">Loading…</div>
          ) : (
            <img
              src={url}
              alt={attachment.original_name || "attachment"}
              className="max-h-[90vh] max-w-[90vw] object-contain"
            />
          )}

          {items.length > 1 && (
            <button
              type="button"
              onClick={onNext}
              aria-label="Next attachment"
              className="absolute right-4 top-1/2 flex h-12 w-12 -translate-y-1/2 cursor-pointer items-center justify-center rounded-full border-none bg-[rgba(255,255,255,0.2)] text-[2rem] text-white"
            >
              ›
            </button>
          )}

          <div className="absolute right-4 top-4 flex items-center gap-4">
            <span className="text-[0.875rem] text-white">
              {currentIndex + 1} / {items.length}
            </span>
            <button
              type="button"
              onClick={onClose}
              aria-label="Close attachment viewer"
              className="flex h-10 w-10 cursor-pointer items-center justify-center rounded-full border-none bg-[rgba(255,255,255,0.2)] text-[1.5rem] text-white"
            >
              ×
            </button>
          </div>
          </div>
        </Dialog>
      </Modal>
    </ModalOverlay>
  );
}
