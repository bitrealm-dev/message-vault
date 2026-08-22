import { useCallback, useEffect, useState } from "react";
import { useAuth } from "./auth";
import { CONTACT_GROUPS_CHANGED_EVENT, fetchContactGroups } from "./contactGroups";

/** Live list of contact groups for the signed-in account. */
export function useContactGroups(): {
  groups: string[];
  loading: boolean;
  refresh: () => Promise<void>;
} {
  const { isAuthenticated, token } = useAuth();
  const [groups, setGroups] = useState<string[]>([]);
  const [loading, setLoading] = useState(true);

  const refresh = useCallback(async () => {
    try {
      const next = await fetchContactGroups();
      setGroups(next);
    } catch {
      /* Keep the last good list. A failed refresh must not hide existing groups. */
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    if (!isAuthenticated || !token) return;
    void refresh();
    const onChange = () => {
      void refresh();
    };
    globalThis.addEventListener(CONTACT_GROUPS_CHANGED_EVENT, onChange);
    return () => {
      globalThis.removeEventListener(CONTACT_GROUPS_CHANGED_EVENT, onChange);
    };
  }, [isAuthenticated, refresh, token]);

  return { groups, loading, refresh };
}
