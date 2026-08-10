import { useEffect, useState } from "react";
import ModalShell from "./ModalShell";
import Button from "./Button";

export default function ApiTokenRevealDialog({
  open,
  label,
  token,
  onClose,
}: {
  open: boolean;
  label: string;
  token: string;
  onClose: () => void;
}) {
  const [copied, setCopied] = useState(false);

  useEffect(() => {
    if (open) setCopied(false);
  }, [open]);

  const copy = async () => {
    try {
      await navigator.clipboard.writeText(token);
      setCopied(true);
    } catch {
      setCopied(false);
    }
  };

  return (
    <ModalShell
      open={open}
      onOpenChange={(o) => {
        if (!o) onClose();
      }}
      label="API token created"
    >
      <button
        type="button"
        aria-label="Close"
        onClick={onClose}
        className="absolute top-3 right-3 cursor-pointer border-none bg-transparent text-[1.25rem] leading-none text-muted"
      >
        ×
      </button>

      <h3 className="mb-2 pr-6 text-[1.05rem] text-text">API token created</h3>
      <p className="mb-3 text-[0.813rem] text-muted">
        Copy this secret for <strong className="text-text">{label}</strong> now.
        It will not be shown again.
      </p>
      <div className="mb-3 rounded border border-border bg-elevated px-2.5 py-2 font-mono text-[0.75rem] break-all text-text">
        {token}
      </div>
      <div className="flex justify-end gap-2">
        <Button variant="ghost" onClick={onClose}>
          Done
        </Button>
        <Button variant="primary" onClick={() => void copy()}>
          {copied ? "Copied" : "Copy"}
        </Button>
      </div>
    </ModalShell>
  );
}
