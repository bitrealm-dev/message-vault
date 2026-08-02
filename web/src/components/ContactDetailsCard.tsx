"use client";

import { formatPhoneDisplay } from "@/lib/phoneE164";
import type { ReactNode } from "react";
import {
  ContactPhoneList,
  displayLabelNames,
  type ContactEditDraft,
} from "./contactEdit";
import {
  PeopleGroupIcon,
  PersonDetailIcon,
  PhoneIcon,
} from "./icons";

const ICON_COL = "flex w-5 shrink-0 justify-center pt-0.5";
const FIELD_LABEL =
  "text-[11px] font-semibold tracking-wide text-muted uppercase";

function LabelNamesList({ names }: { names: string[] }) {
  if (names.length === 0) {
    return (
      <span className="truncate text-[13px] leading-5 text-muted">None</span>
    );
  }
  return (
    <div className="flex min-w-0 flex-wrap gap-1">
      {names.map((name) => (
        <span
          key={name}
          className="max-w-full truncate rounded bg-elevated px-1.5 py-0.5 text-[12px] font-medium text-text"
        >
          {name}
        </span>
      ))}
    </div>
  );
}

function FormSection({
  icon,
  label,
  children,
}: {
  icon: ReactNode;
  label: string;
  children: ReactNode;
}) {
  return (
    <div className="flex gap-3">
      <div className={ICON_COL}>{icon}</div>
      <div className="min-w-0 flex-1">
        <div className={FIELD_LABEL}>{label}</div>
        <div className="mt-1.5">{children}</div>
      </div>
    </div>
  );
}

export function ContactDetailsCard({
  formOpen,
  draft,
  onDraftChange,
  labels,
  phonesView,
  framed = true,
  labelsEditor,
  hideLabels = false,
}: {
  formOpen: boolean;
  draft: ContactEditDraft | null;
  onDraftChange?: (draft: ContactEditDraft) => void;
  labels: string[];
  /** Phones shown in view mode (when form is closed). */
  phonesView: string[];
  /** When false, skip outer card chrome and "Contact details" heading (for dialogs). */
  framed?: boolean;
  /** When set and form is open, replaces the static labels list (e.g. LabelsMenu). */
  labelsEditor?: ReactNode;
  /** Hide labels column (e.g. account identity “Me” edit). */
  hideLabels?: boolean;
}) {
  const shownLabels = displayLabelNames(labels);
  const phoneCount =
    formOpen && draft
      ? draft.phones.filter((p) => p.trim()).length
      : phonesView.length;
  const phoneLabel = phoneCount === 1 ? "Phone" : "Phones";
  const editing = Boolean(formOpen && draft && onDraftChange);

  const inputClass =
    "w-full rounded-md border border-border bg-elevated/40 px-2.5 py-1.5 text-[13px] text-text outline-none placeholder:text-muted focus:border-accent/60";

  const body = editing ? (
    <div className={`space-y-4 ${framed ? "mt-3" : ""}`}>
      <FormSection
        icon={<PersonDetailIcon className="size-5 shrink-0 text-muted" />}
        label="Name"
      >
        <div className="grid grid-cols-2 gap-2">
          <input
            type="text"
            value={draft!.firstName}
            onChange={(e) =>
              onDraftChange!({ ...draft!, firstName: e.target.value })
            }
            placeholder="First"
            className={inputClass}
          />
          <input
            type="text"
            value={draft!.lastName}
            onChange={(e) =>
              onDraftChange!({ ...draft!, lastName: e.target.value })
            }
            placeholder="Last"
            className={inputClass}
          />
        </div>
      </FormSection>

      {!hideLabels && (
        <FormSection
          icon={<PeopleGroupIcon className="size-5 shrink-0 text-muted" />}
          label="Labels"
        >
          <div className="flex min-w-0 flex-col gap-2">
            {labelsEditor}
            <LabelNamesList names={shownLabels} />
          </div>
        </FormSection>
      )}

      <FormSection
        icon={<PhoneIcon className="size-5 shrink-0 text-muted" />}
        label={phoneLabel}
      >
        <ContactPhoneList
          phones={draft!.phones}
          onChange={(phones) => onDraftChange!({ ...draft!, phones })}
        />
      </FormSection>
    </div>
  ) : (
    <div
      className={`${framed ? "mt-3" : ""} ${
        hideLabels ? "space-y-4" : "grid grid-cols-2 items-start gap-4"
      }`}
    >
      {!hideLabels && (
        <FormSection
          icon={<PeopleGroupIcon className="size-5 shrink-0 text-muted" />}
          label="Labels"
        >
          <LabelNamesList names={shownLabels} />
        </FormSection>
      )}

      <FormSection
        icon={<PhoneIcon className="size-5 shrink-0 text-muted" />}
        label={phoneLabel}
      >
        {phonesView.length > 0 ? (
          <div className="flex min-w-0 flex-col gap-0.5">
            {phonesView.map((phone) => (
              <span
                key={phone}
                className="truncate text-[13px] leading-5 text-text tabular-nums"
              >
                {formatPhoneDisplay(phone)}
              </span>
            ))}
          </div>
        ) : (
          <span className="truncate text-[13px] leading-5 text-muted">None</span>
        )}
      </FormSection>
    </div>
  );

  if (!framed) return body;

  return (
    <div className="rounded-xl border border-border bg-popover p-4 shadow-[0_8px_24px_rgba(0,0,0,0.35)]">
      <h2 className="text-[15px] font-semibold text-text">Contact details</h2>
      {body}
    </div>
  );
}
