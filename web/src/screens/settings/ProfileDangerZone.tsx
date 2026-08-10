import { useState } from "react";
import { useAuth } from "../../lib/auth";
import { apiClient } from "../../lib/api";
import DeleteAccountDialog from "../../components/DeleteAccountDialog";
import Button from "../../components/Button";
import { dangerButtonStyle } from "./profileStyles";

export function ProfileDangerZone({
  isDemo,
  username,
}: {
  isDemo: boolean;
  username: string;
}) {
  const { logout } = useAuth();
  const [dangerZoneOpen, setDangerZoneOpen] = useState(false);
  const [deleteDialogOpen, setDeleteDialogOpen] = useState(false);
  const [deleting, setDeleting] = useState(false);
  const [deletingMessages, setDeletingMessages] = useState(false);
  const [dangerError, setDangerError] = useState("");

  const busy = deleting || deletingMessages;
  const demoLocked = isDemo;

  const deleteAllMessages = async () => {
    if (demoLocked) return;
    if (
      !confirm(
        "Delete all messages and attachments? Your contacts and settings will remain.",
      )
    ) {
      return;
    }
    setDeletingMessages(true);
    setDangerError("");
    try {
      await apiClient.post("/v1/account/delete-messages", { confirm: true });
    } catch (e) {
      setDangerError(e instanceof Error ? e.message : String(e));
    } finally {
      setDeletingMessages(false);
    }
  };

  const performDeleteAccount = async () => {
    if (demoLocked) return;
    setDeleting(true);
    setDangerError("");
    try {
      await apiClient.post("/v1/auth/delete-account", { confirm: true });
      setDeleteDialogOpen(false);
      logout();
    } catch (e) {
      setDangerError(e instanceof Error ? e.message : String(e));
    } finally {
      setDeleting(false);
    }
  };

  return (
    <>
      <section style={{ marginTop: "2rem", paddingTop: "1.5rem", borderTop: "1px solid var(--border)" }}>
        <button
          type="button"
          aria-expanded={dangerZoneOpen}
          onClick={() => setDangerZoneOpen((open) => !open)}
          style={{
            display: "flex",
            alignItems: "center",
            gap: "0.5rem",
            width: "100%",
            padding: 0,
            border: "none",
            background: "transparent",
            cursor: "pointer",
            textAlign: "left",
          }}
        >
          <span
            style={{
              display: "inline-block",
              transform: dangerZoneOpen ? "rotate(90deg)" : "none",
              transition: "transform 0.15s ease",
              color: "var(--danger)",
              fontSize: "0.75rem",
            }}
          >
            ▶
          </span>
          <span
            style={{
              fontSize: "0.75rem",
              fontWeight: 600,
              letterSpacing: "0.06em",
              textTransform: "uppercase",
              color: "var(--danger)",
            }}
          >
            Danger zone
          </span>
        </button>
        <p style={{ margin: "0.35rem 0 0 1.25rem", fontSize: "0.813rem", color: "var(--muted)" }}>
          Delete messages or permanently remove your account.
        </p>

        {dangerZoneOpen && (
          <div style={{ marginTop: "1rem", marginLeft: "1.25rem", display: "flex", flexDirection: "column", gap: "1rem" }}>
            <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between", gap: "1rem" }}>
              <p style={{ margin: 0, flex: 1, fontSize: "0.813rem", color: "var(--muted)" }}>
                Delete all messages and attachments. Your contacts and settings remain.
              </p>
              <Button
                variant="danger"
                disabled={busy || demoLocked}
                onClick={() => void deleteAllMessages()}
                style={dangerButtonStyle}
                title={demoLocked ? "Unavailable on the demo account" : undefined}
              >
                {deletingMessages ? "Deleting…" : "Delete all messages"}
              </Button>
            </div>

            <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between", gap: "1rem" }}>
              <p style={{ margin: 0, flex: 1, fontSize: "0.813rem", color: "var(--muted)" }}>
                Delete this account and everything in it.
              </p>
              <Button
                variant="danger"
                disabled={busy || demoLocked}
                onClick={() => setDeleteDialogOpen(true)}
                style={dangerButtonStyle}
                title={demoLocked ? "Unavailable on the demo account" : undefined}
              >
                Delete account
              </Button>
            </div>

            {dangerError && (
              <div style={{ fontSize: "0.813rem", color: "var(--danger)" }} role="alert">
                {dangerError}
              </div>
            )}
          </div>
        )}
      </section>

      <DeleteAccountDialog
        open={deleteDialogOpen}
        username={username}
        deleting={deleting}
        onClose={() => {
          if (!deleting) setDeleteDialogOpen(false);
        }}
        onConfirm={() => void performDeleteAccount()}
      />
    </>
  );
}
