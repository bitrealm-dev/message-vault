import Button from "../../components/Button";
import ModalShell, { DialogFooter } from "../../components/ModalShell";
import TextField from "../../components/TextField";

export function ApiTokenCreateForm({
  label,
  busy,
  onLabelChange,
  onSave,
  onCancel,
}: {
  label: string;
  busy: boolean;
  onLabelChange: (value: string) => void;
  onSave: () => void;
  onCancel: () => void;
}) {
  return (
    <div className="mb-3 flex flex-wrap items-end gap-2 rounded-xl border border-border bg-elevated p-3">
      <TextField
        value={label}
        onChange={onLabelChange}
        placeholder="Enter API key name…"
        isDisabled={busy}
        aria-label="API key name"
        className="min-w-[12rem] flex-1"
        onKeyDown={(e) => {
          if (e.key === "Enter") {
            e.preventDefault();
            onSave();
          }
          if (e.key === "Escape") {
            e.preventDefault();
            onCancel();
          }
        }}
      />
      <span className="pb-2.5 text-[0.75rem] text-muted">Import / Export</span>
      <Button
        variant="secondary"
        disabled={busy || !label.trim()}
        onClick={onSave}
        className="!px-3 !py-1.5 !text-[0.75rem]"
      >
        Save
      </Button>
      <Button
        variant="secondary"
        disabled={busy}
        onClick={onCancel}
        className="!px-3 !py-1.5 !text-[0.75rem]"
      >
        Cancel
      </Button>
    </div>
  );
}

export function ApiTokenRenameDialog({
  open,
  busy,
  renameLabel,
  onRenameLabelChange,
  onClose,
  onSave,
}: {
  open: boolean;
  busy: boolean;
  renameLabel: string;
  onRenameLabelChange: (value: string) => void;
  onClose: () => void;
  onSave: () => void;
}) {
  return (
    <ModalShell
      open={open}
      onOpenChange={(o) => {
        if (!o) onClose();
      }}
      dismissable={!busy}
      label="Rename API key"
      title="Rename API key"
      onClose={onClose}
      closeDisabled={busy}
      maxWidth="24rem"
    >
      <p className="mb-3 text-[0.813rem] text-muted">
        Choose a name you will recognize later. The secret value does not change.
      </p>
      <TextField
        label="Name"
        value={renameLabel}
        onChange={onRenameLabelChange}
        isDisabled={busy}
        autoFocus
        onKeyDown={(e) => {
          if (e.key === "Enter") {
            e.preventDefault();
            onSave();
          }
        }}
      />
      <DialogFooter>
        <Button onPress={onClose} isDisabled={busy}>
          Cancel
        </Button>
        <Button variant="primary" onPress={onSave} isDisabled={busy || !renameLabel.trim()}>
          {busy ? "Saving…" : "Save"}
        </Button>
      </DialogFooter>
    </ModalShell>
  );
}
