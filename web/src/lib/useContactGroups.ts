import { contactGroups } from "./contactGroups";
import { useNameCollection } from "./nameCollection";

/** Live list of contact groups for the signed-in account. */
export function useContactGroups(): {
  groups: string[];
  loading: boolean;
  refresh: () => Promise<void>;
} {
  const { names, loading, refresh } = useNameCollection(contactGroups);
  return { groups: names, loading, refresh };
}
