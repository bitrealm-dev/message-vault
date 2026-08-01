"use client";

import type { SearchContactHit, SearchConversationHit } from "@/lib/search";
import type { MouseEvent } from "react";
import { BrowseContactRow } from "./BrowseContactRow";
import { SearchHitSummary } from "./SearchResultsList";

const CHILD_INDENT_PX = 20;
const EMPTY_SELECTED_CONTACT_IDS: ReadonlySet<number> = new Set();

/**
 * Search results grouped by contact: the normal contact row, expandable to the
 * conversations of theirs that matched. Selecting contacts here feeds the same
 * bulk actions as the contact list, so matches can be labeled directly.
 */
export function SearchContactResultsList({
  contacts,
  total,
  loading,
  highlightTerms,
  selectedConversationId,
  selectedContactIds = EMPTY_SELECTED_CONTACT_IDS,
  expandedContactIds = EMPTY_SELECTED_CONTACT_IDS,
  contactId = null,
  onToggleContact,
  onToggleExpand,
  onSelectHit,
  onContactContextMenu,
  emptyLabel = "No matches",
}: {
  contacts: SearchContactHit[];
  total: number;
  loading: boolean;
  highlightTerms?: string[];
  selectedConversationId: number | null;
  selectedContactIds?: ReadonlySet<number>;
  expandedContactIds?: ReadonlySet<number>;
  /** Contact open in the detail pane. */
  contactId?: number | null;
  onToggleContact?: (
    contactId: number,
    mods?: { shiftKey: boolean },
  ) => void;
  onToggleExpand?: (contactId: number) => void;
  onSelectHit?: (hit: SearchConversationHit) => void;
  onContactContextMenu?: (contactId: number, x: number, y: number) => void;
  emptyLabel?: string;
}) {
  if (loading) {
    return (
      <p className="px-3 py-8 text-center text-[13px] text-muted">Searching…</p>
    );
  }

  if (contacts.length === 0) {
    return (
      <p className="px-3 py-8 text-center text-[13px] text-muted">
        {emptyLabel}
      </p>
    );
  }

  const selectionActive = selectedContactIds.size >= 1;

  return (
    <div className="min-h-0 flex-1 overflow-y-auto [scrollbar-gutter:stable]">
      <div className="sticky top-0 z-10 border-b border-border bg-sidebar px-3 py-1 text-[11px] font-semibold tracking-wider text-muted uppercase">
        Results · {total.toLocaleString()}
      </div>
      {contacts.map(({ contact, hits }, index) => {
        const expanded = expandedContactIds.has(contact.id);
        const checked = selectedContactIds.has(contact.id);
        const active = checked || expanded || contact.id === contactId;
        return (
          <div key={contact.id}>
            <BrowseContactRow
              contact={contact}
              active={active}
              checked={checked}
              selectionActive={selectionActive}
              expanded={expanded}
              showExpandChevron
              showInsetDivider={!expanded && index < contacts.length - 1}
              onSelectColumnClick={(id, e: MouseEvent) =>
                onToggleContact?.(id, { shiftKey: e.shiftKey })
              }
              onNamePhoneClick={(id, e) =>
                onToggleContact?.(id, { shiftKey: e.shiftKey })
              }
              onContextMenu={(id, x, y) => onContactContextMenu?.(id, x, y)}
              onToggleExpand={onToggleExpand}
            />
            {expanded ? (
              <div className="border-b border-border/40">
                {hits.map((hit) => (
                  <button
                    key={hit.conversationId}
                    type="button"
                    onClick={() => onSelectHit?.(hit)}
                    style={{ paddingLeft: CHILD_INDENT_PX + 40 }}
                    className={`relative flex w-full min-w-0 flex-col gap-0.5 py-2 pr-3 text-left transition-colors ${
                      selectedConversationId === hit.conversationId
                        ? "bg-accent/20 hover:bg-accent/25"
                        : "hover:bg-hover"
                    }`}
                  >
                    {selectedConversationId === hit.conversationId ? (
                      <span
                        aria-hidden
                        className="absolute top-1 bottom-1 left-0 w-1 rounded-full bg-accent/80"
                      />
                    ) : null}
                    <SearchHitSummary
                      hit={hit}
                      highlightTerms={highlightTerms}
                    />
                  </button>
                ))}
              </div>
            ) : null}
          </div>
        );
      })}
    </div>
  );
}
