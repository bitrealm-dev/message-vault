"use client";

import type { CollapsedGroupConversation } from "@/lib/groupChatList";
import { CountBadge } from "./CountBadge";
import { MessageIcon, PeopleCountIcon, TrashIcon } from "./icons";
import { useDateTimeFormat } from "./useDateTimeFormat";

export const GROUP_NAME_SEP = "  ·  ";
export const MAX_VISIBLE_GROUP_NAMES = 8;

/** Soft wrap gap around · so whole names stay together. */
export function GroupNameSep({
  variant = "browse",
}: {
  variant?: "browse" | "trash";
}) {
  if (variant === "trash") {
    return (
      <span className="px-1.5 font-normal text-muted" aria-hidden>
        ·
      </span>
    );
  }
  return (
    <span className="font-normal text-muted" aria-hidden>
      {" · "}
    </span>
  );
}

/** Show up to 8 names; 9+ adds +n for the remainder. */
export function visibleParticipantLabels(labels: string[]): string[] {
  if (labels.length <= MAX_VISIBLE_GROUP_NAMES) return labels;
  return [
    ...labels.slice(0, MAX_VISIBLE_GROUP_NAMES),
    `+${labels.length - MAX_VISIBLE_GROUP_NAMES}`,
  ];
}

/** Prefer names, then handles, then parse title (browse fallback). */
export function collapsedParticipantLabels(
  g: CollapsedGroupConversation,
): string[] {
  if (g.participantNames.length > 0) return g.participantNames;
  if (g.participantHandles.length > 0) return g.participantHandles;
  if (g.title && g.title !== "Group chat" && g.title !== "Group message") {
    // Browse rows use a middle-dot separator; search titles join with commas.
    const byDot = g.title
      .split(/\u00a0*\u00a0·\u00a0\u00a0| {2}· {2}/)
      .map((n) => n.replace(/\u00a0/g, " ").trim())
      .filter(Boolean);
    if (byDot.length > 1) return byDot;
    const byComma = g.title
      .split(/\s*,\s*/)
      .map((n) => n.trim())
      .filter(Boolean);
    if (byComma.length > 1) return byComma;
    return byDot;
  }
  return [];
}

/** Shared names / date / message+people counts for group conversation rows. */
export function GroupConversationRowBody({
  conversation: g,
  variant = "browse",
  trashedAt,
}: {
  conversation: CollapsedGroupConversation;
  variant?: "browse" | "trash";
  /** Soft-trash timestamp (trash variant 4th line). */
  trashedAt?: string;
}) {
  const { formatDateRange, formatDateTime } = useDateTimeFormat();
  const allNames = collapsedParticipantLabels(g);
  const names = visibleParticipantLabels(allNames);
  const namesTitle =
    allNames.length > 0
      ? allNames.join(GROUP_NAME_SEP)
      : g.titleFull || g.title;
  const dateLabel = formatDateRange(g.dateStart, g.dateEnd, " – ");

  if (variant === "trash") {
    return (
      <div className="flex w-full gap-2">
        <div className="min-w-0 flex-1">
          {g.namedTitle ? (
            <div className="mb-1 truncate text-[12px] font-medium text-text">
              {g.namedTitle}
            </div>
          ) : null}
          <div
            className="flex min-w-0 flex-wrap gap-y-0.5 text-[13px] leading-snug font-medium text-text"
            title={namesTitle}
          >
            {names.map((name, idx) => (
              <span
                key={`${g.conversationId}-name-${idx}`}
                className="whitespace-nowrap"
              >
                {name}
                {idx < names.length - 1 ? (
                  <GroupNameSep variant="trash" />
                ) : null}
              </span>
            ))}
          </div>
          <div className="mt-1.5 truncate font-mono text-[12px] text-muted">
            {dateLabel}
          </div>
          {trashedAt ? (
            <div className="mt-1 flex items-center gap-1 text-[11px] text-muted tabular-nums">
              <TrashIcon className="size-3 shrink-0 opacity-70" />
              <span>{formatDateTime(trashedAt)}</span>
            </div>
          ) : null}
        </div>
        <div className="flex w-[4.5rem] shrink-0 flex-col items-end justify-between pt-0.5">
          <span className="inline-flex items-center gap-1">
            <CountBadge count={g.messageCount} title="Messages" />
            <MessageIcon className="size-3.5 shrink-0 text-muted opacity-80" />
          </span>
          <span className="inline-flex items-center gap-0.5 text-[11px] tabular-nums text-muted">
            <PeopleCountIcon className="size-3.5 opacity-80" />
            {g.participantCount.toLocaleString()}
          </span>
        </div>
      </div>
    );
  }

  return (
    <span className="flex min-w-0 flex-1 flex-col">
      <span className="flex min-w-0 items-start gap-2">
        <span className="min-w-0 flex-1">
          {g.namedTitle ? (
            <span className="mb-0.5 block truncate text-[12px] font-medium text-text">
              {g.namedTitle}
            </span>
          ) : null}
          {names.length > 0 ? (
            <span
              className="line-clamp-3 text-[13px] font-medium leading-snug text-text"
              title={namesTitle}
            >
              {names.map((name, idx) => (
                <span key={`${g.conversationId}-name-${idx}`}>
                  {idx > 0 ? <GroupNameSep /> : null}
                  <span className="whitespace-nowrap">{name}</span>
                </span>
              ))}
            </span>
          ) : !g.namedTitle ? (
            <span className="line-clamp-3 text-[13px] font-medium leading-snug text-text">
              {g.title}
            </span>
          ) : null}
        </span>
        <span className="inline-flex shrink-0 items-center gap-1 py-0.5 text-[11px] text-muted">
          <CountBadge count={g.messageCount} title="Messages" />
          <MessageIcon className="size-3.5 shrink-0 text-muted opacity-80" />
        </span>
      </span>
      <span className="mt-0.5 flex min-w-0 items-center justify-between gap-2 text-[11px] text-muted tabular-nums">
        <span className="inline-flex shrink-0 items-center gap-0.5">
          <PeopleCountIcon className="size-3.5 shrink-0 opacity-80" />
          {g.participantCount.toLocaleString()}
        </span>
        <span className="min-w-0 truncate text-right">{dateLabel}</span>
      </span>
    </span>
  );
}
