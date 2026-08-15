import { useCallback, useEffect, useState } from "react";
import {
  CONTACT_GROUPS_CHANGED_EVENT,
  fetchContactGroups,
} from "./contactGroups";

/** Live list of contact groups for the signed-in account. */
export function useContactGroups(): {
  groups: string[];
  loading: boolean;
  refresh: () => Promise<void>;
} {
  const [groups, setGroups] = useState<string[]>([]);
  const [loading, setLoading] = useState(true);

  const refresh = useCallback(async () => {
    try {
      const next = await fetchContactGroups();
      setGroups(next);
    } catch {
      setGroups([]);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void refresh();
    const onChange = () => {
      void refresh();
    };
    globalThis.addEventListener(CONTACT_GROUPS_CHANGED_EVENT, onChange);
    return () => {
      globalThis.removeEventListener(CONTACT_GROUPS_CHANGED_EVENT, onChange);
    };
  }, [refresh]);

  return { groups, loading, refresh };
}
