import { useCallback, useEffect, useMemo, useState } from "react";
import ConversationRow from "../components/ConversationRow";
import ConversationSortMenu from "../components/ConversationSortMenu";
import ListRangeHeader from "../components/ListRangeHeader";
import ListRangePill, {
  RANGE_PILL_OVERLAY_INSET,
  RANGE_PILL_SCROLL_PAD,
} from "../components/ListRangePill";
import TagsMenu from "../components/TagsMenu";
import { useSetRightToolbar } from "../components/useRightToolbar";
import VirtualList, { type VisibleRange } from "../components/VirtualList";
import {
  type ConversationSortState,
  loadConversationSort,
  saveConversationSort,
} from "../lib/conversationSort";
import { checksFromMembers } from "../lib/membershipChecks";
import { createMessageTag, setConversationTagMembership } from "../lib/messageTags";
import type { Conversation } from "../lib/types";
import { useMessageTags } from "../lib/useMessageTags";
import { formatVisibleRange, type PagedFetchPage, usePagedList } from "../lib/usePagedList";
import { listConversations } from "../lib/vaultApi";

const QUERY_DEBOUNCE_MS = 300;

export default function ConversationList({
  selectedId,
  onSelect,
  query,
}: {
  selectedId: string | null;
  onSelect: (conversation: Conversation) => void;
  query: string;
}) {
  const [debouncedQ, setDebouncedQ] = useState(query);
  const [visibleRange, setVisibleRange] = useState<VisibleRange>({ start: 0, end: 0 });
  const [checkedIds, setCheckedIds] = useState<Set<string>>(() => new Set());
  const [tagOverrides, setTagOverrides] = useState<Record<string, string[]>>({});
  const [membershipRev, setMembershipRev] = useState(0);
  const [sortState, setSortState] = useState<ConversationSortState>(() => loadConversationSort());
  const { tags: allTags } = useMessageTags();
  const setRightToolbar = useSetRightToolbar();

  useEffect(() => {
    void query;
    setCheckedIds(new Set());
    setTagOverrides({});
  }, [query]);

  useEffect(() => {
    // Filters like contact: and handle: apply immediately so the list does not flash empty.
    if (
      /\b(contact:|handle:|import:|is:direct|is:group|is:trash|participants:|tag:|people:|within:|label:)\b/i.test(
        query,
      )
    ) {
      setDebouncedQ(query);
      return;
    }
    const t = window.setTimeout(() => setDebouncedQ(query), QUERY_DEBOUNCE_MS);
    return () => window.clearTimeout(t);
  }, [query]);

  const fetchPage = useCallback<PagedFetchPage<Conversation>>(
    async ({ limit, offset, signal }) => {
      const res = await listConversations(
        {
          q: debouncedQ,
          limit,
          offset,
          sort: sortState.sort,
          order: sortState.order,
        },
        { signal },
      );
      return {
        items: res.conversations || [],
        total: res.total ?? 0,
      };
    },
    [debouncedQ, sortState],
  );

  const {
    items: conversations,
    total,
    loading,
    refreshing,
    filling,
    error,
    hasMore,
    loadMore,
  } = usePagedList(
    `${debouncedQ}#${membershipRev}#${sortState.sort}:${sortState.order}`,
    fetchPage,
  );

  const displayConversations = useMemo(
    () => conversations.map((c) => (tagOverrides[c.id] ? { ...c, tags: tagOverrides[c.id] } : c)),
    [conversations, tagOverrides],
  );

  const selectedConversation = displayConversations.find((c) => c.id === selectedId) ?? null;
  const targetConversations = useMemo(() => {
    if (checkedIds.size > 0) {
      return displayConversations.filter((c) => checkedIds.has(c.id));
    }
    return selectedConversation ? [selectedConversation] : [];
  }, [checkedIds, displayConversations, selectedConversation]);
  const tagChecks = useMemo(
    () =>
      checksFromMembers(
        allTags,
        targetConversations.map((c) => c.tags ?? []),
      ),
    [allTags, targetConversations],
  );

  const applyMembership = useCallback(
    async (name: string, enable: boolean) => {
      const ids = targetConversations
        .map((c) => Number(c.id))
        .filter((id) => Number.isFinite(id) && id > 0);
      if (ids.length === 0) return;
      await setConversationTagMembership(ids, name, enable);
      setTagOverrides((prev) => {
        const next = { ...prev };
        for (const c of targetConversations) {
          const current = next[c.id] ?? c.tags ?? [];
          next[c.id] = enable
            ? current.some((t) => t.toLowerCase() === name.toLowerCase())
              ? current
              : [...current, name]
            : current.filter((t) => t.toLowerCase() !== name.toLowerCase());
        }
        return next;
      });
      if (/\b(?:-?tag:|-?people:|within:|label:)/i.test(query)) {
        setMembershipRev((n) => n + 1);
      }
    },
    [query, targetConversations],
  );

  useEffect(() => {
    setRightToolbar(
      <TagsMenu
        allTags={allTags}
        checks={tagChecks}
        disabled={targetConversations.length === 0}
        onToggle={(name) => {
          const on = tagChecks[name] === "on";
          void applyMembership(name, !on);
        }}
        onCreate={(name) => {
          void (async () => {
            const existing = allTags.find((t) => t.toLowerCase() === name.toLowerCase());
            if (!existing) {
              await createMessageTag(name);
            }
            await applyMembership(existing ?? name, true);
          })();
        }}
        onClearAll={() => {
          const names = new Set<string>();
          for (const c of targetConversations) {
            for (const t of c.tags ?? []) names.add(t);
          }
          void (async () => {
            for (const name of names) {
              await applyMembership(name, false);
            }
          })();
        }}
      />,
    );
    return () => setRightToolbar(null);
  }, [allTags, applyMembership, setRightToolbar, tagChecks, targetConversations]);

  const selectAllChecked =
    displayConversations.length > 0 && displayConversations.every((c) => checkedIds.has(c.id));
  const selectAllIndeterminate =
    !selectAllChecked && displayConversations.some((c) => checkedIds.has(c.id));

  const rangeLabel =
    loading && conversations.length === 0
      ? "Loading…"
      : formatVisibleRange(visibleRange.start, visibleRange.end, total, conversations.length);
  // Once there are rows the count rides at the bottom of the panel, the way the
  // contact list shows it; the header keeps it only while the list is still empty.
  const showRangePill = displayConversations.length > 0;

  if (error && conversations.length === 0) {
    return (
      <div className="p-4 text-[0.813rem] text-danger">Could not load conversations: {error}</div>
    );
  }

  return (
    <div className="relative flex min-h-0 flex-1 flex-col">
      <ListRangeHeader
        rangeLabel={showRangePill ? undefined : rangeLabel}
        refreshing={!showRangePill && refreshing}
        filling={!showRangePill && filling}
        selectAllChecked={selectAllChecked}
        selectAllIndeterminate={selectAllIndeterminate}
        onSelectAllChange={(on) => {
          setCheckedIds(on ? new Set(displayConversations.map((c) => c.id)) : new Set());
        }}
        selectAllLabel="Select all conversations"
        selectAllDisabled={displayConversations.length === 0}
        actions={
          <ConversationSortMenu
            sort={sortState.sort}
            order={sortState.order}
            onChange={(next) => {
              setSortState(next);
              saveConversationSort(next);
            }}
          />
        }
      />
      <VirtualList
        count={displayConversations.length}
        estimateSize={64}
        dynamicSize
        onVisibleRangeChange={setVisibleRange}
        visibleBottomInset={RANGE_PILL_OVERLAY_INSET}
        footer={<div aria-hidden className="shrink-0" style={{ height: RANGE_PILL_SCROLL_PAD }} />}
        onNearEnd={() => {
          if (hasMore) loadMore();
        }}
        empty={
          !loading ? <div className="p-4 text-[0.813rem] text-muted">No conversations</div> : null
        }
        renderItem={(index) => {
          const c = displayConversations[index];
          if (!c) return null;
          return (
            <ConversationRow
              conversation={c}
              isSelected={c.id === selectedId}
              onClick={() => onSelect(c)}
              checked={checkedIds.has(c.id)}
              onCheckChange={(id) => {
                setCheckedIds((prev) => {
                  const next = new Set(prev);
                  if (next.has(id)) next.delete(id);
                  else next.add(id);
                  return next;
                });
              }}
            />
          );
        }}
      />
      {showRangePill ? (
        <ListRangePill
          rangeLabel={rangeLabel}
          refreshing={refreshing}
          filling={filling}
          testId="conversation-list-range-pill"
        />
      ) : null}
    </div>
  );
}
