import type { CSSProperties, ReactNode } from "react";
import type { Message, MessageAttachment } from "../../lib/types";
import { personDisplayLabel, readUseNameAliases } from "../../lib/nameAliases";
import { highlightText } from "../../lib/highlightText";

type BubblePalette = "imessage" | "sms";

/** Props every per-service message renderer accepts. */
export type MessageBubbleProps = {
  message: Message;
  highlight?: string;
  isActive?: boolean;
  onAttachmentClick?: (attachment: MessageAttachment, source: string) => void;
};

/** Timestamp shown under a bubble or beside a flat-row sender. */
export function formatMessageTime(timestamp: string, withYear = false): string {
  return new Date(timestamp).toLocaleString([], {
    month: "short",
    day: "numeric",
    ...(withYear ? { year: "numeric" as const } : {}),
    hour: "numeric",
    minute: "2-digit",
  });
}

/** Message text with search matches marked, or nothing when the body is empty. */
export function bubbleBody(
  body: string,
  highlight: string | undefined,
): ReactNode | undefined {
  if (!body) return undefined;
  return highlight ? highlightText(body, highlight) : body;
}

export function senderName(m: Message): string {
  if (m.is_from_me) return "Me";
  const useAliases = readUseNameAliases();
  const labelFor = (p: {
    preferred_name?: string | null;
    name_alias: string | null;
    handle: string;
  }) =>
    personDisplayLabel(
      {
        preferredName: p.preferred_name,
        nameAlias: p.name_alias,
        handle: p.handle,
      },
      useAliases,
    );
  if (m.sender) {
    const p = m.conversation.participants.find((x) => x.handle === m.sender);
    return p ? labelFor(p) : m.sender;
  }
  const p = m.conversation.participants[0];
  return p ? labelFor(p) : "Unknown";
}

export function isGroupConversation(m: Message): boolean {
  return (
    m.conversation.conversation_type === "group" ||
    m.conversation.participants.length > 1
  );
}

/** Bubble fill/text color per palette (theme vars switch with data-theme). */
function bubbleColorClasses(palette: BubblePalette, mine: boolean): string {
  if (mine) {
    return palette === "imessage"
      ? "bg-[var(--imessage-sent)] text-[var(--imessage-sent-text)]"
      : "bg-[var(--sms-sent)] text-[var(--sms-sent-text)]";
  }
  return "bg-[var(--bubble-received)] text-[var(--bubble-received-text)]";
}

/** Sender-label color for the palette (same sent color as the bubble). */
function senderColorClass(palette: BubblePalette): string {
  return palette === "imessage"
    ? "text-[var(--imessage-sent)]"
    : "text-[var(--sms-sent)]";
}

/** Chat-row chrome: aligned bubble, optional sender label, timestamp under bubble. */
export function ChatBubbleRow({
  messageId,
  mine,
  isActive,
  palette,
  showSender,
  senderLabel,
  timeLabel,
  meta,
  children,
  footer,
}: {
  messageId: string;
  mine: boolean;
  isActive?: boolean;
  palette: BubblePalette;
  showSender?: boolean;
  senderLabel?: string;
  timeLabel: string;
  meta?: ReactNode;
  children?: ReactNode;
  footer?: ReactNode;
}) {
  const radius = mine
    ? "rounded-[18px] rounded-br-[4px]"
    : "rounded-[18px] rounded-bl-[4px]";
  const hasBubble = children != null && children !== false && children !== "";

  return (
    <div
      id={`msg-${messageId}`}
      className={`flex flex-col px-4 py-[0.2rem] ${
        mine ? "mb-[0.15rem] items-end" : "mb-[0.4rem] items-start"
      } ${isActive ? "bg-search-active" : "bg-transparent"}`}
    >
      {showSender && senderLabel ? (
        <div
          className={`mb-[0.15rem] max-w-[min(78%,34rem)] px-[0.35rem] text-[0.75rem] font-semibold ${senderColorClass(palette)}`}
        >
          {senderLabel}
        </div>
      ) : null}

      {hasBubble ? (
        <div
          className={`${radius} ${bubbleColorClasses(palette, mine)} max-w-[min(78%,34rem)] whitespace-pre-wrap break-words px-[0.7rem] py-[0.45rem] text-[0.9375rem] leading-[1.35] ${
            mine ? "" : "shadow-[0_0.5px_0_rgb(0_0_0/0.06)]"
          }`}
        >
          {children}
        </div>
      ) : null}

      {footer ? (
        <div
          className={`max-w-[min(78%,34rem)] ${
            hasBubble ? "mt-[0.2rem]" : "mt-0"
          } ${mine ? "self-end" : "self-start"}`}
        >
          {footer}
        </div>
      ) : null}

      <div className="mt-[0.15rem] flex items-center gap-[0.4rem] px-[0.35rem] text-[0.688rem] text-muted">
        <span>{timeLabel}</span>
        {meta}
      </div>
    </div>
  );
}

/** Full-width row chrome for services rendered as flat rows rather than bubbles. */
export function ServiceRow({
  messageId,
  isActive,
  children,
}: {
  messageId: string;
  isActive?: boolean;
  children: ReactNode;
}) {
  return (
    <div
      id={`msg-${messageId}`}
      className={`border-b border-border px-6 py-2 ${
        isActive ? "bg-search-active" : "bg-transparent"
      }`}
    >
      {children}
    </div>
  );
}

/**
 * Shared branded-service row: ServiceRow + sender/time header.
 * Color and optional header slots stay per-service; body is `children`.
 */
export function ServiceBubbleShell({
  message,
  isActive,
  senderClassName,
  senderStyle,
  timeClassName = "text-[0.75rem] text-muted",
  headerAlignClassName,
  headerExtra,
  children,
}: {
  message: Message;
  isActive?: boolean;
  senderClassName?: string;
  senderStyle?: CSSProperties;
  timeClassName?: string;
  /** Extra flex alignment on the header row (e.g. `items-center`). */
  headerAlignClassName?: string;
  headerExtra?: ReactNode;
  children: ReactNode;
}) {
  const mine = message.is_from_me;
  return (
    <ServiceRow messageId={message.id} isActive={isActive}>
      <div
        className={`mb-1 flex gap-2 ${headerAlignClassName ?? ""} ${
          mine ? "justify-end" : "justify-start"
        }`}
      >
        <span
          className={`text-[0.75rem] font-semibold ${senderClassName ?? ""}`}
          style={senderStyle}
        >
          {senderName(message)}
        </span>
        <span className={timeClassName}>
          {formatMessageTime(message.timestamp)}
        </span>
        {headerExtra}
      </div>
      {children}
    </ServiceRow>
  );
}

/** Body text of a flat service row, aligned by author and search-highlighted. */
export function ServiceMessageText({
  text,
  highlight,
  mine,
}: {
  text: string;
  highlight?: string;
  mine: boolean;
}) {
  return (
    <div
      className={`whitespace-pre-wrap text-[0.875rem] leading-[1.5] text-text ${
        mine ? "text-right" : "text-left"
      }`}
    >
      {highlight ? highlightText(text, highlight) : text}
    </div>
  );
}
