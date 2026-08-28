import { useEffect, useState } from "react";
import { apiClient } from "../../lib/api";
import type { CachedContactHandle } from "../../lib/contactDetailCache";
import { formatHandleServiceLabel, handleServiceSelectValue } from "./contactDrawerTypes";
import { conversationCount, type RemoveIdentityTarget } from "./handleTableLogic";

export function useHandleMutations({
  contactId,
  onHandlesChanged,
}: {
  contactId: string;
  onHandlesChanged: () => void;
}) {
  const [adding, setAdding] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [removeTarget, setRemoveTarget] = useState<RemoveIdentityTarget | null>(null);

  useEffect(() => {
    void contactId;
    setAdding(false);
    setBusy(false);
    setError("");
    setRemoveTarget(null);
  }, [contactId]);

  const requestRemoveHandle = (h: CachedContactHandle) => {
    if (busy) return;
    setRemoveTarget({
      handle: h.handle,
      service: h.service,
      serviceLabel: formatHandleServiceLabel(h.handle, h.service),
      threadCount: conversationCount(h),
    });
  };

  const confirmRemoveHandle = async () => {
    if (!removeTarget || busy) return;
    const handle = removeTarget.handle;
    const service = handleServiceSelectValue(handle, removeTarget.service);
    setBusy(true);
    setError("");
    try {
      await apiClient.post(`/v1/export/contacts/${contactId}`, {
        remove_handle: { handle, service },
      });
      setRemoveTarget(null);
      onHandlesChanged();
    } catch (e: unknown) {
      // Keep the dialog open for retry, but say why it did not go through.
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  const confirmAdd = async (args: { handle: string; service: string }) => {
    if (busy) return;
    setBusy(true);
    setError("");
    try {
      await apiClient.post(`/v1/export/contacts/${contactId}`, {
        add_handle: { handle: args.handle, service: args.service },
      });
      setAdding(false);
      onHandlesChanged();
    } catch (e: unknown) {
      // Keep the dialog open for retry, but say why it did not go through.
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  return {
    adding,
    setAdding,
    busy,
    error,
    removeTarget,
    setRemoveTarget,
    requestRemoveHandle,
    confirmRemoveHandle,
    confirmAdd,
  };
}
