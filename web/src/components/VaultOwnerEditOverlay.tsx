"use client";

import type { VaultOwner } from "@/lib/vaultOwner";
import { phoneHandlesOnly } from "@/lib/handleKind";
import { useEffect, useState } from "react";
import {
  phonesForSave,
  seedContactEditDraft,
  type ContactEditDraft,
} from "./contactEdit";
import { ContactDetailsCard } from "./ContactDetailsCard";
import {
  ContactFormOverlay,
  contactFormAnchorFromRect,
  type ContactFormAnchor,
} from "./ContactFormOverlay";

/** Anchored popup to edit the vault owner (“Me”) name and phones. */
export function VaultOwnerEditOverlay({
  open,
  owner,
  anchor,
  onDismiss,
  onSaved,
}: {
  open: boolean;
  owner: VaultOwner;
  anchor: ContactFormAnchor | null;
  onDismiss: () => void;
  onSaved: (owner: VaultOwner) => void;
}) {
  const [draft, setDraft] = useState<ContactEditDraft | null>(null);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!open) {
      setDraft(null);
      setError(null);
      setSaving(false);
      return;
    }
    setDraft(
      seedContactEditDraft({
        firstName: owner.first_name,
        lastName: owner.last_name,
        phones: owner.phones,
        labels: [],
      }),
    );
    setError(null);
  }, [open, owner]);

  if (!open || !draft) return null;

  const phones = phoneHandlesOnly(phonesForSave(draft.phones));
  const canSave = draft.firstName.trim() !== "" && phones.length > 0;

  const save = async () => {
    if (!canSave || saving) return;
    setSaving(true);
    setError(null);
    try {
      const res = await fetch("/api/settings/account", {
        method: "PATCH",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          vaultOwner: {
            firstName: draft.firstName,
            lastName: draft.lastName,
            phones,
          },
        }),
      });
      const data = (await res.json()) as {
        error?: string;
        vaultOwner?: {
          firstName: string;
          lastName: string;
          displayName: string;
          phones: string[];
        };
      };
      if (!res.ok || !data.vaultOwner) {
        throw new Error(data.error ?? "Save failed");
      }
      onSaved({
        first_name: data.vaultOwner.firstName,
        last_name: data.vaultOwner.lastName,
        display_name: data.vaultOwner.displayName,
        phones: data.vaultOwner.phones,
        emails: owner.emails,
      });
      onDismiss();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Save failed");
    } finally {
      setSaving(false);
    }
  };

  return (
    <ContactFormOverlay
      anchor={anchor}
      titleId="mv-vault-owner-edit-title"
      title="Edit contact"
      busy={saving}
      onDismiss={onDismiss}
      footer={
        <>
          <button
            type="button"
            disabled={saving}
            onClick={onDismiss}
            className="rounded-md bg-elevated px-3 py-1.5 text-[13px] text-text transition-colors hover:bg-hover disabled:opacity-50"
          >
            Cancel
          </button>
          <button
            type="button"
            disabled={saving || !canSave}
            onClick={() => void save()}
            className="rounded-md bg-accent/25 px-3 py-1.5 text-[13px] font-medium text-text transition-colors hover:bg-accent/35 disabled:opacity-50"
          >
            Save
          </button>
        </>
      }
    >
      <ContactDetailsCard
        formOpen
        framed={false}
        hideLabels
        draft={draft}
        onDraftChange={setDraft}
        labels={[]}
        phonesView={owner.phones}
      />
      {error && (
        <p className="mt-3 text-[12px] text-danger" role="alert">
          {error}
        </p>
      )}
    </ContactFormOverlay>
  );
}

export { contactFormAnchorFromRect };
export type { ContactFormAnchor };
