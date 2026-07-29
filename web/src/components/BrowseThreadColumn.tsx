"use client";

import type { CollapsedGroupConversation } from "@/lib/groupChatList";
import type {
  ContactDetail,
  ContactListItem,
  GroupParticipant,
  MessageRow,
  YearThread,
} from "@/lib/types";
import { GroupParticipantChip } from "./GroupParticipantChip";
import {
  browseSelectionSummaryFlags,
  BrowseSelectionSummary,
} from "./BrowseSelectionSummary";
import {
  BrowseThreadPane,
  type BrowseGroupThreadMeta,
} from "./BrowseThreadPane";

export function BrowseThreadColumn({
  paneStorageKey,
  selectedIds,
  selectedContacts,
  hasSelection,
  hasGroupSelection,
  selectedGroupIds,
  selectedGroupRows,
  detail,
  groupThread,
  vaultReadOnly,
  statusMsg,
  contactId,
  contacts,
  activeThread,
  sources,
  messageSources,
  sourceCounts,
  source,
  onSourceChange,
  yearly,
  messages,
  loadingMessages,
  loadingSelectionGroups,
  threadsLoadedFor,
  hasConversationChoices = false,
  conversationsPanelCollapsed = false,
  highlightTerms = [],
  scrollToMessageId = null,
  onContactNameClick,
  onGroupParticipantClick,
  onClearContactSelection,
  onClearGroupSelection,
  onClearContactFocus,
}: {
  paneStorageKey: string;
  selectedIds: ReadonlySet<number>;
  selectedContacts: ContactListItem[];
  hasSelection: boolean;
  hasGroupSelection: boolean;
  selectedGroupIds: ReadonlySet<number>;
  selectedGroupRows: CollapsedGroupConversation[];
  detail: ContactDetail | null;
  groupThread: BrowseGroupThreadMeta | null;
  vaultReadOnly: boolean;
  statusMsg: string | null;
  contactId: number | null;
  contacts: ContactListItem[];
  activeThread: string | null;
  sources: string[];
  messageSources: string[];
  sourceCounts: { all: number; bySource: Record<string, number> };
  source: string | null;
  onSourceChange: (id: string | null) => void;
  yearly: YearThread[];
  messages: MessageRow[];
  loadingMessages: boolean;
  loadingSelectionGroups: boolean;
  threadsLoadedFor: number | null;
  hasConversationChoices?: boolean;
  conversationsPanelCollapsed?: boolean;
  highlightTerms?: string[];
  scrollToMessageId?: number | null;
  onContactNameClick: (anchorRect: DOMRect) => void;
  onGroupParticipantClick: (
    participant: GroupParticipant,
    anchorRect: DOMRect,
  ) => void;
  onClearContactSelection: () => void;
  onClearGroupSelection: () => void;
  onClearContactFocus: () => void;
}) {
  const {
    showContactsCard,
    showGroupsCard,
    showFocusContactCard,
    showThreadPane,
    focusedContact,
  } = browseSelectionSummaryFlags({
    hasSelection,
    hasGroupSelection,
    activeThread,
    contactId,
    detail,
    contacts,
  });

  return (
    <div
      id={`browse-${paneStorageKey}-thread`}
      className="flex h-full min-h-0 min-w-0 flex-col"
    >
      <div className="flex h-[45px] shrink-0 items-center gap-2 border-b border-border px-5">
        <div className="flex min-w-0 flex-1 items-center justify-center">
          {selectedIds.size === 1 && selectedContacts[0] ? (
            <h1 className="truncate text-lg font-semibold tracking-tight text-text">
              {selectedContacts[0].displayName}
            </h1>
          ) : !hasSelection && detail && !groupThread ? (
            <h1 className="truncate text-lg font-semibold tracking-tight text-text">
              {!vaultReadOnly ? (
                <GroupParticipantChip
                  label={detail.displayName || "Contact"}
                  onClick={onContactNameClick}
                />
              ) : (
                detail.displayName || "Contact"
              )}
            </h1>
          ) : null}
        </div>
        {statusMsg && (
          <span className="shrink-0 truncate text-[12px] text-muted">
            {statusMsg}
          </span>
        )}
      </div>

      {showContactsCard || showGroupsCard || showFocusContactCard ? (
        <BrowseSelectionSummary
          showContactsCard={showContactsCard}
          showGroupsCard={showGroupsCard}
          showFocusContactCard={showFocusContactCard}
          selectedContactCount={selectedIds.size}
          selectedGroupCount={selectedGroupIds.size}
          selectedContacts={selectedContacts}
          selectedGroupRows={selectedGroupRows}
          focusedContact={focusedContact}
          onClearContactSelection={onClearContactSelection}
          onClearGroupSelection={onClearGroupSelection}
          onClearContactFocus={onClearContactFocus}
        />
      ) : showThreadPane ? (
        <div className="min-h-0 flex-1">
          <BrowseThreadPane
            detail={detail}
            sources={sources}
            messageSources={messageSources}
            sourceCounts={sourceCounts}
            source={source}
            onSourceChange={onSourceChange}
            yearly={yearly}
            messages={messages}
            loadingMessages={loadingMessages}
            threadsReady={
              hasSelection
                ? !loadingSelectionGroups
                : threadsLoadedFor === contactId
            }
            activeThread={activeThread}
            groupThread={groupThread}
            onParticipantClick={
              vaultReadOnly ? undefined : onGroupParticipantClick
            }
            hasConversationChoices={hasConversationChoices}
            conversationsPanelCollapsed={conversationsPanelCollapsed}
            highlightTerms={highlightTerms}
            scrollToMessageId={scrollToMessageId}
          />
        </div>
      ) : null}
    </div>
  );
}
