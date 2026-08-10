import type { ReactNode } from "react";
import type { Message } from "../../lib/types";

export type BubblePalette = "imessage" | "sms";

export function highlightText(text: string, term: string): ReactNode[] {
  const t = term.trim().toLowerCase();
  if (!t) return [text];
  const out: ReactNode[] = [];
  let rest = text;
  let key = 0;
  while (true) {
    const idx = rest.toLowerCase().indexOf(t);
    if (idx === -1) {
      out.push(rest);
      break;
    }
    if (idx > 0) out.push(rest.slice(0, idx));
    out.push(
      <mark key={key++} className="rounded-sm bg-search-mark px-px">
        {rest.slice(idx, idx + t.length)}
      </mark>,
    );
    rest = rest.slice(idx + t.length);
  }
  return out;
}

export function senderName(m: Message): string {
  if (m.is_from_me) return "Me";
  if (m.sender) {
    const p = m.conversation.participants.find((x) => x.handle === m.sender);
    return p?.name_hint || m.sender;
  }
  const p = m.conversation.participants[0];
  return p ? p.name_hint || p.handle : "Unknown";
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
