import ModalShell from "./ModalShell";
import Button from "./Button";

export default function ConfirmDialog({
  open,
  title,
  body,
  confirmLabel = "OK",
  danger = false,
  busy = false,
  onConfirm,
  onClose,
}: {
  open: boolean;
  title: string;
  body: React.ReactNode;
  confirmLabel?: string;
  danger?: boolean;
  busy?: boolean;
  onConfirm: () => void;
  onClose: () => void;
}) {
  return (
    <ModalShell
      open={open}
      onOpenChange={(o) => { if (!o && !busy) onClose(); }}
      dismissable={!busy}
      label={title}
      maxWidth="24rem"
    >
      <h2 className="mb-2 text-[1.125rem] font-semibold text-text">{title}</h2>
      {typeof body === "string"
        ? <p className="mt-3 text-[0.875rem] leading-relaxed text-muted">{body}</p>
        : body}
      <div className="mt-5 flex justify-end gap-2">
        <Button onPress={onClose} isDisabled={busy}>Cancel</Button>
        <Button variant={danger ? "danger" : "primary"} onPress={onConfirm} isDisabled={busy}>
          {busy ? "Working…" : confirmLabel}
        </Button>
      </div>
    </ModalShell>
  );
}
