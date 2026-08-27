import { useState } from "react";
import Button from "../../components/Button";
import ConfirmDialog from "../../components/ConfirmDialog";
import DeleteAccountDialog from "../../components/DeleteAccountDialog";
import { apiClient } from "../../lib/api";
import { useAuth } from "../../lib/auth";
import { dangerButtonClass } from "./profileStyles";

export function ProfileDangerZone({ isDemo, username }: { isDemo: boolean; username: string }) {
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

  const performDeleteAccount = async (currentPassword: string) => {
    if (demoLocked) return;
    setDeleting(true);
    setDangerError("");
    try {
      await apiClient.post("/v1/auth/delete-account", {
        confirm: true,
        current_password: currentPassword,
      });
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
          <div className="ml-5 mt-4 rounded-xl border-solid border-danger p-5 [border-width:0.75px]">
            <div className="grid grid-cols-[minmax(0,1fr)_auto] items-center gap-x-5 gap-y-5">
              <div className="min-w-0">
                <p className="m-0 text-[0.813rem] font-bold text-text">
                  Delete all messages and attachments
                </p>
                <p className="m-0 mt-0.5 text-[0.813rem] text-muted">
                  Your contacts and settings remain
                </p>
              </div>
              <div className="justify-self-end p-px">
                <Button
                  variant="danger"
                  disabled={busy || demoLocked}
                  onClick={() => setConfirmDeleteMessagesOpen(true)}
                  className={`${dangerButtonClass} !box-border !w-auto !min-w-[10.5rem] !whitespace-nowrap !border-transparent !px-3 !py-2 !text-[0.813rem] !shadow-[inset_0_0_0_1px_var(--danger-soft-border)]`}
                  title={demoLocked ? "Unavailable on the demo account" : undefined}
                >
                  {deletingMessages ? "Deleting…" : "Delete all messages"}
                </Button>
              </div>

              <div className="min-w-0">
                <p className="m-0 text-[0.813rem] font-bold text-text">Delete your account</p>
                <p className="m-0 mt-0.5 text-[0.813rem] text-muted">
                  Permanently delete all contacts, messages, and attachments. This can&apos;t be
                  undone.
                </p>
              </div>
              <div className="justify-self-end p-px">
                <Button
                  variant="danger"
                  disabled={busy || demoLocked}
                  onClick={() => setDeleteDialogOpen(true)}
                  className={`${dangerButtonClass} !box-border !w-auto !min-w-[10.5rem] !whitespace-nowrap !border-transparent !px-3 !py-2 !text-[0.813rem] !shadow-[inset_0_0_0_1px_var(--danger-soft-border)]`}
                  title={demoLocked ? "Unavailable on the demo account" : undefined}
                >
                  Delete account
                </Button>
              </div>

              {dangerError && (
                <div className="col-span-2 text-[0.813rem] text-danger" role="alert">
                  {dangerError}
                </div>
              )}
            </div>
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
        onConfirm={(password) => void performDeleteAccount(password)}
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
