import { useCallback, useEffect, useState } from "react";
import { useAuth } from "./auth";
import { THREAD_TAGS_CHANGED_EVENT, fetchThreadTags } from "./threadTags";

/** Live list of thread tags for the signed-in account. */
export function useThreadTags(): {
  tags: string[];
  loading: boolean;
  refresh: () => Promise<void>;
} {
  const { isAuthenticated, token } = useAuth();
  const [tags, setTags] = useState<string[]>([]);
  const [loading, setLoading] = useState(true);

  const refresh = useCallback(async () => {
    try {
      const next = await fetchThreadTags();
      setTags(next);
    } catch {
      /* Keep the last good list. A failed refresh must not hide existing tags. */
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
    globalThis.addEventListener(THREAD_TAGS_CHANGED_EVENT, onChange);
    return () => {
      globalThis.removeEventListener(THREAD_TAGS_CHANGED_EVENT, onChange);
    };
  }, [isAuthenticated, refresh, token]);

  return { tags, loading, refresh };
}
