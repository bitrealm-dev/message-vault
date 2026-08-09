import type { ReactNode } from "react";
import type { Message, MessageAttachment } from "../../lib/types";
import MessageAttachments from "../MessageAttachments";

function highlightText(text: string, term: string): ReactNode[] {
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
      <mark key={key++} style={{ background: "var(--search-mark)", borderRadius: "2px", padding: "0 1px" }}>
        {rest.slice(idx, idx + t.length)}
      </mark>,
    );
    rest = rest.slice(idx + t.length);
  }
  return out;
}

function senderName(m: Message): string {
  if (m.is_from_me) return "Me";
  if (m.sender) {
    const p = m.conversation.participants.find((x) => x.handle === m.sender);
    return p?.name_hint || m.sender;
  }
  const p = m.conversation.participants[0];
  return p ? p.name_hint || p.handle : "Unknown";
}

export default function InstagramBubble({
  message,
  highlight,
  isActive,
  onAttachmentClick,
}: {
  message: Message;
  highlight?: string;
  isActive?: boolean;
  onAttachmentClick?: (attachment: MessageAttachment, source: string) => void;
}) {
  const time = new Date(message.timestamp).toLocaleString([], {
    month: "short", day: "numeric", hour: "numeric", minute: "2-digit",
  });
  const mine = message.is_from_me;

  return (
    <div id={`msg-${message.id}`} style={{
      padding: "0.5rem 1.5rem", borderBottom: "1px solid var(--border)",
      background: isActive ? "var(--search-active)" : "transparent",
    }}>
      <div style={{
        display: "flex", gap: "0.5rem", marginBottom: "0.25rem",
        justifyContent: mine ? "flex-end" : "flex-start",
      }}>
        <span style={{ fontSize: "0.75rem", fontWeight: 600, color: "#e4405f" }}>
          {senderName(message)}
        </span>
        <span style={{ fontSize: "0.75rem", color: "var(--muted)" }}>{time}</span>
        {message.is_story_reply && (
          <span style={{ fontSize: "0.688rem", color: "#e4405f" }}>Story reply</span>
        )}
        {message.forwarded && (
          <span style={{ fontSize: "0.688rem", color: "var(--muted)" }}>Forwarded</span>
        )}
      </div>

      <div style={{
        fontSize: "0.875rem", color: "var(--text)", lineHeight: 1.5,
        whiteSpace: "pre-wrap", textAlign: mine ? "right" : "left",
      }}>
        {highlight ? highlightText(message.text || "", highlight) : message.text || ""}
      </div>

      <MessageAttachments message={message} onAttachmentClick={onAttachmentClick} />
    </div>
  );
}
