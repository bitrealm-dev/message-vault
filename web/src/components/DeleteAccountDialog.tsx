import { useEffect, useState } from "react";
import Button from "./Button";

export default function DeleteAccountDialog({
  open,
  username,
  deleting = false,
  onClose,
  onConfirm,
}: {
  open: boolean;
  username: string;
  deleting?: boolean;
  onClose: () => void;
  onConfirm: () => void;
}) {
  const [typedUsername, setTypedUsername] = useState("");

  useEffect(() => {
    if (open) setTypedUsername("");
  }, [open]);

  if (!open) return null;

  const expected = username.trim();
  const matches = expected.length > 0 && typedUsername === expected;

  return (
    <div
      role="presentation"
      onClick={() => {
        if (!deleting) onClose();
      }}
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
        aria-labelledby="mv-delete-account-dialog-title"
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
          disabled={deleting}
          onClick={onClose}
          style={{
            position: "absolute",
            top: "0.75rem",
            right: "0.75rem",
            border: "none",
            background: "transparent",
            color: "var(--muted)",
            cursor: deleting ? "not-allowed" : "pointer",
            fontSize: "1.25rem",
            lineHeight: 1,
          }}
        >
          ×
        </button>

        <h2
          id="mv-delete-account-dialog-title"
          style={{ margin: "0 1.5rem 0 0", fontSize: "1rem", fontWeight: 600 }}
        >
          Delete your account?
        </h2>

        <p style={{ margin: "0.75rem 0 0", fontSize: "0.875rem", color: "var(--muted)", lineHeight: 1.5 }}>
          This cannot be undone. Your messages, contacts, group chats, profile,
          and attachments will be permanently deleted.
        </p>

        <label style={{ display: "block", marginTop: "1.25rem" }}>
          <span style={{ fontSize: "0.875rem", color: "var(--text)" }}>
            Type your user ID{" "}
            {expected ? <strong>{expected}</strong> : null} to confirm.
          </span>
          <input
            type="text"
            value={typedUsername}
            onChange={(e) => setTypedUsername(e.target.value)}
            disabled={deleting}
            autoComplete="off"
            spellCheck={false}
            style={{
              marginTop: "0.5rem",
              width: "100%",
              boxSizing: "border-box",
              padding: "0.5rem 0.75rem",
              fontSize: "0.875rem",
              border: "1px solid var(--border)",
              borderRadius: "4px",
              background: "var(--bg)",
              color: "var(--text)",
            }}
          />
        </label>

        <div style={{ marginTop: "1.25rem", display: "flex", justifyContent: "flex-end" }}>
          <Button
            variant="danger"
            disabled={deleting || !matches}
            onClick={onConfirm}
            style={{ padding: "0.5rem 1rem", fontSize: "0.813rem" }}
          >
            {deleting ? "Deleting…" : "Permanently delete my account"}
          </Button>
        </div>
      </div>
    </div>
  );
}
