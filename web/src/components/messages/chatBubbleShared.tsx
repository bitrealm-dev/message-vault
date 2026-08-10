import type { CSSProperties, ReactNode } from "react";
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
      <mark
        key={key++}
        style={{ background: "var(--search-mark)", borderRadius: "2px", padding: "0 1px" }}
      >
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

function bubbleColors(palette: BubblePalette, mine: boolean): {
  background: string;
  color: string;
} {
  if (mine) {
    if (palette === "imessage") {
      return { background: "var(--imessage-sent)", color: "var(--imessage-sent-text)" };
    }
    return { background: "var(--sms-sent)", color: "var(--sms-sent-text)" };
  }
  return { background: "var(--bubble-received)", color: "var(--bubble-received-text)" };
}

/** Chat-row chrome: aligned bubble, optional sender label, timestamp under bubble. */
export function ChatBubbleRow({
  messageId,
  mine,
  isActive,
  palette,
  showSender,
  senderLabel,
  senderColor,
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
  senderColor?: string;
  timeLabel: string;
  meta?: ReactNode;
  children?: ReactNode;
  footer?: ReactNode;
}) {
  const colors = bubbleColors(palette, mine);
  const radius: CSSProperties = mine
    ? {
        borderRadius: "18px",
        borderBottomRightRadius: "4px",
      }
    : {
        borderRadius: "18px",
        borderBottomLeftRadius: "4px",
      };
  const hasBubble = children != null && children !== false && children !== "";

  return (
    <div
      id={`msg-${messageId}`}
      style={{
        display: "flex",
        flexDirection: "column",
        alignItems: mine ? "flex-end" : "flex-start",
        padding: "0.2rem 1rem",
        marginBottom: mine ? "0.15rem" : "0.4rem",
        background: isActive ? "var(--search-active)" : "transparent",
      }}
    >
      {showSender && senderLabel ? (
        <div
          style={{
            fontSize: "0.75rem",
            fontWeight: 600,
            color: senderColor || "var(--muted)",
            marginBottom: "0.15rem",
            padding: "0 0.35rem",
            maxWidth: "min(78%, 34rem)",
          }}
        >
          {senderLabel}
        </div>
      ) : null}

      {hasBubble ? (
        <div
          style={{
            ...radius,
            background: colors.background,
            color: colors.color,
            padding: "0.45rem 0.7rem",
            maxWidth: "min(78%, 34rem)",
            fontSize: "0.9375rem",
            lineHeight: 1.35,
            whiteSpace: "pre-wrap",
            wordBreak: "break-word",
            boxShadow: mine ? "none" : "0 0.5px 0 rgb(0 0 0 / 0.06)",
          }}
        >
          {children}
        </div>
      ) : null}

      {footer ? (
        <div
          style={{
            marginTop: hasBubble ? "0.2rem" : 0,
            maxWidth: "min(78%, 34rem)",
            alignSelf: mine ? "flex-end" : "flex-start",
          }}
        >
          {footer}
        </div>
      ) : null}

      <div
        style={{
          display: "flex",
          gap: "0.4rem",
          alignItems: "center",
          marginTop: "0.15rem",
          padding: "0 0.35rem",
          fontSize: "0.688rem",
          color: "var(--muted)",
        }}
      >
        <span>{timeLabel}</span>
        {meta}
      </div>
    </div>
  );
}
