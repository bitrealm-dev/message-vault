import { useCallback, useEffect, useRef, useState } from "react";
import type { UnlistenFn } from "@tauri-apps/api/event";
import { invokeCancel, onExtractEvents } from "../lib/tauri";

/**
 * Shared lifecycle for Tauri long-running jobs that emit extract:* events
 * (extract, format, and similar). Subscribes before invoke, tears down on
 * finish/error/unmount, and exposes cancel via the shared cancel command.
 */
export function useTauriJob(options?: {
  onError?: (msg: string) => void;
}): {
  running: boolean;
  log: string[];
  start: (invokeFn: () => Promise<void>, startErrorLabel: string) => Promise<void>;
  cancel: () => Promise<void>;
} {
  const onError = options?.onError;
  const [running, setRunning] = useState(false);
  const [log, setLog] = useState<string[]>([]);
  const unlistenRef = useRef<UnlistenFn | null>(null);

  const tearDown = useCallback(() => {
    unlistenRef.current?.();
    unlistenRef.current = null;
  }, []);

  useEffect(() => () => tearDown(), [tearDown]);

  const start = useCallback(
    async (invokeFn: () => Promise<void>, startErrorLabel: string) => {
      tearDown();
      setRunning(true);
      setLog([]);

      unlistenRef.current = await onExtractEvents({
        onLog: (line) => {
          setLog((prev) => [...prev, line]);
        },
        onFinished: (summary) => {
          setLog((prev) => [...prev, summary]);
          setRunning(false);
          tearDown();
        },
        onError: (err) => {
          setLog((prev) => {
            const next = [...prev, `Error: ${err.detail}`];
            if (err.user_message) next.push(err.user_message);
            return next;
          });
          setRunning(false);
          onError?.(err.user_message ?? err.detail);
          tearDown();
        },
      });

      try {
        await invokeFn();
      } catch (err) {
        setLog((prev) => [...prev, `${startErrorLabel}: ${err}`]);
        setRunning(false);
        tearDown();
      }
    },
    [onError, tearDown],
  );

  const cancel = useCallback(async () => {
    await invokeCancel();
  }, []);

  return { running, log, start, cancel };
}
