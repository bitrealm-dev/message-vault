"use client";

import type { CollapsedGroupConversation } from "@/lib/groupChatList";
import { formatPhoneDisplay } from "@/lib/phoneE164";
import type { ContactDetail, ContactListItem } from "@/lib/types";
import {
  collapsedParticipantLabels,
  GroupNameSep,
} from "./GroupConversationRow";
import { useDateTimeFormat } from "./useDateTimeFormat";

export function browseSelectionSummaryFlags(options: {
  hasSelection: boolean;
  hasGroupSelection: boolean;
  activeThread: string | null;
  contactId: number | null;
  detail: ContactDetail | null;
  contacts: ContactListItem[];
}): {
  showContactsCard: boolean;
  showGroupsCard: boolean;
  showFocusContactCard: boolean;
  showThreadPane: boolean;
  focusedContact: {
    id: number;
    displayName: string;
    preferredHandle: string | null;
  } | null;
} {
  const {
    hasSelection,
    hasGroupSelection,
    activeThread,
    contactId,
    detail,
    contacts,
  } = options;

  const showContactsCard =
    hasSelection &&
    (hasGroupSelection || !(activeThread?.startsWith("gfull-")));
  const showGroupsCard = hasGroupSelection;
  const focusedContact =
    contactId != null
      ? detail?.id === contactId
        ? detail
        : (contacts.find((c) => c.id === contactId) ?? null)
      : null;
  const showFocusContactCard =
    hasGroupSelection && !hasSelection && focusedContact != null;
  const nothingFocused =
    !hasSelection && !hasGroupSelection && contactId == null;
  const showThreadPane =
    !nothingFocused &&
    !showGroupsCard &&
    !(hasSelection && !(activeThread?.startsWith("gfull-")));

  return {
    showContactsCard,
    showGroupsCard,
    showFocusContactCard,
    showThreadPane,
    focusedContact,
  };
}

