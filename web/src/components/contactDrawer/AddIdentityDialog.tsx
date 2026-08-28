import { useEffect, useId, useState } from "react";
import {
  CONTACT_IDENTITY_SERVICE_OPTIONS,
  CONTACT_IDENTITY_SERVICES,
  type ContactIdentityService,
  handleServiceSelectValue,
} from "../../lib/handleService";
import { parseSelectKey } from "../../lib/selectKey";
import { Z_POPOVER_IN_MODAL } from "../../lib/zLayers";
import Button from "../Button";
import ModalShell, { DialogError, DialogFooter } from "../ModalShell";
import Select, { ListBoxItem, selectItemClassName } from "../Select";

const fieldLabelClass = "mb-1 block text-[0.813rem] font-medium text-text";
const inputClass =
  "box-border w-full rounded border border-border bg-elevated px-3 py-2 text-[0.875rem] font-normal leading-none text-text outline-none focus:border-accent";
const selectTriggerClass =
  "!box-border !h-9 !min-h-9 !w-full !rounded !px-3 !py-0 !text-[0.875rem] !font-normal !leading-none !bg-elevated";
const selectValueClass = "!text-[0.875rem] !font-normal !leading-none";

function identityAlreadyExists(
  existing: { handle: string; service: string | null }[],
  handle: string,
  service: string,
): boolean {
  const needle = handle.trim().toLowerCase();
  if (!needle) return false;
  const platform = handleServiceSelectValue(handle, service);
  return existing.some(
    (row) =>
      row.handle.trim().toLowerCase() === needle &&
      handleServiceSelectValue(row.handle, row.service) === platform,
  );
}

export default function AddIdentityDialog({
  open,
  busy = false,
  error = "",
  existingHandles = [],
  onClose,
  onConfirm,
}: {
  open: boolean;
  busy?: boolean;
  /** Why the last submit failed. The dialog stays open so it can be retried. */
  error?: string;
  existingHandles?: { handle: string; service: string | null }[];
  onClose: () => void;
  onConfirm: (args: { handle: string; service: string }) => void;
}) {
  const [service, setService] = useState<ContactIdentityService>("phone");
  const [handle, setHandle] = useState("");
  const serviceId = useId();

  useEffect(() => {
    if (open) {
      setService("phone");
      setHandle("");
    }
  }, [open]);

  const trimmed = handle.trim();
  const duplicate = identityAlreadyExists(existingHandles, handle, service);
  const canSubmit = trimmed.length > 0 && !duplicate && !busy;

  const submit = () => {
    if (!canSubmit) return;
    onConfirm({ handle: trimmed, service });
  };

  return (
    <ModalShell
      open={open}
      onOpenChange={(o) => {
        if (!o && !busy) onClose();
      }}
      dismissable={!busy}
      label="Add identity"
      title="Add identity"
      onClose={onClose}
      closeDisabled={busy}
      maxWidth="24rem"
    >
      <div className="mt-4 mb-4">
        <label htmlFor={serviceId} className={fieldLabelClass}>
          Service
        </label>
        <Select
          id={serviceId}
          selectedKey={service}
          onSelectionChange={(k) => {
            const next = parseSelectKey(k, CONTACT_IDENTITY_SERVICES);
            if (next) setService(next);
          }}
          aria-label="Service"
          isDisabled={busy}
          triggerClassName={selectTriggerClass}
          valueClassName={selectValueClass}
          popoverClassName={Z_POPOVER_IN_MODAL}
          className="block w-full min-w-0"
        >
          {CONTACT_IDENTITY_SERVICE_OPTIONS.map((s) => (
            <ListBoxItem key={s.value} id={s.value} className={selectItemClassName}>
              {s.label}
            </ListBoxItem>
          ))}
        </Select>
      </div>

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
          autoComplete="off"
          spellCheck={false}
          placeholder="Enter a user id, phone number, or similar"
          aria-invalid={duplicate || undefined}
          aria-describedby={duplicate ? "add-identity-duplicate" : undefined}
          className={inputClass}
        />
      </label>
      {duplicate ? (
        <p id="add-identity-duplicate" className="mt-2 text-[0.813rem] text-danger" role="alert">
          Identity already exists
        </p>
      ) : null}

      <DialogError message={error} />

      <DialogFooter>
        <Button onPress={onClose} isDisabled={busy}>
          Cancel
        </Button>
        <Button variant="primary" onPress={submit} isDisabled={!canSubmit}>
          {busy ? "Working…" : "OK"}
        </Button>
      </DialogFooter>
    </ModalShell>
  );
}
