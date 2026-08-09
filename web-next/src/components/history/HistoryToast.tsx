"use client";

import { XIcon } from "../icons";
import { useHistory } from "./HistoryProvider";

/** Fixed bottom-center snackbar after undoable actions (15s). Escape does not dismiss. */
export function HistoryToast() {
  const { toast, undo, busy, dismissToast } = useHistory();
  if (!toast) return null;

  return (
    <div
      role="status"
      className="pointer-events-none fixed bottom-4 left-1/2 z-[100] w-max max-w-[min(100vw-2rem,28rem)] -translate-x-1/2"
    >
      <div className="pointer-events-auto flex items-center gap-2 rounded-lg border border-border bg-popover py-2 pr-2 pl-3.5 text-[13px] text-text shadow-[0_8px_24px_rgba(0,0,0,0.45)]">
        <span className="min-w-0 flex-1 truncate">{toast.text}</span>
        {toast.showUndo && (
          <button
            type="button"
            disabled={busy}
            onClick={() => void undo()}
            className="shrink-0 rounded-md bg-hover px-2.5 py-1 text-[12px] font-medium text-text transition-colors hover:bg-hover-strong disabled:opacity-40"
          >
            Undo
          </button>
        )}
        <button
          type="button"
          aria-label="Dismiss"
          onClick={dismissToast}
          className="flex size-7 shrink-0 items-center justify-center rounded-md text-muted transition-colors hover:bg-hover hover:text-text"
        >
          <XIcon className="size-3.5" />
        </button>
      </div>
    </div>
  );
}
