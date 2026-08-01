"use client";

import { useState } from "react";
import { XIcon } from "./icons";

export function ApiTokenRevealDialog({
  open,
  token,
  onClose,
}: {
  open: boolean;
  token: string;
  onClose: () => void;
}) {
  const [copied, setCopied] = useState(false);

  if (!open) return null;

  const copy = async () => {
    try {
      await navigator.clipboard.writeText(token);
      setCopied(true);
      window.setTimeout(() => setCopied(false), 2000);
    } catch {
      // ignore
    }
  };

  return (
    <div
      className="fixed inset-0 z-[200] flex items-center justify-center bg-scrim px-4"
      role="presentation"
      onClick={onClose}
    >
      <div
        role="dialog"
        aria-modal="true"
        aria-labelledby="mv-api-token-reveal-title"
        className="relative w-full max-w-md rounded-xl border border-border bg-popover p-5 shadow-[0_16px_48px_rgba(0,0,0,0.5)]"
        onClick={(e) => e.stopPropagation()}
      >
        <button
          type="button"
          aria-label="Close"
          onClick={onClose}
          className="absolute top-4 right-4 flex h-7 w-7 items-center justify-center rounded-md text-muted transition-colors hover:bg-hover hover:text-text"
        >
          <XIcon className="size-4" />
        </button>

        <h2
          id="mv-api-token-reveal-title"
          className="pr-8 text-[16px] font-semibold text-text"
        >
          Your API token
        </h2>
        <p className="mt-2 text-[13px] text-muted">
          Copy this token now. You won’t be able to view it again.
        </p>

        <code className="mt-4 block break-all rounded-md border border-border bg-elevated px-3 py-2 font-mono text-[12px] text-text">
          {token}
        </code>

        <div className="mt-4 flex justify-end gap-2">
          <button
            type="button"
            onClick={() => void copy()}
            className="rounded-md border border-border bg-elevated px-3 py-2 text-[13px] text-text transition-colors hover:bg-hover"
          >
            {copied ? "Copied" : "Copy"}
          </button>
          <button
            type="button"
            onClick={onClose}
            className="rounded-md border border-border bg-elevated px-3 py-2 text-[13px] text-text transition-colors hover:bg-hover"
          >
            Done
          </button>
        </div>
      </div>
    </div>
  );
}
