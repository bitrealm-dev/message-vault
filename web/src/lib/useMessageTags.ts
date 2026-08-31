import { messageTags } from "./messageTags";
import { useNameCollection } from "./nameCollection";

/** Live list of message tags for the signed-in account. */
export function useMessageTags(): {
  tags: string[];
  loading: boolean;
  refresh: () => Promise<void>;
} {
  const { names, loading, refresh } = useNameCollection(messageTags);
  return { tags: names, loading, refresh };
}
