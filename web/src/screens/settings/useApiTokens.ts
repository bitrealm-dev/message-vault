import { useCallback, useState } from "react";
import { apiClient } from "../../lib/api";
import { useAsyncAction } from "../../lib/useAsyncAction";
import { useResource } from "../../lib/useResource";
import type { ApiTokenItem } from "./apiTokensUtils";

const fetchTokens = (signal: AbortSignal) =>
  apiClient
    .get<{ items: ApiTokenItem[] }>("/v1/account/api-tokens", { signal })
    .then((res) => res.items ?? []);

/**
 * The list goes through `useResource` and each mutation through
 * `useAsyncAction` — the busy flag, the cleared-then-captured error and the
 * try/catch/finally around each call were previously written out three times
 * here, matching those hooks line for line.
 */
export function useApiTokens() {
  const [composing, setComposing] = useState(false);
  const [label, setLabel] = useState("");
  const [reveal, setReveal] = useState<{ label: string; token: string } | null>(null);
  const [revokeTarget, setRevokeTarget] = useState<ApiTokenItem | null>(null);
  const [renameTarget, setRenameTarget] = useState<ApiTokenItem | null>(null);
  const [renameLabel, setRenameLabel] = useState("");

  const {
    data,
    loading,
    error: loadError,
    reload,
  } = useResource("account/api-tokens", fetchTokens);
  const { busy, error: actionError, run, clearError } = useAsyncAction();

  const cancelCompose = useCallback(() => {
    setComposing(false);
    setLabel("");
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
      const res = await apiClient.post<{
        id: string;
        label: string;
        scopes: string;
        created_at: string;
        token: string;
      }>("/v1/account/api-tokens", { label: trimmed, scopes: "both" });
      setLabel("");
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
      await apiClient.patch(`/v1/account/api-tokens/${encodeURIComponent(renameTarget.id)}`, {
        label: trimmed,
      });
      setRenameTarget(null);
      setRenameLabel("");
      reload();
    });
  };

  const revoke = (item: ApiTokenItem) =>
    run(async () => {
      try {
        await apiClient.delete(`/v1/account/api-tokens/${encodeURIComponent(item.id)}`);
        reload();
      } finally {
        setRevokeTarget(null);
      }
    });

  return {
    items: data ?? [],
    loading,
    loadError,
    busy,
    composing,
    setComposing,
    label,
    setLabel,
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
