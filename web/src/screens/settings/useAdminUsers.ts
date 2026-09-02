import { type UseMutationResult, useMutation } from "@tanstack/react-query";
import { useCallback, useState } from "react";
import { apiErrorMessage } from "../../lib/apiErrorMessage";
import {
  createUser as createVaultUser,
  deleteUserMessages,
  deleteUser as deleteVaultUser,
  listUsers,
  setUserPassword as setVaultUserPassword,
  updateUser,
} from "../../lib/vaultApi";
import { keys } from "../../lib/vaultKeys";
import { useVaultCache, useVaultQuery } from "../../lib/vaultQuery";

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

/** The flags an administrator can change on one account. */
export type AdminUserChanges = Partial<
  Pick<AdminUser, "is_admin" | "disabled" | "can_import" | "can_export" | "can_delete">
>;

const fetchUsers = (signal: AbortSignal) => listUsers({ signal }).then((res) => res.items ?? []);

/** Every write but a password change shows on the account list. */
function useAdminWrite<V>(
  write: (vars: V) => Promise<unknown>,
  refreshesTheList = true,
): UseMutationResult<unknown, Error, V> {
  const cache = useVaultCache();
  return useMutation<unknown, Error, V>({
    mutationFn: write,
    onSettled: refreshesTheList ? () => cache.invalidate(keys.adminUsers.all) : undefined,
  });
}

export function useCreateUser(): UseMutationResult<
  unknown,
  Error,
  { username: string; password: string; is_admin: boolean }
> {
  return useAdminWrite((body) => createVaultUser(body));
}

export function useUpdateUser(): UseMutationResult<
  unknown,
  Error,
  { id: string; changes: AdminUserChanges }
> {
  return useAdminWrite(({ id, changes }) => updateUser(id, changes));
}

export function useDeleteUser(): UseMutationResult<unknown, Error, string> {
  return useAdminWrite((id: string) => deleteVaultUser(id));
}

export function useDeleteUserMessages(): UseMutationResult<unknown, Error, string> {
  return useAdminWrite((id: string) => deleteUserMessages(id));
}

export function useSetUserPassword(): UseMutationResult<
  unknown,
  Error,
  { id: string; password: string }
> {
  return useAdminWrite(({ id, password }) => setVaultUserPassword(id, { password }), false);
}

/** The administrator's view of every account, plus the actions on one. */
export function useAdminUsers() {
  const {
    data,
    isPending: loading,
    error: loadError,
  } = useVaultQuery(keys.adminUsers.all, fetchUsers);
  const createAccount = useCreateUser();
  const changeAccount = useUpdateUser();
  const removeAccount = useDeleteUser();
  const removeMessages = useDeleteUserMessages();
  const changePassword = useSetUserPassword();

  const busy =
    createAccount.isPending ||
    changeAccount.isPending ||
    removeAccount.isPending ||
    removeMessages.isPending ||
    changePassword.isPending;

  // Each mutate call resets that mutation's own error and stamps a fresh
  // `submittedAt`, so whichever of the five last started is also whichever
  // last settled; its error (or lack of one) is `actionError`. A fixed
  // create-then-update-then-… order would instead let an old failure outlive
  // a later, successful write.
  const latest = [
    createAccount,
    changeAccount,
    removeAccount,
    removeMessages,
    changePassword,
  ].reduce((newest, next) => (next.submittedAt > newest.submittedAt ? next : newest));
  const actionError = latest.error ? latest.error.message : "";

  const resetCreate = createAccount.reset;
  const resetChange = changeAccount.reset;
  const resetRemove = removeAccount.reset;
  const resetRemoveMessages = removeMessages.reset;
  const resetChangePassword = changePassword.reset;
  const clearError = useCallback(() => {
    resetCreate();
    resetChange();
    resetRemove();
    resetRemoveMessages();
    resetChangePassword();
  }, [resetCreate, resetChange, resetRemove, resetRemoveMessages, resetChangePassword]);

  const [composing, setComposing] = useState(false);
  const [newUsername, setNewUsername] = useState("");
  const [newPassword, setNewPassword] = useState("");
  const [newIsAdmin, setNewIsAdmin] = useState(false);

  const [passwordTarget, setPasswordTarget] = useState<AdminUser | null>(null);
  const [resetPasswordValue, setResetPasswordValue] = useState("");

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
    if (!username || !password) return;
    createAccount.mutate(
      { username, password, is_admin: newIsAdmin },
      {
        onSuccess: () => {
          setNewUsername("");
          setNewPassword("");
          setNewIsAdmin(false);
          setComposing(false);
        },
      },
    );
  }, [newUsername, newPassword, newIsAdmin, createAccount.mutate]);

  const openPasswordReset = useCallback(
    (user: AdminUser) => {
      clearError();
      setPasswordTarget(user);
      setResetPasswordValue("");
    },
    [clearError],
  );

  const closePasswordReset = useCallback(() => {
    if (busy) return;
    setPasswordTarget(null);
    setResetPasswordValue("");
  }, [busy]);

  const setUserPassword = useCallback(() => {
    if (!passwordTarget || !resetPasswordValue) return;
    changePassword.mutate(
      { id: passwordTarget.account_id, password: resetPasswordValue },
      {
        onSuccess: () => {
          setPasswordTarget(null);
          setResetPasswordValue("");
        },
      },
    );
  }, [passwordTarget, resetPasswordValue, changePassword.mutate]);

  const patch = useCallback(
    (id: string, changes: AdminUserChanges) =>
      changeAccount.mutateAsync({ id, changes }).then(
        () => undefined,
        () => undefined,
      ),
    [changeAccount.mutateAsync],
  );

  // These two answer whether the vault agreed, so the confirmation dialog can
  // stay open and show the refusal instead of closing as though it had worked.
  const deleteMessages = useCallback(
    (id: string) =>
      removeMessages.mutateAsync(id).then(
        () => true,
        () => false,
      ),
    [removeMessages.mutateAsync],
  );

  const deleteUser = useCallback(
    (id: string) =>
      removeAccount.mutateAsync(id).then(
        () => true,
        () => false,
      ),
    [removeAccount.mutateAsync],
  );

  return {
    users: data ?? [],
    loading,
    loadError: loadError ? apiErrorMessage(loadError, "Could not load users.") : "",
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
    resetPassword: resetPasswordValue,
    setResetPassword: setResetPasswordValue,
    openPasswordReset,
    closePasswordReset,
    setUserPassword,
  };
}
