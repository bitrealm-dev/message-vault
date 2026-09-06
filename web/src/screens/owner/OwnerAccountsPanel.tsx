import { useState } from "react";
import Button from "../../components/Button";
import Checkbox from "../../components/Checkbox";
import ConfirmDialog from "../../components/ConfirmDialog";
import ModalShell, { DialogError, DialogFooter } from "../../components/ModalShell";
import TextField from "../../components/TextField";
import { tdClass, tdMuted, thClass } from "../settings/apiTokensUtils";
import { formatBytes } from "../settings/storage/storageUtils";
import { type ManagedAccount, useOwnerAccounts } from "./useOwnerAccounts";

type ConfirmTarget = { account: ManagedAccount; kind: "messages" | "account" };

function confirmBody(target: ConfirmTarget): string {
  const { account, kind } = target;
  const count = account.message_count.toLocaleString();
  return kind === "messages"
    ? `This permanently deletes ${count} messages belonging to ${account.username}, and their attachments. It cannot be undone.`
    : `This permanently deletes ${account.username}'s account along with ${count} messages and their attachments. It cannot be undone.`;
}

/**
 * The accounts of this vault, and what the vault owner may do to one.
 *
 * A row carries a username, a status, a message count and a storage total —
 * never a message. The counts are what the owner acts on: deleting an
 * account's messages is done on the strength of the number and the account
 * holder's word, not on inspection.
 */
