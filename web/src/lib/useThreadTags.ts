import { useNameCollection } from "./nameCollection";
import { threadTags } from "./threadTags";

/** Live list of thread tags for the signed-in account. */
export function useThreadTags(): {
  tags: string[];
  loading: boolean;
  refresh: () => Promise<void>;
} {
  const { names, loading, refresh } = useNameCollection(threadTags);
  return { tags: names, loading, refresh };
}
