import { useEffect, useState } from "react";
import { apiClient } from "../../lib/api";
import type { CachedContactHandle } from "../../lib/contactDetailCache";
import {
  formatHandleServiceLabel,
  handleServiceSelectValue,
} from "./contactDrawerTypes";
import {
  conversationCount,
  type RemoveIdentityTarget,
} from "./handleTableLogic";

export function useHandleMutations({
  contactId,
  onHandlesChanged,
}: {
  contactId: string;
  onHandlesChanged: () => void;
}) {
  const [adding, setAdding] = useState(false);
  const [busy, setBusy] = useState(false);
  const [removeTarget, setRemoveTarget] = useState<RemoveIdentityTarget | null>(null);

  useEffect(() => {
    setAdding(false);
    setBusy(false);
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
    try {
      await apiClient.post(`/v1/export/contacts/${contactId}`, {
        remove_handle: { handle, service },
      });
      setRemoveTarget(null);
      onHandlesChanged();
    } catch {
      /* keep dialog open for retry */
    } finally {
      setBusy(false);
    }
  };

  const confirmAdd = async (args: { handle: string; service: string }) => {
    if (busy) return;
    setBusy(true);
    try {
      await apiClient.post(`/v1/export/contacts/${contactId}`, {
        add_handle: { handle: args.handle, service: args.service },
      });
      setAdding(false);
      onHandlesChanged();
    } catch {
      /* keep dialog open for retry */
    } finally {
      setBusy(false);
    }
  };

  return {
    adding,
    setAdding,
    busy,
    removeTarget,
    setRemoveTarget,
    requestRemoveHandle,
    confirmRemoveHandle,
    confirmAdd,
  };
}
