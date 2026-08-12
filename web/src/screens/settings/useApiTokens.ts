import { useCallback, useEffect, useState } from "react";
import { apiClient } from "../../lib/api";
import type { ApiTokenItem } from "./apiTokensUtils";

export function useApiTokens() {
  const [items, setItems] = useState<ApiTokenItem[]>([]);
  const [loadError, setLoadError] = useState("");
  const [busy, setBusy] = useState(false);
  const [composing, setComposing] = useState(false);
  const [label, setLabel] = useState("");
  const [actionError, setActionError] = useState("");
  const [reveal, setReveal] = useState<{ label: string; token: string } | null>(null);
  const [revokeTarget, setRevokeTarget] = useState<ApiTokenItem | null>(null);
  const [renameTarget, setRenameTarget] = useState<ApiTokenItem | null>(null);
  const [renameLabel, setRenameLabel] = useState("");

  const reload = useCallback(async () => {
    setLoadError("");
    try {
      const res = await apiClient.get<{ items: ApiTokenItem[] }>("/v1/account/api-tokens");
      setItems(res.items ?? []);
    } catch (e) {
      setLoadError(e instanceof Error ? e.message : String(e));
    }
  }, []);

  useEffect(() => {
    void reload();
  }, [reload]);

  const cancelCompose = () => {
    setComposing(false);
    setLabel("");
    setActionError("");
  };

  const openRename = (item: ApiTokenItem) => {
    setRenameTarget(item);
    setRenameLabel(item.label);
    setActionError("");
  };

  const closeRename = () => {
    if (busy) return;
    setRenameTarget(null);
    setRenameLabel("");
  };

  const create = async () => {
    const trimmed = label.trim();
    if (!trimmed) return;
    setBusy(true);
    setActionError("");
    try {
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
      await reload();
    } catch (e) {
      setActionError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  const rename = async () => {
    if (!renameTarget) return;
    const trimmed = renameLabel.trim();
    if (!trimmed) return;
    setBusy(true);
    setActionError("");
    try {
      await apiClient.patch(`/v1/account/api-tokens/${encodeURIComponent(renameTarget.id)}`, {
        label: trimmed,
      });
      setRenameTarget(null);
      setRenameLabel("");
      await reload();
    } catch (e) {
      setActionError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  const revoke = async (item: ApiTokenItem) => {
    setBusy(true);
    setActionError("");
    try {
      await apiClient.delete(`/v1/account/api-tokens/${encodeURIComponent(item.id)}`);
      await reload();
    } catch (e) {
      setActionError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
      setRevokeTarget(null);
    }
  };

  return {
    items,
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