export function OwnerAccountsPanel() {
  const {
    accounts,
    loading,
    loadError,
    busy,
    actionError,
    clearError,
    patch,
    deleteMessages,
    deleteAccount,
    composing,
    setComposing,
    newUsername,
    setNewUsername,
    newPassword,
    setNewPassword,
    cancelCompose,
    createAccount,
    passwordTarget,
    resetPassword,
    setResetPassword,
    openPasswordReset,
    closePasswordReset,
    setAccountPassword,
  } = useOwnerAccounts();
  const [confirming, setConfirming] = useState<ConfirmTarget | null>(null);

  if (loading) return <p className="text-[0.875rem] text-muted">Loading accounts…</p>;
  if (loadError) return <p className="text-[0.875rem] text-danger">{loadError}</p>;

  const openConfirm = (target: ConfirmTarget) => {
    clearError();
    setConfirming(target);
  };

  const closeConfirm = () => {
    if (busy) return;
    clearError();
    setConfirming(null);
  };

  return (
    <section>
      <div className="flex flex-wrap items-center justify-between gap-2">
        <h3 className="m-0 text-text">User Accounts</h3>
        {!composing && (
          <Button
            variant="secondary"
            size="xs"
            disabled={busy}
            onClick={() => {
              clearError();
              setComposing(true);
            }}
          >
            Add account
          </Button>
        )}
      </div>
      <p className="mt-[0.35rem] text-[0.875rem] text-muted">
        The people with an account on this vault. You can change what they may do, disable them, or
        delete their messages. You cannot read them.
      </p>

      {/* A failure while the delete confirmation or password dialog is open
          shows inside that dialog instead (via its own `error` prop) — showing
          it here too would duplicate it, and the dialog is where the person is
          looking. */}
      {actionError && confirming === null && passwordTarget === null ? (
        <p className="mt-3 text-[0.875rem] text-danger" role="alert">
          {actionError}
        </p>
      ) : null}

      {composing && (
        <div className="mt-3 flex flex-col gap-3 rounded-xl border border-border bg-elevated p-3">
          <div className="flex flex-wrap items-end gap-2">
            <TextField
              value={newUsername}
              onChange={setNewUsername}
              placeholder="Username"
              isDisabled={busy}
              aria-label="New account's username"
              className="min-w-[10rem] flex-1"
            />
            <TextField
              value={newPassword}
              onChange={setNewPassword}
              type="password"
              placeholder="First password"
              isDisabled={busy}
              aria-label="New account's first password"
              className="min-w-[10rem] flex-1"
            />
            <Button
              variant="secondary"
              disabled={busy || !newUsername.trim() || !newPassword}
              onClick={() => void createAccount()}
              className="!px-3 !py-1.5 !text-[0.75rem]"
            >
              Save
            </Button>
            <Button
              variant="secondary"
              disabled={busy}
              onClick={cancelCompose}
              className="!px-3 !py-1.5 !text-[0.75rem]"
            >
              Cancel
            </Button>
          </div>
          <p className="text-[0.75rem] text-muted">
            Hand this password over yourself. Whoever signs in with it is made to replace it before
            they can go anywhere, so you will not know theirs.
          </p>
        </div>
      )}

      <div className="mt-4 overflow-x-auto rounded-xl border border-border bg-elevated">
        <table className="w-full border-collapse">
          <thead>
            <tr>
              <th className={thClass}>Account</th>
              <th className={thClass}>Status</th>
              <th className={thClass}>Messages</th>
              <th className={thClass}>Storage</th>
              <th className={thClass}>Import</th>
              <th className={thClass}>Export</th>
              <th className={thClass}>Delete</th>
              <th className={thClass}>Actions</th>
            </tr>
          </thead>
          <tbody>
            {accounts.map((account) => (
              <tr key={account.account_id} className="border-t border-border">
                <td className={tdClass}>
                  {account.username}
                  {account.must_change_password ? (
                    <span className="ml-2 text-muted">(has not set a password)</span>
                  ) : null}
                </td>
                <td className={tdMuted}>{account.disabled ? "Disabled" : "Active"}</td>
                <td className={tdMuted}>{account.message_count.toLocaleString()}</td>
                <td className={tdMuted}>{formatBytes(account.storage_bytes)}</td>
                <td className={tdClass}>
                  <Checkbox
                    checked={account.can_import}
                    disabled={busy}
                    aria-label={`Allow importing messages for ${account.username}`}
                    onChange={(checked) => patch(account.account_id, { can_import: checked })}
                  />
                </td>
                <td className={tdClass}>
                  <Checkbox
                    checked={account.can_export}
                    disabled={busy}
                    aria-label={`Allow exporting messages for ${account.username}`}
                    onChange={(checked) => patch(account.account_id, { can_export: checked })}
                  />
                </td>
                <td className={tdClass}>
                  <Checkbox
                    checked={account.can_delete}
                    disabled={busy}
                    aria-label={`Allow deleting messages and attachments for ${account.username}`}
                    onChange={(checked) => patch(account.account_id, { can_delete: checked })}
                  />
                </td>
                <td className={tdClass}>
                  <div className="flex flex-wrap items-center gap-1">
                    <Button
                      variant="secondary"
                      size="xs"
                      disabled={busy}
                      onClick={() => patch(account.account_id, { disabled: !account.disabled })}
                    >
                      {account.disabled ? "Enable" : "Disable"}
                    </Button>
                    <Button
                      variant="secondary"
                      size="xs"
                      disabled={busy}
                      onClick={() => openPasswordReset(account)}
                    >
                      Reset password
                    </Button>
                    <Button
                      variant="secondary"
                      size="xs"
                      disabled={busy}
                      onClick={() => openConfirm({ account, kind: "messages" })}
                    >
                      Delete messages
                    </Button>
                    <Button
                      variant="danger"
                      size="xs"
                      disabled={busy}
                      onClick={() => openConfirm({ account, kind: "account" })}
                    >
                      Delete account
                    </Button>
                  </div>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>

      <ConfirmDialog
        open={confirming !== null}
        title={confirming?.kind === "messages" ? "Delete messages" : "Delete account"}
        body={confirming ? confirmBody(confirming) : ""}
        confirmLabel="Delete"
        danger
        busy={busy}
        error={actionError}
        onClose={closeConfirm}
        onConfirm={async () => {
          if (!confirming) return;
          const { account, kind } = confirming;
          // These resolve to whether the call actually succeeded: on a refusal
          // the mutation answers `false` and leaves the reason in `actionError`
          // instead of throwing. Close only on success — a 404 must keep the
          // dialog open with the reason showing, not vanish as though the
          // delete had gone through.
          const ok =
            kind === "messages"
              ? await deleteMessages(account.account_id)
              : await deleteAccount(account.account_id);
          if (ok) setConfirming(null);
        }}
      />

      <ModalShell
        open={passwordTarget !== null}
        onOpenChange={(o) => {
          if (!o) closePasswordReset();
        }}
        dismissable={!busy}
        label="Reset password"
        title="Reset password"
        onClose={closePasswordReset}
        closeDisabled={busy}
        maxWidth="24rem"
      >
        <p className="mb-3 text-[0.813rem] text-muted">
          Set a new password for {passwordTarget?.username}. This ends their current session, and
          they are made to replace this password once they sign in with it.
        </p>
        <TextField
          label="New password"
          value={resetPassword}
          onChange={setResetPassword}
          type="password"
          isDisabled={busy}
          autoFocus
          onKeyDown={(e) => {
            if (e.key === "Enter") {
              e.preventDefault();
              void setAccountPassword();
            }
          }}
        />
        <DialogError message={actionError} />
        <DialogFooter>
          <Button onPress={closePasswordReset} isDisabled={busy}>
            Cancel
          </Button>
          <Button
            variant="primary"
            onPress={() => void setAccountPassword()}
            isDisabled={busy || !resetPassword}
          >
            {busy ? "Saving…" : "Save"}
          </Button>
        </DialogFooter>
      </ModalShell>
    </section>
  );
}
