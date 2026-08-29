import { useCallback, useState } from "react";
import { apiClient } from "../../lib/api";
import { useAsyncAction } from "../../lib/useAsyncAction";
import { useResource } from "../../lib/useResource";

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

const fetchUsers = (signal: AbortSignal) =>
  apiClient
    .get<{ items: AdminUser[] }>("/v1/admin/users", { signal })
    .then((res) => res.items ?? []);

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
      await apiClient.post("/v1/admin/users", {
        username,
        password,
        is_admin: newIsAdmin,
      });
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
      await apiClient.put(
        `/v1/admin/users/${encodeURIComponent(passwordTarget.account_id)}/password`,
        { password },
      );
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
        await apiClient.patch(`/v1/admin/users/${encodeURIComponent(id)}`, changes);
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
        await apiClient.delete(`/v1/admin/users/${encodeURIComponent(id)}/messages`);
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
        await apiClient.delete(`/v1/admin/users/${encodeURIComponent(id)}`);
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
