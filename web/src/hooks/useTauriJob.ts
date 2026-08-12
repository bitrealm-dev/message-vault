import { useCallback, useEffect, useRef, useState } from "react";
import type { UnlistenFn } from "@tauri-apps/api/event";
import {
  awaitTauriJob,
  invokeCancel,
  onExtractEvents,
  type TauriJobResult,
} from "../lib/tauri";
import type { ImportIssueEvent, ImportProgressEvent } from "../lib/types";

export type TauriJobRunCallbacks = {
  onLog?: (line: string) => void;
  onProgress?: (event: ImportProgressEvent) => void;
  onIssue?: (event: ImportIssueEvent) => void;
};

/**
 * Shared lifecycle for Tauri long-running jobs that emit extract:* events
 * (extract, format, push, and similar). Subscribes before invoke, tears down on
 * finish/error/unmount, and exposes cancel via the shared cancel command.
 *
 * Use `start` for fire-and-forget UI (Extract/Format/Export). Use `run` when the
 * caller must await a parsed result and/or chain jobs (Import extract→push).
 */
export function useTauriJob(options?: {
  onError?: (msg: string) => void;
  onProgress?: (event: ImportProgressEvent) => void;
  onIssue?: (event: ImportIssueEvent) => void;
}): {
  running: boolean;
  /** True after a successful `extract:finished` for the current run. */
  finished: boolean;
  log: string[];
  start: (invokeFn: () => Promise<void>, startErrorLabel: string) => Promise<void>;
  /**
   * Await one Tauri job to completion. Tears down any prior `start` listeners,
   * then uses the same `awaitTauriJob` subscription path Import used to call
   * directly. Rejects on `extract:error` or invoke failure (does not call
   * `onError` — the caller handles the rejection).
   */
  run: (
    invokeFn: () => Promise<void>,
    callbacks?: TauriJobRunCallbacks,
  ) => Promise<TauriJobResult>;
  cancel: () => Promise<void>;
} {
  const onError = options?.onError;
  const onProgress = options?.onProgress;
  const onIssue = options?.onIssue;
  const [running, setRunning] = useState(false);
  const [finished, setFinished] = useState(false);
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
      setFinished(false);
      setLog([]);

      unlistenRef.current = await onExtractEvents({
        onLog: (line) => {
          setLog((prev) => [...prev, line]);
        },
        onProgress,
        onIssue,
        onFinished: (summary) => {
          setLog((prev) => [...prev, summary]);
          setRunning(false);
          setFinished(true);
          tearDown();
        },
        onError: (err) => {
          setLog((prev) => {
            const next = [...prev, `Error: ${err.detail}`];
            if (err.user_message) next.push(err.user_message);
            return next;
          });
          setRunning(false);
          setFinished(false);
          onError?.(err.user_message ?? err.detail);
          tearDown();
        },
      });

      try {
        await invokeFn();
      } catch (err) {
        setLog((prev) => [...prev, `${startErrorLabel}: ${err}`]);
        setRunning(false);
        setFinished(false);
        tearDown();
      }
    },
    [onError, onIssue, onProgress, tearDown],
  );

  const run = useCallback(
    async (
      invokeFn: () => Promise<void>,
      callbacks?: TauriJobRunCallbacks,
    ): Promise<TauriJobResult> => {
      // Drop any fire-and-forget listeners so only one subscription is active.
      tearDown();
      setRunning(true);
      setFinished(false);
      try {
        const result = await awaitTauriJob(
          invokeFn,
          callbacks?.onLog,
          callbacks?.onProgress ?? onProgress,
          callbacks?.onIssue ?? onIssue,
        );
        setFinished(true);
        return result;
      } catch (err) {
        setFinished(false);
        throw err;
      } finally {
        setRunning(false);
      }
    },
    [onIssue, onProgress, tearDown],
  );

  const cancel = useCallback(async () => {
    await invokeCancel();
  }, []);

  return { running, finished, log, start, run, cancel };
}
