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

export default function DiscordBubble({
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
        display: "flex", gap: "0.5rem", alignItems: "center", marginBottom: "0.25rem",
        justifyContent: mine ? "flex-end" : "flex-start",
      }}>
        <span style={{
          fontSize: "0.75rem", fontWeight: 600,
          color: message.role_color || "#5865f2",
        }}>
          {senderName(message)}
        </span>
        <span style={{ fontSize: "0.688rem", color: "var(--muted)" }}>{time}</span>
      </div>

      <div style={{
        fontSize: "0.875rem", color: "var(--text)", lineHeight: 1.5,
        whiteSpace: "pre-wrap", textAlign: mine ? "right" : "left",
      }}>
        {highlight ? highlightText(message.text || "", highlight) : message.text || ""}
      </div>

      <MessageAttachments message={message} onAttachmentClick={onAttachmentClick} />

      {message.embeds && message.embeds.length > 0 && message.embeds.map((embed, i) => (
        <div key={i} style={{
          marginTop: "0.5rem", borderLeft: "4px solid #5865f2",
          background: "var(--hover)", padding: "0.5rem 0.75rem", borderRadius: "0 4px 4px 0",
        }}>
          {embed.title && (
            <div style={{ fontSize: "0.813rem", fontWeight: 600, marginBottom: "0.125rem" }}>
              {embed.url ? <a href={embed.url} style={{ color: "var(--accent)" }}>{embed.title}</a> : embed.title}
            </div>
          )}
          {embed.description && (
            <div style={{ fontSize: "0.813rem", color: "var(--muted)" }}>{embed.description}</div>
          )}
        </div>
      ))}

      {message.reactions && message.reactions.length > 0 && (
        <div style={{ display: "flex", gap: "0.375rem", marginTop: "0.25rem" }}>
          {message.reactions.map((r, i) => (
            <span key={i} style={{
              fontSize: "0.75rem", background: "var(--border)",
              padding: "0.125rem 0.375rem", borderRadius: "4px",
            }}>
              {r.emoji} {r.count}
            </span>
          ))}
        </div>
      )}
    </div>
  );
}
