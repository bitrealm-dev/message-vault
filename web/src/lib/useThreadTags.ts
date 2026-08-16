import { useCallback, useEffect, useState } from "react";
import { THREAD_TAGS_CHANGED_EVENT, fetchThreadTags } from "./threadTags";

/** Live list of thread tags for the signed-in account. */
export function useThreadTags(): {
  tags: string[];
  loading: boolean;
  refresh: () => Promise<void>;
} {
  const [tags, setTags] = useState<string[]>([]);
  const [loading, setLoading] = useState(true);

  const refresh = useCallback(async () => {
    try {
      const next = await fetchThreadTags();
      setTags(next);
    } catch {
      setTags([]);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void refresh();
    const onChange = () => {
      void refresh();
    };
    globalThis.addEventListener(THREAD_TAGS_CHANGED_EVENT, onChange);
    return () => {
      globalThis.removeEventListener(THREAD_TAGS_CHANGED_EVENT, onChange);
    };
  }, [refresh]);

  return { tags, loading, refresh };
}
