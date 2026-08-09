"use client";

import type { ContactHandle } from "@/lib/types";
import type { ContactEditDraft } from "./contactEdit";
import { ContactDetailsCard } from "./ContactDetailsCard";
import {
  ContactFormOverlay,
  type ContactFormAnchor,
} from "./ContactFormOverlay";
import { LabelsMenu, type LabelCheckState } from "./LabelsMenu";
import type { Dispatch, SetStateAction } from "react";

/** Props needed to render the shared participant contact form overlay. */
export type ParticipantContactFormView = {
  formOpen: boolean;
  editDraft: ContactEditDraft | null;
  setEditDraft: Dispatch<SetStateAction<ContactEditDraft | null>>;
  formAnchor: ContactFormAnchor | null;
  contactCreating: boolean;
  contactSaving: boolean;
  canSaveForm: boolean;
  draftMenuLabels: string[];
  draftLabelChecks: Record<string, LabelCheckState>;
  cancelContactForm: () => void;
  saveContactEdit: () => Promise<void>;
  saveContactCreate: () => Promise<void>;
  toggleDraftLabel: (name: string) => void;
  createAndAssignDraftLabel: (name: string) => void;
  clearDraftLabels: () => void;
};

export function ParticipantContactFormOverlay({
  form,
  titleId,
  phonesView = [],
  handlesView,
}: {
  form: ParticipantContactFormView;
  titleId: string;
  /** @deprecated Prefer `handlesView`. */
  phonesView?: string[];
  /** Handles with types for the closed-form view (rarely used; form renders open). */
  handlesView?: ContactHandle[];
}) {
  const {
    formOpen,
    editDraft,
    setEditDraft,
    formAnchor,
    contactCreating,
    contactSaving,
    canSaveForm,
    draftMenuLabels,
    draftLabelChecks,
    cancelContactForm,
    saveContactCreate,
    saveContactEdit,
    toggleDraftLabel,
    createAndAssignDraftLabel,
    clearDraftLabels,
  } = form;

  if (!formOpen || !editDraft) return null;

  return (
    <ContactFormOverlay
      anchor={formAnchor}
      titleId={titleId}
      title={contactCreating ? "Add new contact" : "Edit contact"}
      busy={contactSaving}
      onDismiss={cancelContactForm}
      footer={
        <>
          <button
            type="button"
            disabled={contactSaving}
            onClick={cancelContactForm}
            className="rounded-md bg-elevated px-3 py-1.5 text-[13px] text-text transition-colors hover:bg-hover disabled:opacity-50"
          >
            Cancel
          </button>
          <button
            type="button"
            disabled={contactSaving || (contactCreating && !canSaveForm)}
            onClick={() =>
              void (contactCreating ? saveContactCreate() : saveContactEdit())
            }
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
        draft={editDraft}
        onDraftChange={setEditDraft}
        labels={editDraft.labels}
        phonesView={phonesView}
        handlesView={handlesView}
        labelsEditor={
          <LabelsMenu
            allLabels={draftMenuLabels}
            checks={draftLabelChecks}
            disabled={contactSaving}
            onToggle={toggleDraftLabel}
            onCreate={createAndAssignDraftLabel}
            onClearAll={clearDraftLabels}
          />
        }
      />
    </ContactFormOverlay>
  );
}