export function BrowseSelectionSummary({
  showContactsCard,
  showGroupsCard,
  showFocusContactCard,
  selectedContactCount,
  selectedGroupCount,
  selectedContacts,
  selectedGroupRows,
  focusedContact,
  onClearContactSelection,
  onClearGroupSelection,
  onClearContactFocus,
}: {
  showContactsCard: boolean;
  showGroupsCard: boolean;
  showFocusContactCard: boolean;
  selectedContactCount: number;
  selectedGroupCount: number;
  selectedContacts: ContactListItem[];
  selectedGroupRows: CollapsedGroupConversation[];
  focusedContact: {
    id: number;
    displayName: string;
    preferredHandle: string | null;
  } | null;
  onClearContactSelection: () => void;
  onClearGroupSelection: () => void;
  onClearContactFocus: () => void;
}) {
  const { formatDateRange } = useDateTimeFormat();
  const groupRowDate = (g: CollapsedGroupConversation) =>
    formatDateRange(g.dateStart, g.dateEnd, " – ");

  const contactsCard = (
    <div className="overflow-hidden rounded-xl border border-border bg-popover shadow-[0_8px_24px_rgba(0,0,0,0.35)]">
      <div className="flex items-center justify-between gap-3 border-b border-white/15 px-4 py-3">
        <h2 className="text-[14px] font-semibold text-text">
          {selectedContactCount} contact
          {selectedContactCount === 1 ? "" : "s"} selected
        </h2>
        <button
          type="button"
          onClick={onClearContactSelection}
          className="inline-flex items-center rounded-md bg-hover px-2.5 py-1 text-[12px] text-text transition-colors hover:bg-hover-strong"
        >
          Clear selection
        </button>
      </div>
      <ul className="bg-received">
        {selectedContacts.map((c, i) => (
          <li
            key={c.id}
            className="relative flex items-center justify-between gap-4 px-4 py-2.5"
          >
            <span className="min-w-0 truncate text-[13px] text-text">
              {c.displayName}
            </span>
            <span className="shrink-0 text-[13px] text-muted tabular-nums">
              {c.preferredHandle
                ? formatPhoneDisplay(c.preferredHandle)
                : ""}
            </span>
            {i < selectedContacts.length - 1 && (
              <span
                aria-hidden
                className="pointer-events-none absolute right-4 bottom-0 left-4 h-px bg-hover-strong"
              />
            )}
          </li>
        ))}
      </ul>
    </div>
  );

  const focusContactCard =
    focusedContact != null ? (
      <div className="overflow-hidden rounded-xl border border-border bg-popover shadow-[0_8px_24px_rgba(0,0,0,0.35)]">
        <div className="flex items-center justify-between gap-3 border-b border-white/15 px-4 py-3">
          <h2 className="text-[14px] font-semibold text-text">
            1 contact selected
          </h2>
          <button
            type="button"
            onClick={onClearContactFocus}
            className="inline-flex items-center rounded-md bg-hover px-2.5 py-1 text-[12px] text-text transition-colors hover:bg-hover-strong"
          >
            Clear selection
          </button>
        </div>
        <ul className="bg-received">
          <li className="flex items-center justify-between gap-4 px-4 py-2.5">
            <span className="min-w-0 truncate text-[13px] text-text">
              {focusedContact.displayName}
            </span>
            <span className="shrink-0 text-[13px] text-muted tabular-nums">
              {focusedContact.preferredHandle
                ? formatPhoneDisplay(focusedContact.preferredHandle)
                : ""}
            </span>
          </li>
        </ul>
      </div>
    ) : null;

  const groupsCard = (
    <div className="overflow-hidden rounded-xl border border-border bg-popover shadow-[0_8px_24px_rgba(0,0,0,0.35)]">
      <div className="flex items-center justify-between gap-3 border-b border-white/15 px-4 py-3">
        <h2 className="text-[14px] font-semibold text-text">
          {selectedGroupCount} group message
          {selectedGroupCount === 1 ? "" : "s"} selected
        </h2>
        <button
          type="button"
          onClick={onClearGroupSelection}
          className="inline-flex items-center rounded-md bg-hover px-2.5 py-1 text-[12px] text-text transition-colors hover:bg-hover-strong"
        >
          Clear selection
        </button>
      </div>
      <ul className="bg-received">
        {selectedGroupRows.map((g, i) => {
          const namedTitle = g.namedTitle?.trim() || null;
          const names = !namedTitle ? collapsedParticipantLabels(g) : [];
          return (
            <li
              key={g.conversationId}
              className="relative flex items-start justify-between gap-4 px-4 py-2.5"
            >
              <span className="min-w-0 flex-1 text-[13px]">
                {namedTitle ? (
                  <span className="block truncate font-medium text-text">
                    {namedTitle}
                  </span>
                ) : names.length > 0 ? (
                  <span className="block leading-snug font-medium text-text">
                    {names.map((name, idx) => (
                      <span key={`${g.conversationId}-name-${idx}`}>
                        {idx > 0 ? <GroupNameSep /> : null}
                        <span className="whitespace-nowrap">{name}</span>
                      </span>
                    ))}
                  </span>
                ) : (
                  <span className="block truncate font-medium text-text">
                    {g.title || "Group message"}
                  </span>
                )}
              </span>
              <span className="shrink-0 pt-px text-[13px] leading-snug text-muted tabular-nums">
                {groupRowDate(g)}
              </span>
              {i < selectedGroupRows.length - 1 && (
                <span
                  aria-hidden
                  className="pointer-events-none absolute right-4 bottom-0 left-4 h-px bg-hover-strong"
                />
              )}
            </li>
          );
        })}
      </ul>
    </div>
  );

  return (
    <div className="min-h-0 flex-1 space-y-4 overflow-y-auto bg-bg px-5 pt-8 pb-5">
      {showGroupsCard ? groupsCard : null}
      {showContactsCard ? contactsCard : null}
      {showFocusContactCard ? focusContactCard : null}
    </div>
  );
}
