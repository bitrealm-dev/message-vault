"use client";

import { useState } from "react";
import { XIcon } from "./icons";

export function DeleteAccountDialog({
  open,
  username,
  deleting = false,
  onClose,
  onConfirm,
}: {
  open: boolean;
  username: string;
  deleting?: boolean;
  onClose: () => void;
  onConfirm: () => void;
}) {
  const [typedUsername, setTypedUsername] = useState("");
  const [wasOpen, setWasOpen] = useState(open);
  if (open !== wasOpen) {
    setWasOpen(open);
    if (open) setTypedUsername("");
  }

  if (!open) return null;

  const expected = username.trim();
  const matches = expected.length > 0 && typedUsername === expected;

  return (
    <div
      className="fixed inset-0 z-[200] flex items-center justify-center bg-scrim px-4"
      role="presentation"
      onClick={() => {
        if (!deleting) onClose();
      }}
    >
      <div
        role="dialog"
        aria-modal="true"
        aria-labelledby="mv-delete-account-dialog-title"
        className="relative w-full max-w-md rounded-xl border border-border bg-popover p-5 shadow-[0_16px_48px_rgba(0,0,0,0.5)]"
        onClick={(e) => e.stopPropagation()}
      >
        <button
          type="button"
          aria-label="Close"
          disabled={deleting}
          onClick={onClose}
          className="absolute top-4 right-4 flex h-7 w-7 items-center justify-center rounded-md text-muted transition-colors hover:bg-hover hover:text-text disabled:opacity-50"
        >
          <XIcon className="size-4" />
        </button>

        <h2
          id="mv-delete-account-dialog-title"
          className="pr-8 text-[16px] font-semibold text-text"
        >
          Delete your account?
        </h2>

        <p className="mt-3 text-[14px] leading-relaxed text-muted">
          This cannot be undone. Your messages, contacts, group chats, profile,
          and attachments will be permanently deleted.
        </p>

        <label className="mt-5 block">
          <span className="text-[14px] text-text">
            Type your user ID{" "}
            {expected ? (
              <span className="font-semibold text-text">{expected}</span>
            ) : null}{" "}
            to confirm.
          </span>
          <input
            type="text"
            value={typedUsername}
            onChange={(e) => setTypedUsername(e.target.value)}
            disabled={deleting}
            autoComplete="off"
            spellCheck={false}
            className="mt-2 w-full rounded-md border border-border bg-elevated px-3 py-2 text-[14px] text-text outline-none focus:border-accent disabled:opacity-50"
          />
        </label>

        <div className="mt-5 flex justify-end">
          <button
            type="button"
            disabled={deleting || !matches}
            onClick={onConfirm}
            className={`rounded-md border px-4 py-2 text-[13px] transition-colors ${
              matches && !deleting
                ? "border-red-500/40 bg-red-500/15 text-red-100 hover:bg-red-500/25"
                : "cursor-not-allowed border-border bg-elevated text-muted opacity-50"
            }`}
          >
            {deleting ? "Deleting…" : "Permanently delete my account"}
          </button>
        </div>
      </div>
    </div>
  );
}
