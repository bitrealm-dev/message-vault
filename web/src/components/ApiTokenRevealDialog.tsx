import { useEffect, useState } from "react";
import Button from "./Button";

export default function ApiTokenRevealDialog({
  open,
  label,
  token,
  onClose,
}: {
  open: boolean;
  label: string;
  token: string;
  onClose: () => void;
}) {
  const [copied, setCopied] = useState(false);

  useEffect(() => {
    if (open) setCopied(false);
  }, [open]);

  if (!open) return null;

  const copy = async () => {
    try {
      await navigator.clipboard.writeText(token);
      setCopied(true);
    } catch {
      setCopied(false);
    }
  };

  return (
    <div
      role="presentation"
      onClick={onClose}
      style={{
        position: "fixed",
        inset: 0,
        zIndex: 200,
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        background: "rgba(0,0,0,0.45)",
        padding: "1rem",
      }}
    >
      <div
        role="dialog"
        aria-modal="true"
        aria-labelledby="mv-api-token-reveal-title"
        onClick={(e) => e.stopPropagation()}
        style={{
          position: "relative",
          width: "100%",
          maxWidth: "28rem",
          borderRadius: "8px",
          border: "1px solid var(--border)",
          background: "var(--panel)",
          padding: "1.25rem",
          boxShadow: "0 16px 48px rgba(0,0,0,0.25)",
        }}
      >
        <button
          type="button"
          aria-label="Close"
          onClick={onClose}
          style={{
            position: "absolute",
            top: "0.75rem",
            right: "0.75rem",
            border: "none",
            background: "transparent",
            color: "var(--muted)",
            cursor: "pointer",
            fontSize: "1.25rem",
            lineHeight: 1,
          }}
        >
          ×
        </button>

        <h3
          id="mv-api-token-reveal-title"
          style={{ margin: "0 1.5rem 0.5rem 0", fontSize: "1.05rem", color: "var(--text)" }}
        >
          API token created
        </h3>
        <p style={{ margin: "0 0 0.75rem", fontSize: "0.813rem", color: "var(--muted)" }}>
          Copy this secret for <strong style={{ color: "var(--text)" }}>{label}</strong> now.
          It will not be shown again.
        </p>
        <div
          style={{
            padding: "0.5rem 0.65rem",
            borderRadius: "4px",
            border: "1px solid var(--border)",
            background: "var(--elevated)",
            fontFamily: "ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace",
            fontSize: "0.75rem",
            wordBreak: "break-all",
            color: "var(--text)",
            marginBottom: "0.75rem",
          }}
        >
          {token}
        </div>
        <div style={{ display: "flex", gap: "0.5rem", justifyContent: "flex-end" }}>
          <Button variant="ghost" onClick={onClose}>
            Done
          </Button>
          <Button variant="primary" onClick={() => void copy()}>
            {copied ? "Copied" : "Copy"}
          </Button>
        </div>
      </div>
    </div>
  );
}
