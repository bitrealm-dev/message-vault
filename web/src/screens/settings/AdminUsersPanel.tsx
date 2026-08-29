import { useState } from "react";
import Button from "../../components/Button";
import Checkbox from "../../components/Checkbox";
import ConfirmDialog from "../../components/ConfirmDialog";
import { tdClass, tdMuted, thClass } from "./apiTokensUtils";
import { formatBytes } from "./storage/storageUtils";
import { type AdminUser, useAdminUsers } from "./useAdminUsers";

type ConfirmTarget = { user: AdminUser; kind: "messages" | "account" };

function confirmBody(target: ConfirmTarget): string {
  const { user, kind } = target;
  const count = user.message_count.toLocaleString();
  return kind === "messages"
    ? `This permanently deletes ${count} messages belonging to ${user.username}, and their attachments. It cannot be undone.`
    : `This permanently deletes ${user.username}'s account along with ${count} messages and their attachments. It cannot be undone.`;
}

/** Every account in the vault, with the actions an administrator has on one. */
export function AdminUsersPanel() {
  const {
    users,
    loading,
    loadError,
    busy,
    actionError,
    clearError,
    patch,
    deleteMessages,
    deleteUser,
  } = useAdminUsers();
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
      <h3 className="m-0 text-text">Users</h3>
      <p className="mt-[0.35rem] text-[0.875rem] text-muted">
        Everyone with an account on this vault. You can change what they may do, disable them, or
        delete their messages. You cannot read them.
      </p>

      {/* A failure while the delete confirmation is open shows inside that
          dialog instead (via ConfirmDialog's `error` prop) — showing it here
          too would duplicate it, and the dialog is where the user is looking. */}
      {actionError && confirming === null ? (
        <p className="mt-3 text-[0.875rem] text-danger" role="alert">
          {actionError}
        </p>
      ) : null}

      <div className="mt-4 overflow-x-auto rounded-xl border border-border bg-elevated">
        <table className="w-full border-collapse">
          <thead>
            <tr>
              <th className={thClass}>User</th>
              <th className={thClass}>Status</th>
              <th className={thClass}>Messages</th>
              <th className={thClass}>Storage</th>
              <th className={thClass}>Admin</th>
              <th className={thClass}>Import</th>
              <th className={thClass}>Export</th>
              <th className={thClass}>Delete</th>
              <th className={thClass}>Actions</th>
            </tr>
          </thead>
          <tbody>
            {users.map((user) => (
              <tr key={user.account_id} className="border-t border-border">
                <td className={tdClass}>
                  {user.username}
                  {user.is_admin ? <span className="ml-2 text-muted">(admin)</span> : null}
                </td>
                <td className={tdMuted}>{user.disabled ? "Disabled" : "Active"}</td>
                <td className={tdMuted}>{user.message_count.toLocaleString()}</td>
                <td className={tdMuted}>{formatBytes(user.storage_bytes)}</td>
                <td className={tdClass}>
                  <Checkbox
                    checked={user.is_admin}
                    disabled={busy}
                    aria-label={`Allow ${user.username} to manage the vault`}
                    onChange={(checked) => patch(user.account_id, { is_admin: checked })}
                  />
                </td>
                <td className={tdClass}>
                  <Checkbox
                    checked={user.can_import}
                    disabled={busy}
                    aria-label={`Allow importing messages for ${user.username}`}
                    onChange={(checked) => patch(user.account_id, { can_import: checked })}
                  />
                </td>
                <td className={tdClass}>
                  <Checkbox
                    checked={user.can_export}
                    disabled={busy}
                    aria-label={`Allow exporting messages for ${user.username}`}
                    onChange={(checked) => patch(user.account_id, { can_export: checked })}
                  />
                </td>
                <td className={tdClass}>
                  <Checkbox
                    checked={user.can_delete}
                    disabled={busy}
                    aria-label={`Allow deleting messages and attachments for ${user.username}`}
                    onChange={(checked) => patch(user.account_id, { can_delete: checked })}
                  />
                </td>
                <td className={tdClass}>
                  <div className="flex flex-wrap items-center gap-1">
                    <Button
                      variant="secondary"
                      size="xs"
                      disabled={busy}
                      onClick={() => patch(user.account_id, { disabled: !user.disabled })}
                    >
                      {user.disabled ? "Enable" : "Disable"}
                    </Button>
                    <Button
                      variant="secondary"
                      size="xs"
                      disabled={busy}
                      onClick={() => openConfirm({ user, kind: "messages" })}
                    >
                      Delete messages
                    </Button>
                    <Button
                      variant="danger"
                      size="xs"
                      disabled={busy}
                      onClick={() => openConfirm({ user, kind: "account" })}
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
          const { user, kind } = confirming;
          // `deleteMessages`/`deleteUser` resolve to whether the call actually
          // succeeded (the shared `run` swallows the error into `actionError`
          // rather than rejecting). Close only on success — a 400 ("only
          // administrator") or 404 must keep the dialog open with the reason
          // showing, not vanish as though the delete had gone through.
          const ok =
            kind === "messages"
              ? await deleteMessages(user.account_id)
              : await deleteUser(user.account_id);
          if (ok) setConfirming(null);
        }}
      />
    </section>
  );
}
