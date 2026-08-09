"use client";

import { useCallback, useRef, useState, type ReactNode } from "react";
import { ConfirmDialog } from "./ConfirmDialog";

type PendingConfirm = {
  title: string;
  confirmLabel?: string;
  resolve: (ok: boolean) => void;
};

/** Promise-based themed confirm; render `dialog` once in the tree. */
export function useConfirmDialog(): {
  confirm: (title: string, confirmLabel?: string) => Promise<boolean>;
  dialog: ReactNode;
  busy: boolean;
} {
  const [pending, setPending] = useState<PendingConfirm | null>(null);
  const pendingRef = useRef<PendingConfirm | null>(null);

  const confirm = useCallback((title: string, confirmLabel?: string) => {
    return new Promise<boolean>((resolve) => {
      const next = { title, confirmLabel, resolve };
      pendingRef.current = next;
      setPending(next);
    });
  }, []);

  const settle = useCallback((ok: boolean) => {
    const cur = pendingRef.current;
    pendingRef.current = null;
    setPending(null);
    cur?.resolve(ok);
  }, []);

  const dialog = pending ? (
    <ConfirmDialog
      title={pending.title}
      confirmLabel={pending.confirmLabel}
      onCancel={() => settle(false)}
      onConfirm={() => settle(true)}
    />
  ) : null;

  return { confirm, dialog, busy: pending != null };
}
