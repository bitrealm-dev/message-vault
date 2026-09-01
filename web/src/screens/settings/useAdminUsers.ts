import { useCallback, useState } from "react";
import { useAsyncAction } from "../../lib/useAsyncAction";
import { useResource } from "../../lib/useResource";
import {
  createUser as createVaultUser,
  deleteUserMessages,
  deleteUser as deleteVaultUser,
  listUsers,
  setUserPassword as setVaultUserPassword,
  updateUser,
} from "../../lib/vaultApi";

/** One account as an administrator sees it — mirrors `AdminUser` in `admin_api.rs`. */
export type AdminUser = {
  account_id: string;
  username: string;
  is_admin: boolean;
  disabled: boolean;
  can_import: boolean;
  can_export: boolean;
  can_delete: boolean;
  message_count: number;
  storage_bytes: number;
};

const fetchUsers = (signal: AbortSignal) => listUsers({ signal }).then((res) => res.items ?? []);

/** The administrator's view of every account, plus the actions on one. */
export function useAdminUsers() {
  const { data, loading, error: loadError, reload } = useResource("admin/users", fetchUsers);
  const { busy, error: actionError, run, clearError } = useAsyncAction();

  const [composing, setComposing] = useState(false);
  const [newUsername, setNewUsername] = useState("");
  const [newPassword, setNewPassword] = useState("");
  const [newIsAdmin, setNewIsAdmin] = useState(false);

  const [passwordTarget, setPasswordTarget] = useState<AdminUser | null>(null);
  const [resetPassword, setResetPassword] = useState("");

  const cancelCompose = useCallback(() => {
    setComposing(false);
    setNewUsername("");
    setNewPassword("");
    setNewIsAdmin(false);
    clearError();
  }, [clearError]);

  const createUser = useCallback(() => {
    const username = newUsername.trim();
    const password = newPassword;
    if (!username || !password) return Promise.resolve();
    return run(async () => {
      await createVaultUser({ username, password, is_admin: newIsAdmin });
      setNewUsername("");
      setNewPassword("");
      setNewIsAdmin(false);
      setComposing(false);
      reload();
    });
  }, [newUsername, newPassword, newIsAdmin, run, reload]);

  const openPasswordReset = useCallback(
    (user: AdminUser) => {
      clearError();
      setPasswordTarget(user);
      setResetPassword("");
    },
    [clearError],
  );

  const closePasswordReset = useCallback(() => {
    if (busy) return;
    setPasswordTarget(null);
    setResetPassword("");
  }, [busy]);

  const setUserPassword = useCallback(() => {
    if (!passwordTarget) return Promise.resolve();
    const password = resetPassword;
    if (!password) return Promise.resolve();
    return run(async () => {
      await setVaultUserPassword(passwordTarget.account_id, { password });
      setPasswordTarget(null);
      setResetPassword("");
    });
  }, [passwordTarget, resetPassword, run]);

  const patch = useCallback(
    (
      id: string,
      changes: Partial<
        Pick<AdminUser, "is_admin" | "disabled" | "can_import" | "can_export" | "can_delete">
      >,
    ) =>
      run(async () => {
        await updateUser(id, changes);
        reload();
      }),
    [run, reload],
  );

  // `run` swallows its error into `actionError` rather than rethrowing, so it
  // always resolves — a caller cannot tell success from failure just by
  // awaiting it. These two report it explicitly (via a boolean the caller
  // awaits) so a confirmation dialog can stay open and show the refusal
  // instead of closing as though the delete had gone through.
  const deleteMessages = useCallback(
    (id: string) => {
      let succeeded = false;
      return run(async () => {
        await deleteUserMessages(id);
        succeeded = true;
        reload();
      }).then(() => succeeded);
    },
    [run, reload],
  );

  const deleteUser = useCallback(
    (id: string) => {
      let succeeded = false;
      return run(async () => {
        await deleteVaultUser(id);
        succeeded = true;
        reload();
      }).then(() => succeeded);
    },
    [run, reload],
  );

  return {
    users: data ?? [],
    loading,
    loadError,
    busy,
    actionError,
    clearError,
    patch,
    deleteMessages,
    deleteUser,
    composing,
    setComposing,
    newUsername,
    setNewUsername,
    newPassword,
    setNewPassword,
    newIsAdmin,
    setNewIsAdmin,
    cancelCompose,
    createUser,
    passwordTarget,
    resetPassword,
    setResetPassword,
    openPasswordReset,
    closePasswordReset,
    setUserPassword,
  };
}
