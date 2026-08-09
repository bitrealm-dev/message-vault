"use client";

/** Centered, themed confirm dialog (replaces window.confirm). */
export function ConfirmDialog({
  title,
  confirmLabel = "Delete",
  cancelLabel = "Cancel",
  saving = false,
  onConfirm,
  onCancel,
}: {
  title: string;
  confirmLabel?: string;
  cancelLabel?: string;
  saving?: boolean;
  onConfirm: () => void;
  onCancel: () => void;
}) {
  return (
    <div
      className="fixed inset-0 z-[200] flex items-center justify-center bg-scrim px-4"
      role="presentation"
      onClick={() => {
        if (!saving) onCancel();
      }}
    >
      <div
        role="dialog"
        aria-modal="true"
        aria-labelledby="mv-confirm-dialog-title"
        className="w-full max-w-md rounded-xl border border-border bg-popover p-5 shadow-[0_16px_48px_rgba(0,0,0,0.5)]"
        onClick={(e) => e.stopPropagation()}
      >
        <h2
          id="mv-confirm-dialog-title"
          className="text-[16px] font-semibold text-text"
        >
          {title}
        </h2>
        <div className="mt-4 flex justify-end gap-2">
          <button
            type="button"
            disabled={saving}
            onClick={onCancel}
            className="rounded-md bg-elevated px-3 py-1.5 text-[13px] text-text transition-colors hover:bg-hover disabled:opacity-50"
          >
            {cancelLabel}
          </button>
          <button
            type="button"
            disabled={saving}
            onClick={onConfirm}
            className="rounded-md bg-red-500/25 px-3 py-1.5 text-[13px] font-medium text-red-100 transition-colors hover:bg-red-500/35 disabled:opacity-50"
          >
            {confirmLabel}
          </button>
        </div>
      </div>
    </div>
  );
}
