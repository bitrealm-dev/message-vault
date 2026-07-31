"use client";

import type { RefObject } from "react";
import { LockIcon, TrashMessagesIcon } from "./icons";

export function SearchMessageCtxMenu({
  menuRef,
  x,
  y,
  count,
  vaultReadOnly,
  saving,
  onDelete,
  onUnlockVault,
}: {
  menuRef: RefObject<HTMLDivElement | null>;
  x: number;
  y: number;
  count: number;
  vaultReadOnly: boolean;
  saving: boolean;
  onDelete: () => void;
  onUnlockVault?: () => void;
}) {
  return (
    <div
      ref={menuRef}
      className="fixed z-[100] min-w-[180px] rounded-lg border border-border bg-popover py-1 shadow-xl"
      style={{ left: x, top: y }}
    >
      {vaultReadOnly ? (
        <button
          type="button"
          className="flex w-full items-center gap-2.5 px-3 py-1.5 text-left text-[13px] text-text hover:bg-hover-strong"
          onClick={onUnlockVault}
        >
          <LockIcon className="size-5 shrink-0 opacity-80" />
          Unlock vault to edit
        </button>
      ) : (
        <button
          type="button"
          disabled={saving}
          className="flex w-full items-center gap-2.5 px-3 py-1.5 text-left text-[13px] text-text hover:bg-red-500/15 hover:text-red-300 disabled:opacity-50"
          onClick={onDelete}
        >
          <TrashMessagesIcon className="size-5 shrink-0 opacity-80" />
          {count === 1 ? "Delete message" : "Delete messages"}
        </button>
      )}
    </div>
  );
}
