"use client";

import type {
  ContactDetail,
  GroupParticipant,
  MessageRow,
  YearThread,
} from "@/lib/types";
import { GroupParticipantChip } from "./GroupParticipantChip";
import {
  BrowseThreadPane,
  type BrowseGroupThreadMeta,
} from "./BrowseThreadPane";

export function BrowseThreadColumn({
  paneStorageKey,
  detail,
  groupThread,
  vaultReadOnly,
  statusMsg,
  contactId,
  activeThread,
  sources,
  messageSources,
  sourceCounts,
  source,
  onSourceChange,
  yearly,
  messages,
  loadingMessages,
  threadsLoadedFor,
  hasConversationChoices = false,
  highlightTerms = [],
  scrollToMessageId = null,
  onContactNameClick,
  onGroupParticipantClick,
  readerOnly = false,
  hasSelection = false,
  hasGroupSelection = false,
  emptySelectionGuidance = "Selection details are shown in the inspector on the right.",
  emptyFocusGuidance = "Choose a Direct or group conversation in the tree.",
  emptyIdleGuidance = "Select a person or conversation to read messages.",
  threadsReadyOverride,
}: {
  paneStorageKey: string;
  detail: ContactDetail | null;
  groupThread: BrowseGroupThreadMeta | null;
  vaultReadOnly: boolean;
  statusMsg: string | null;
  contactId: number | null;
  activeThread: string | null;
  sources: string[];
  messageSources: string[];
  sourceCounts: { all: number; bySource: Record<string, number> };
  source: string | null;
  onSourceChange: (id: string | null) => void;
  yearly: YearThread[];
  messages: MessageRow[];
  loadingMessages: boolean;
  threadsLoadedFor: number | null;
  hasConversationChoices?: boolean;
  highlightTerms?: string[];
  scrollToMessageId?: number | null;
  onContactNameClick?: (anchorRect: DOMRect) => void;
  onGroupParticipantClick: (
    participant: GroupParticipant,
    anchorRect: DOMRect,
  ) => void;
  /** When true, always prefer the message reader over empty/selection states. */
  readerOnly?: boolean;
  hasSelection?: boolean;
  hasGroupSelection?: boolean;
  emptySelectionGuidance?: string;
  emptyFocusGuidance?: string;
  emptyIdleGuidance?: string;
  /** When set, overrides the contact-based threadsReady check. */
  threadsReadyOverride?: boolean;
}) {
  const showReader =
    readerOnly
      ? !hasSelection && !hasGroupSelection && activeThread != null
      : activeThread != null;
  const threadsReady =
    threadsReadyOverride ?? threadsLoadedFor === contactId;

  return (
    <div
      id={`browse-${paneStorageKey}-thread`}
      className="flex h-full min-h-0 min-w-0 flex-col"
    >
      <div className="flex h-[45px] shrink-0 items-center gap-2 border-b border-border px-5">
        <div className="flex min-w-0 flex-1 items-center justify-center">
          {!hasSelection && detail && !groupThread ? (
            <h1 className="truncate text-lg font-semibold tracking-tight text-text">
              {!vaultReadOnly && onContactNameClick ? (
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

      {showReader ? (
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
            threadsReady={threadsReady}
            activeThread={activeThread}
            groupThread={groupThread}
            onParticipantClick={
              vaultReadOnly ? undefined : onGroupParticipantClick
            }
            hasConversationChoices={hasConversationChoices}
            conversationsPanelCollapsed={false}
            highlightTerms={highlightTerms}
            scrollToMessageId={scrollToMessageId}
          />
        </div>
      ) : (
        <div className="flex min-h-0 flex-1 items-start justify-center px-5 pt-8">
          <p className="max-w-sm text-center text-[13px] text-muted">
            {hasSelection || hasGroupSelection
              ? emptySelectionGuidance
              : contactId != null
                ? emptyFocusGuidance
                : emptyIdleGuidance}
          </p>
        </div>
      )}
    </div>
  );
}
