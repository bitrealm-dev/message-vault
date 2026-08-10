import type { ReactNode } from "react";
import type { Conversation } from "../lib/types";
import { useListColumnResizing } from "./ListColumnResizeContext";

/** Calendar date: year, month, and day (e.g. "Sep 9, 2024"). */
function formatYmd(iso: string): string {
  return new Date(iso).toLocaleDateString([], {
    year: "numeric",
    month: "short",
    day: "numeric",
  });
}

/** First and last message dates for the bottom-right corner. */
function formatDateSpan(start: string | null, end: string | null): string | null {
  if (start && end) {
    const a = formatYmd(start);
    const b = formatYmd(end);
    return a === b ? a : `${a} – ${b}`;
  }
  if (end) return formatYmd(end);
  if (start) return formatYmd(start);
  return null;
}

/** Short service label (imessage / sms/mms). */
function formatServiceLabel(service: string): string | null {
  const s = service.trim();
  if (!s || s.toLowerCase() === "unknown") return null;
  const lower = s.toLowerCase();
  if (lower === "imessage" || lower === "ios") return "imessage";
  if (lower === "sms/mms" || lower === "sms" || lower === "mms" || lower.includes("sms")) {
    return "sms/mms";
  }
  return s;
}

function GroupIcon() {
  return (
    <svg
      width="12"
      height="12"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
      className="inline-block shrink-0 align-[-1px]"
    >
      <path d="M17 21v-2a4 4 0 0 0-4-4H5a4 4 0 0 0-4 4v2" />
      <circle cx="7" cy="7" r="4" />
      <path d="M23 21v-2a4 4 0 0 0-3-3.87" />
      <path d="M16 3.13a4 4 0 0 1 0 7.75" />
    </svg>
  );
}

function participantLabel(p: { name: string | null; handle: string }): string {
  return p.name || p.handle;
}

/** Comma-separated names; each name stays whole; at most two lines then ellipsis. */
function GroupNames({ conv }: { conv: Conversation }) {
  return (
    <span className="line-clamp-2 break-words leading-[1.35]">
      {conv.participants.map((p, i) => {
        const label = participantLabel(p);
        return (
          <span key={`${p.handle}-${i}`}>
            {i > 0 ? ", " : null}
            <span className="whitespace-nowrap">{label}</span>
          </span>
        );
      })}
    </span>
  );
}

function titleContent(conv: Conversation): ReactNode {
  if (conv.label) return conv.label;
  if (!conv.is_group) {
    const p = conv.participants[0];
    return p?.name || p?.handle || "(unknown)";
  }
  return <GroupNames conv={conv} />;
}

/** Bottom-left for groups: service only (count sits upper-right). */
function GroupService({ conv }: { conv: Conversation }) {
  return formatServiceLabel(conv.service);
}

function GroupParticipantCount({ count }: { count: number }) {
  return (
    <span
      className="mt-[0.1rem] inline-flex shrink-0 items-center gap-1 text-[0.75rem] font-medium leading-[1.35] text-muted"
      title={`${count} participants`}
    >
      <span>{count}</span>
      <GroupIcon />
    </span>
  );
}

function directService(conv: Conversation): string | null {
  const fromConv = formatServiceLabel(conv.service);
  if (fromConv) return fromConv;
  const p = conv.participants[0];
  return p ? formatServiceLabel(p.service) : null;
}

export default function ConversationRow({
  conversation,
  isSelected,
  onClick,
  checked,
  onCheckChange,
}: {
  conversation: Conversation;
  isSelected: boolean;
  onClick: () => void;
  checked?: boolean;
  onCheckChange?: (id: string) => void;
}) {
  const columnResizing = useListColumnResizing();
  const isGroup = conversation.is_group;
  const wraps = isGroup && !conversation.label && !columnResizing;
  const dateSpan = formatDateSpan(
    conversation.date_range_start,
    conversation.last_message_at || conversation.date_range_end,
  );
  const bottomLeft = isGroup ? (
    <GroupService conv={conversation} />
  ) : (
    directService(conversation)
  );

  return (
    <button
      onClick={onClick}
      className={`box-border flex w-full cursor-pointer items-start gap-2 border-b border-border border-none px-[0.85rem] py-[0.7rem] text-left ${
        isSelected ? "bg-hover" : "bg-transparent"
      }`}
    >
      {onCheckChange && (
        <input
          type="checkbox"
          checked={checked || false}
          onChange={(e) => {
            e.stopPropagation();
            onCheckChange(conversation.id);
          }}
          onClick={(e) => e.stopPropagation()}
          className="mt-0.5 shrink-0"
        />
      )}
      <div className="flex min-w-0 flex-1 flex-col gap-[0.3rem]">
        <div className="flex min-w-0 items-start justify-between gap-2">
          <span
            className={`min-w-0 flex-1 text-[0.875rem] font-medium leading-[1.35] text-text ${
              wraps ? "overflow-hidden" : "truncate"
            }`}
          >
            {titleContent(conversation)}
          </span>
          {isGroup ? (
            <GroupParticipantCount count={conversation.participants.length} />
          ) : null}
        </div>

        <div className="flex items-baseline justify-between gap-2 text-[0.75rem] text-muted">
          <span className="min-w-0 truncate">
            {bottomLeft}
          </span>
          {dateSpan ? (
            <span className="shrink-0 text-right">{dateSpan}</span>
          ) : null}
        </div>
      </div>
    </button>
  );
}
