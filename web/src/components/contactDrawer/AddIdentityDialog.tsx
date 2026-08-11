import { useEffect, useState } from "react";
import Button from "../Button";
import ModalShell from "../ModalShell";
import Select, { ListBoxItem, selectItemClassName } from "../Select";
import { HANDLE_SERVICE_OPTIONS } from "./contactDrawerTypes";

const fieldLabelClass = "mb-1 block text-[0.813rem] font-medium text-text";
const inputClass =
  "box-border w-full rounded border border-border bg-elevated px-3 py-2 text-[0.875rem] font-normal leading-none text-text outline-none focus:border-accent";
const selectTriggerClass =
  "!box-border !h-9 !min-h-9 !w-full !rounded !px-3 !py-0 !text-[0.875rem] !font-normal !leading-none !bg-elevated";
const selectValueClass = "!text-[0.875rem] !font-normal !leading-none";

export default function AddIdentityDialog({
  open,
  busy = false,
  onClose,
  onConfirm,
}: {
  open: boolean;
  busy?: boolean;
  onClose: () => void;
  onConfirm: (args: { handle: string; service: string }) => void;
}) {
  const [service, setService] = useState("phone");
  const [handle, setHandle] = useState("");

  useEffect(() => {
    if (open) {
      setService("phone");
      setHandle("");
    }
  }, [open]);

  const canSubmit = handle.trim().length > 0 && !busy;

  const submit = () => {
    if (!canSubmit) return;
    onConfirm({ handle: handle.trim(), service });
  };

  return (
    <ModalShell
      open={open}
      onOpenChange={(o) => {
        if (!o && !busy) onClose();
      }}
      dismissable={!busy}
      label="Add identity"
      maxWidth="24rem"
    >
      <button
        type="button"
        aria-label="Close"
        disabled={busy}
        onClick={onClose}
        className="absolute top-3 right-3 cursor-pointer border-none bg-transparent text-[1.25rem] leading-none text-muted disabled:cursor-not-allowed disabled:opacity-50"
      >
        ×
      </button>
      <h2 className="mb-4 pr-6 text-[1.125rem] font-semibold text-text">Add identity</h2>

      <label className="mb-4 block">
        <span className={fieldLabelClass}>Service</span>
        <Select
          selectedKey={service}
          onSelectionChange={(k) => setService(String(k))}
          aria-label="Service"
          isDisabled={busy}
          triggerClassName={selectTriggerClass}
          valueClassName={selectValueClass}
          popoverClassName="!z-[250]"
          className="block w-full min-w-0"
        >
          {HANDLE_SERVICE_OPTIONS.map((s) => (
            <ListBoxItem key={s.value} id={s.value} className={selectItemClassName}>
              {s.label}
            </ListBoxItem>
          ))}
        </Select>
      </label>

      <label className="block">
        <span className={fieldLabelClass}>Identity</span>
        <input
          type="text"
          value={handle}
          onChange={(e) => setHandle(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") {
              e.preventDefault();
              submit();
            }
          }}
          disabled={busy}
          autoFocus
          autoComplete="off"
          spellCheck={false}
          placeholder="Phone number or handle"
          className={inputClass}
        />
      </label>

      <div className="mt-5 flex justify-end gap-2">
        <Button onPress={onClose} isDisabled={busy}>
          Cancel
        </Button>
        <Button variant="primary" onPress={submit} isDisabled={!canSubmit}>
          {busy ? "Working…" : "OK"}
        </Button>
      </div>
    </ModalShell>
  );
}
