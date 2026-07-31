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

const ICON_COL = "flex w-5 shrink-0 justify-center pt-[3px]";

function LabelNamesList({ names }: { names: string[] }) {
  if (names.length === 0) {
    return (
      <span className="truncate text-[13px] leading-5 text-muted">None</span>
    );
  }
  return (
    <>
      {names.map((name) => (
        <span
          key={name}
          className="truncate text-[13px] leading-5 text-text"
        >
          {name}
        </span>
      ))}
    </>
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
  /** Hide labels column (e.g. vault owner “Me” edit). */
  hideLabels?: boolean;
}) {
  const shownLabels = displayLabelNames(labels);
  const phoneCount =
    formOpen && draft
      ? draft.phones.filter((p) => p.trim()).length
      : phonesView.length;
  const phoneLabel = phoneCount === 1 ? "Phone" : "Phones";
  const editing = Boolean(formOpen && draft && onDraftChange);

  const body = editing ? (
    <div className={framed ? "mt-3" : undefined}>
      <div className="flex gap-3">
        <div className={ICON_COL}>
          <PersonDetailIcon className="size-5 shrink-0 text-muted" />
        </div>
        <div className="min-w-0 flex-1">
          <div className="text-[11px] leading-4 tracking-wide text-muted">
            Name
          </div>
          <div className="mt-0.5 grid grid-cols-2 items-start gap-2">
            <input
              type="text"
              value={draft!.firstName}
              onChange={(e) =>
                onDraftChange!({ ...draft!, firstName: e.target.value })
              }
              placeholder="First"
              className="rounded-md border border-border bg-transparent px-2 py-1 text-[13px] text-text outline-none placeholder:text-muted focus:border-accent/60"
            />
            <input
              type="text"
              value={draft!.lastName}
              onChange={(e) =>
                onDraftChange!({ ...draft!, lastName: e.target.value })
              }
              placeholder="Last"
              className="rounded-md border border-border bg-transparent px-2 py-1 text-[13px] text-text outline-none placeholder:text-muted focus:border-accent/60"
            />
            {!hideLabels && (
              <div className="flex min-w-0 flex-col gap-1.5">
                {labelsEditor}
                <div className="flex min-w-0 flex-col gap-0.5">
                  <LabelNamesList names={shownLabels} />
                </div>
              </div>
            )}
            <div
              className={`flex min-w-0 items-start gap-2 ${
                hideLabels ? "col-span-2" : ""
              }`}
            >
              <div className="flex shrink-0 justify-center pt-[5px]">
                <PhoneIcon className="size-5 shrink-0 text-muted" />
              </div>
              <div className="min-w-0 flex-1">
                <ContactPhoneList
                  phones={draft!.phones}
                  onChange={(phones) =>
                    onDraftChange!({ ...draft!, phones })
                  }
                />
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>
  ) : (
    <div className={framed ? "mt-3" : undefined}>
      <div
        className={
          hideLabels
            ? "flex min-w-0 gap-3"
            : "grid grid-cols-2 items-start gap-4"
        }
      >
        {!hideLabels && (
          <div className="flex min-w-0 gap-3">
            <div className={ICON_COL}>
              <PeopleGroupIcon className="size-5 shrink-0 text-muted" />
            </div>
            <div className="min-w-0 flex-1">
              <div className="text-[11px] leading-4 tracking-wide text-muted">
                Labels
              </div>
              <div className="mt-0.5 flex min-h-5 min-w-0 flex-col gap-0.5">
                <LabelNamesList names={shownLabels} />
              </div>
            </div>
          </div>
        )}

        <div className="flex min-w-0 gap-3">
          <div className={ICON_COL}>
            <PhoneIcon className="size-5 shrink-0 text-muted" />
          </div>
          <div className="min-w-0 flex-1">
            <div className="text-[11px] leading-4 tracking-wide text-muted">
              {phoneLabel}
            </div>
            <div className="mt-0.5 min-h-5 min-w-0">
              {phonesView.length > 0 ? (
                <div className="flex min-w-0 flex-col gap-0.5">
                  {phonesView.map((phone) => (
                    <span
                      key={phone}
                      className="truncate text-[13px] leading-5 text-text"
                    >
                      {formatPhoneDisplay(phone)}
                    </span>
                  ))}
                </div>
              ) : (
                <span className="truncate text-[13px] leading-5 text-muted">
                  None
                </span>
              )}
            </div>
          </div>
        </div>
      </div>
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
