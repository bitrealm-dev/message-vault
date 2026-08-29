import { useCallback } from "react";
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

  const deleteMessages = useCallback(
    (id: string) =>
      run(async () => {
        await apiClient.delete(`/v1/admin/users/${encodeURIComponent(id)}/messages`);
        reload();
      }),
    [run, reload],
  );

  const deleteUser = useCallback(
    (id: string) =>
      run(async () => {
        await apiClient.delete(`/v1/admin/users/${encodeURIComponent(id)}`);
        reload();
      }),
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
  };
}
