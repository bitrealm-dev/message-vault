"use client";

import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from "react";
import { useRouter } from "next/navigation";
import { redoCommand, undoCommand } from "./historyRunner";
import {
  HISTORY_MAX_DEPTH,
  HISTORY_TOAST_MS,
  redoToastTextForCommand,
  toastTextForCommand,
  undoToastTextForCommand,
  type HistoryCommand,
  type HistoryToast,
} from "./historyTypes";

type HistoryContextValue = {
  canUndo: boolean;
  canRedo: boolean;
  busy: boolean;
  toast: HistoryToast | null;
  undoLabel: string | null;
  redoLabel: string | null;
  /** Increments after a successful undo/redo so clients can refetch local state. */
  revision: number;
  push: (cmd: HistoryCommand) => void;
  undo: () => Promise<void>;
  redo: () => Promise<void>;
  clear: () => void;
  dismissToast: () => void;
};

const HistoryContext = createContext<HistoryContextValue | null>(null);

/**
 * Session undo/redo stack (max HISTORY_MAX_DEPTH).
 * TODO: clear after 30 minutes of inactivity once that timer is added.
 */
export function HistoryProvider({ children }: { children: ReactNode }) {
  const router = useRouter();
  const [past, setPast] = useState<HistoryCommand[]>([]);
  const [future, setFuture] = useState<HistoryCommand[]>([]);
  const [toast, setToast] = useState<HistoryToast | null>(null);
  const [busy, setBusy] = useState(false);
  const [revision, setRevision] = useState(0);
  const toastTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const busyRef = useRef(false);
  const pastRef = useRef(past);
  const futureRef = useRef(future);
  pastRef.current = past;
  futureRef.current = future;

  const dismissToast = useCallback(() => {
    if (toastTimerRef.current) {
      clearTimeout(toastTimerRef.current);
      toastTimerRef.current = null;
    }
    setToast(null);
  }, []);

  const showToast = useCallback(
    (text: string, showUndo: boolean) => {
      if (toastTimerRef.current) {
        clearTimeout(toastTimerRef.current);
        toastTimerRef.current = null;
      }
      setToast({ text, showUndo });
      toastTimerRef.current = setTimeout(() => {
        toastTimerRef.current = null;
        setToast(null);
      }, HISTORY_TOAST_MS);
    },
    [],
  );

  useEffect(() => {
    return () => {
      if (toastTimerRef.current) clearTimeout(toastTimerRef.current);
    };
  }, []);

  const push = useCallback(
    (cmd: HistoryCommand) => {
      setPast((prev) => {
        const next = [...prev, cmd];
        if (next.length > HISTORY_MAX_DEPTH) {
          return next.slice(next.length - HISTORY_MAX_DEPTH);
        }
        return next;
      });
      setFuture([]);
      showToast(toastTextForCommand(cmd), true);
    },
    [showToast],
  );

  const clear = useCallback(() => {
    setPast([]);
    setFuture([]);
    dismissToast();
  }, [dismissToast]);

  const undo = useCallback(async () => {
    if (busyRef.current) return;
    const cmd = pastRef.current[pastRef.current.length - 1];
    if (!cmd) return;
    busyRef.current = true;
    setBusy(true);
    dismissToast();
    try {
      await undoCommand(cmd);
      setPast((prev) => prev.slice(0, -1));
      setFuture((prev) => [...prev, cmd]);
      setRevision((n) => n + 1);
      router.refresh();
      showToast(undoToastTextForCommand(cmd), false);
    } catch (err) {
      console.error(err);
      showToast(
        err instanceof Error ? err.message : "Undo failed",
        false,
      );
    } finally {
      busyRef.current = false;
      setBusy(false);
    }
  }, [router, showToast, dismissToast]);

  const redo = useCallback(async () => {
    if (busyRef.current) return;
    const cmd = futureRef.current[futureRef.current.length - 1];
    if (!cmd) return;
    busyRef.current = true;
    setBusy(true);
    dismissToast();
    try {
      await redoCommand(cmd);
      setFuture((prev) => prev.slice(0, -1));
      setPast((prev) => {
        const next = [...prev, cmd];
        if (next.length > HISTORY_MAX_DEPTH) {
          return next.slice(next.length - HISTORY_MAX_DEPTH);
        }
        return next;
      });
      setRevision((n) => n + 1);
      router.refresh();
      showToast(redoToastTextForCommand(cmd), false);
    } catch (err) {
      console.error(err);
      showToast(
        err instanceof Error ? err.message : "Redo failed",
        false,
      );
    } finally {
      busyRef.current = false;
      setBusy(false);
    }
  }, [router, showToast, dismissToast]);

  const value = useMemo<HistoryContextValue>(
    () => ({
      canUndo: past.length > 0 && !busy,
      canRedo: future.length > 0 && !busy,
      busy,
      toast,
      undoLabel: past[past.length - 1]?.label ?? null,
      redoLabel: future[future.length - 1]?.label ?? null,
      revision,
      push,
      undo,
      redo,
      clear,
      dismissToast,
    }),
    [past, future, busy, toast, revision, push, undo, redo, clear, dismissToast],
  );

  return (
    <HistoryContext.Provider value={value}>{children}</HistoryContext.Provider>
  );
}

export function useHistory(): HistoryContextValue {
  const ctx = useContext(HistoryContext);
  if (!ctx) {
    throw new Error("useHistory must be used within HistoryProvider");
  }
  return ctx;
}

/** Safe for optional mount points that may render outside the provider in tests. */
export function useHistoryOptional(): HistoryContextValue | null {
  return useContext(HistoryContext);
}
