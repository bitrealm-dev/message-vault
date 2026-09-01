import { useCallback, useState } from "react";
import { apiErrorMessage } from "../../lib/apiErrorMessage";
import { useAsyncAction } from "../../lib/useAsyncAction";
import { createApiToken, deleteApiToken, listApiTokens, renameApiToken } from "../../lib/vaultApi";
import { useVaultQuery } from "../../lib/vaultQuery";
import type { ApiTokenItem } from "./apiTokensUtils";

const fetchTokens = (signal: AbortSignal) =>
  listApiTokens({ signal }).then((res) => res.items ?? []);

/**
 * The list goes through `useVaultQuery` and each mutation through
 * `useAsyncAction` — the busy flag, the cleared-then-captured error and the
 * try/catch/finally around each call were previously written out three times
 * here, matching those hooks line for line.
 */
export function useApiTokens() {
  const [composing, setComposing] = useState(false);
  const [label, setLabel] = useState("");
  const [canImport, setCanImport] = useState(true);
  const [canExport, setCanExport] = useState(true);
  const [canDelete, setCanDelete] = useState(false);
  const [reveal, setReveal] = useState<{ label: string; token: string } | null>(null);
  const [revokeTarget, setRevokeTarget] = useState<ApiTokenItem | null>(null);
  const [renameTarget, setRenameTarget] = useState<ApiTokenItem | null>(null);
  const [renameLabel, setRenameLabel] = useState("");

  const {
    data,
    isPending: loading,
    error: loadError,
    refetch: reload,
  } = useVaultQuery(["api-tokens"], fetchTokens);
  const { busy, error: actionError, run, clearError } = useAsyncAction();

  const cancelCompose = useCallback(() => {
    setComposing(false);
    setLabel("");
    setCanImport(true);
    setCanExport(true);
    setCanDelete(false);
    clearError();
  }, [clearError]);

  const openRename = useCallback(
    (item: ApiTokenItem) => {
      setRenameTarget(item);
      setRenameLabel(item.label);
      clearError();
    },
    [clearError],
  );

  const closeRename = useCallback(() => {
    if (busy) return;
    setRenameTarget(null);
    setRenameLabel("");
  }, [busy]);

  const create = () => {
    const trimmed = label.trim();
    if (!trimmed) return Promise.resolve();
    return run(async () => {
      const res = await createApiToken({
        label: trimmed,
        can_import: canImport,
        can_export: canExport,
        can_delete: canDelete,
      });
      setLabel("");
      setCanImport(true);
      setCanExport(true);
      setCanDelete(false);
      setComposing(false);
      setReveal({ label: res.label, token: res.token });
      reload();
    });
  };

  const rename = () => {
    if (!renameTarget) return Promise.resolve();
    const trimmed = renameLabel.trim();
    if (!trimmed) return Promise.resolve();
    return run(async () => {
      await renameApiToken(renameTarget.id, { label: trimmed });
      setRenameTarget(null);
      setRenameLabel("");
      reload();
    });
  };

  const revoke = (item: ApiTokenItem) =>
    run(async () => {
      try {
        await deleteApiToken(item.id);
        reload();
      } finally {
        setRevokeTarget(null);
      }
    });

  return {
    items: data ?? [],
    loading,
    loadError: loadError ? apiErrorMessage(loadError, "Could not load API keys.") : "",
    busy,
    composing,
    setComposing,
    label,
    setLabel,
    canImport,
    setCanImport,
    canExport,
    setCanExport,
    canDelete,
    setCanDelete,
    actionError,
    reveal,
    setReveal,
    revokeTarget,
    setRevokeTarget,
    renameTarget,
    renameLabel,
    setRenameLabel,
    cancelCompose,
    openRename,
    closeRename,
    create,
    rename,
    revoke,
  };
}
