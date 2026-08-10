import { useState } from "react";
import { useAuth } from "../../lib/auth";
import { apiClient } from "../../lib/api";
import DeleteAccountDialog from "../../components/DeleteAccountDialog";
import ConfirmDialog from "../../components/ConfirmDialog";
import Button from "../../components/Button";
import { dangerButtonClass } from "./profileStyles";

export function ProfileDangerZone({
  isDemo,
  username,
}: {
  isDemo: boolean;
  username: string;
}) {
  const { logout } = useAuth();
  const [dangerZoneOpen, setDangerZoneOpen] = useState(false);
  const [confirmDeleteMessagesOpen, setConfirmDeleteMessagesOpen] = useState(false);
  const [deleteDialogOpen, setDeleteDialogOpen] = useState(false);
  const [deleting, setDeleting] = useState(false);
  const [deletingMessages, setDeletingMessages] = useState(false);
  const [dangerError, setDangerError] = useState("");

  const busy = deleting || deletingMessages;
  const demoLocked = isDemo;

  const deleteAllMessages = async () => {
    if (demoLocked) return;
    setDeletingMessages(true);
    setDangerError("");
    try {
      await apiClient.post("/v1/account/delete-messages", { confirm: true });
    } catch (e) {
      setDangerError(e instanceof Error ? e.message : String(e));
    } finally {
      setDeletingMessages(false);
      setConfirmDeleteMessagesOpen(false);
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
      <section className="mt-8 border-t border-border pt-6">
        <button
          type="button"
          aria-expanded={dangerZoneOpen}
          onClick={() => setDangerZoneOpen((open) => !open)}
          className="flex w-full cursor-pointer items-center gap-2 border-none bg-transparent p-0 text-left"
        >
          <span
            className={`inline-block text-[0.75rem] text-danger transition-transform duration-150 ${
              dangerZoneOpen ? "rotate-90" : ""
            }`}
          >
            ▶
          </span>
          <span className="text-[0.75rem] font-semibold uppercase tracking-[0.06em] text-danger">
            Danger zone
          </span>
        </button>
        <p className="ml-5 mt-[0.35rem] text-[0.813rem] text-muted">
          Delete messages or permanently remove your account.
        </p>

        {dangerZoneOpen && (
          <div className="ml-5 mt-4 flex flex-col gap-4">
            <div className="flex items-center justify-between gap-4">
              <p className="m-0 flex-1 text-[0.813rem] text-muted">
                Delete all messages and attachments. Your contacts and settings remain.
              </p>
              <Button
                variant="danger"
                disabled={busy || demoLocked}
                onClick={() => setConfirmDeleteMessagesOpen(true)}
                className={`${dangerButtonClass} !w-40 shrink-0 !px-3 !py-2 !text-[0.813rem]`}
                title={demoLocked ? "Unavailable on the demo account" : undefined}
              >
                {deletingMessages ? "Deleting…" : "Delete all messages"}
              </Button>
            </div>

            <div className="flex items-center justify-between gap-4">
              <p className="m-0 flex-1 text-[0.813rem] text-muted">
                Delete this account and everything in it.
              </p>
              <Button
                variant="danger"
                disabled={busy || demoLocked}
                onClick={() => setDeleteDialogOpen(true)}
                className={`${dangerButtonClass} !w-40 shrink-0 !px-3 !py-2 !text-[0.813rem]`}
                title={demoLocked ? "Unavailable on the demo account" : undefined}
              >
                Delete account
              </Button>
            </div>

            {dangerError && (
              <div className="text-[0.813rem] text-danger" role="alert">
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

      <ConfirmDialog
        open={confirmDeleteMessagesOpen}
        title="Delete all messages?"
        body="Delete all messages and attachments? Your contacts and settings will remain."
        confirmLabel="Delete all messages"
        danger
        busy={deletingMessages}
        onClose={() => setConfirmDeleteMessagesOpen(false)}
        onConfirm={() => void deleteAllMessages()}
      />
    </>
  );
}
