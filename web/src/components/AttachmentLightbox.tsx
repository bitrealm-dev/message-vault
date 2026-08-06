import { useEffect } from "react";
import { getBaseUrl } from "../lib/api";
import type { MessageAttachment } from "../lib/types";

export default function AttachmentLightbox({
  attachments,
  currentIndex,
  onClose,
  onPrev,
  onNext,
}: {
  attachments: MessageAttachment[];
  currentIndex: number;
  onClose: () => void;
  onPrev: () => void;
  onNext: () => void;
}) {
  const attachment = attachments[currentIndex];
  const url = attachment?.sha256
    ? `${getBaseUrl()}/v1/assets/${attachment.sha256}`
    : null;

  // Close with Escape, navigate with arrow keys
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
      else if (e.key === "ArrowLeft") onPrev();
      else if (e.key === "ArrowRight") onNext();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose, onPrev, onNext]);

  if (!attachment || !url) return null;

  return (
    <div style={{
      position: "fixed", inset: 0, background: "rgba(0,0,0,0.9)",
      display: "flex", alignItems: "center", justifyContent: "center",
      zIndex: 200,
    }} onClick={onClose}>
      {/* Prev */}
      {attachments.length > 1 && (
        <button onClick={(e) => { e.stopPropagation(); onPrev(); }}
          style={{
            position: "absolute", left: "1rem", top: "50%", transform: "translateY(-50%)",
            background: "rgba(255,255,255,0.2)", border: "none", color: "#fff",
            fontSize: "2rem", width: "48px", height: "48px", borderRadius: "50%",
            cursor: "pointer", display: "flex", alignItems: "center", justifyContent: "center",
          }}>
          ‹
        </button>
      )}

      <img
        src={url}
        alt={attachment.original_name || "attachment"}
        style={{ maxWidth: "90vw", maxHeight: "90vh", objectFit: "contain" }}
        onClick={(e) => e.stopPropagation()}
      />

      {/* Next */}
      {attachments.length > 1 && (
        <button onClick={(e) => { e.stopPropagation(); onNext(); }}
          style={{
            position: "absolute", right: "1rem", top: "50%", transform: "translateY(-50%)",
            background: "rgba(255,255,255,0.2)", border: "none", color: "#fff",
            fontSize: "2rem", width: "48px", height: "48px", borderRadius: "50%",
            cursor: "pointer", display: "flex", alignItems: "center", justifyContent: "center",
          }}>
          ›
        </button>
      )}

      {/* Close + counter */}
      <div style={{ position: "absolute", top: "1rem", right: "1rem", display: "flex", gap: "1rem", alignItems: "center" }}>
        <span style={{ color: "#fff", fontSize: "0.875rem" }}>
          {currentIndex + 1} / {attachments.length}
        </span>
        <button onClick={onClose}
          style={{ background: "rgba(255,255,255,0.2)", border: "none", color: "#fff",
            fontSize: "1.5rem", width: "40px", height: "40px", borderRadius: "50%", cursor: "pointer" }}>
          ×
        </button>
      </div>
    </div>
  );
}
