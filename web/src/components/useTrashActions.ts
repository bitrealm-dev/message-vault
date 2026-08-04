"use client";

import { isDeletionUiBlocked } from "@/lib/v1Capabilities";
import { useCallback, useState, type ReactNode } from "react";
import { useRouter } from "next/navigation";
import { useConfirmDialog } from "./useConfirmDialog";

type TrashStatusMessages = {
  trashedOne: string;
  trashedMany: (n: number) => string;
  restoredOne: string;
  restoredMany: (n: number) => string;
  deletedOne: string;
  deletedMany: (n: number) => string;
};

export function useTrashActions<TId extends string | number>(options: {
  endpoint: string;
  /** JSON body field name for the id (`handle` or `conversationId`). */
  idField: string;
  getTargets: (override?: TId) => TId[];
  canTrash: boolean;
  canRestoreOrDelete: boolean;
  /** Return confirm message, or null to skip confirm. */
  confirmTrash?: (targets: TId[]) => string | null;
  confirmPermanent?: (targets: TId[]) => string | null;
  status: TrashStatusMessages;
  setStatus: (s: string | null) => void;
  onRemoved: (targets: TId[]) => void;
  onDismissMenus?: () => void;
  /** After successful trash (default: router.refresh). */
  afterTrash?: () => void;
  afterRestore?: () => void;
  afterPermanent?: () => void;
  /** Called with trashed ids after a successful move-to-trash. */
  onTrashed?: (targets: TId[]) => void;
}): {
  saving: boolean;
  moveToTrash: (override?: TId) => Promise<void>;
  restoreFromTrash: (override?: TId) => Promise<void>;
  permanentlyDeleteFromTrash: (override?: TId) => Promise<void>;
  confirmDialog: ReactNode;
} {
  const {
    endpoint,
    idField,
    getTargets,
    canTrash,
    canRestoreOrDelete,
    confirmTrash,
    confirmPermanent,
    status,
    setStatus,
    onRemoved,
    onDismissMenus,
    afterTrash,
    afterRestore,
    afterPermanent,
    onTrashed,
  } = options;

  const router = useRouter();
  const [saving, setSaving] = useState(false);
  const { confirm: askConfirm, dialog: confirmDialog } = useConfirmDialog();

  const runLoop = useCallback(
    async (
      targets: TId[],
      method: "POST" | "DELETE",
      bodyExtra: Record<string, unknown> | undefined,
      successStatus: string,
      afterSuccess: (() => void) | undefined,
      failFallback: string,
      onSuccessTargets?: (targets: TId[]) => void,
    ) => {
      setSaving(true);
      onDismissMenus?.();
      try {
        for (const id of targets) {
          const res = await fetch(endpoint, {
            method,
            headers: { "Content-Type": "application/json" },
            body: JSON.stringify({ [idField]: id, ...bodyExtra }),
          });
          const data = await res.json();
          if (!res.ok) throw new Error(data.error ?? failFallback);
        }
        setStatus(successStatus);
        onRemoved(targets);
        onSuccessTargets?.(targets);
        (afterSuccess ?? (() => router.refresh()))();
      } catch (err) {
        console.error(err);
        setStatus(err instanceof Error ? err.message : failFallback);
        router.refresh();
      } finally {
        setSaving(false);
      }
    },
    [endpoint, idField, onDismissMenus, onRemoved, router, setStatus],
  );

  const moveToTrash = useCallback(
    async (override?: TId) => {
      if (isDeletionUiBlocked() || !canTrash) return;
      const targets = getTargets(override);
      if (targets.length === 0) return;
      const confirmMsg = confirmTrash?.(targets) ?? null;
      if (confirmMsg != null && !(await askConfirm(confirmMsg))) return;
      await runLoop(
        targets,
        "POST",
        undefined,
        targets.length === 1
          ? status.trashedOne
          : status.trashedMany(targets.length),
        afterTrash,
        "delete failed",
        onTrashed,
      );
    },
    [
      afterTrash,
      askConfirm,
      canTrash,
      confirmTrash,
      getTargets,
      onTrashed,
      runLoop,
      status.trashedMany,
      status.trashedOne,
    ],
  );

  const restoreFromTrash = useCallback(
    async (override?: TId) => {
      if (!canRestoreOrDelete) return;
      const targets = getTargets(override);
      if (targets.length === 0) return;
      await runLoop(
        targets,
        "DELETE",
        undefined,
        targets.length === 1
          ? status.restoredOne
          : status.restoredMany(targets.length),
        afterRestore,
        "undelete failed",
      );
    },
    [
      afterRestore,
      canRestoreOrDelete,
      getTargets,
      runLoop,
      status.restoredMany,
      status.restoredOne,
    ],
  );

  const permanentlyDeleteFromTrash = useCallback(
    async (override?: TId) => {
      if (isDeletionUiBlocked() || !canRestoreOrDelete) return;
      const targets = getTargets(override);
      if (targets.length === 0) return;
      const confirmMsg = confirmPermanent?.(targets) ?? null;
      if (confirmMsg != null && !(await askConfirm(confirmMsg, "Delete")))
        return;
      await runLoop(
        targets,
        "DELETE",
        { permanent: true },
        targets.length === 1
          ? status.deletedOne
          : status.deletedMany(targets.length),
        afterPermanent,
        "delete forever failed",
      );
    },
    [
      afterPermanent,
      askConfirm,
      canRestoreOrDelete,
      confirmPermanent,
      getTargets,
      runLoop,
      status.deletedMany,
      status.deletedOne,
    ],
  );

  return {
    saving,
    moveToTrash,
    restoreFromTrash,
    permanentlyDeleteFromTrash,
    confirmDialog,
  };
}
