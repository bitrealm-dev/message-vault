import type { ReactNode } from "react";
import type { Message } from "../../lib/types";

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
      <mark key={key++} style={{ background: "#fde68a", borderRadius: "2px", padding: "0 1px" }}>
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

export default function ImessageBubble({
  message,
  highlight,
  isActive,
}: {
  message: Message;
  highlight?: string;
  isActive?: boolean;
}) {
  const time = new Date(message.timestamp).toLocaleString([], {
    month: "short", day: "numeric", hour: "numeric", minute: "2-digit",
  });
  const mine = message.is_from_me;

  return (
    <div id={`msg-${message.id}`} style={{
      padding: "0.5rem 1.5rem", borderBottom: "1px solid #f3f4f6",
      background: isActive ? "#fef9c3" : "transparent",
    }}>
      <div style={{
        display: "flex", gap: "0.5rem", marginBottom: "0.25rem",
        justifyContent: mine ? "flex-end" : "flex-start",
      }}>
        <span style={{ fontSize: "0.75rem", fontWeight: 600, color: "#007aff" }}>
          {senderName(message)}
        </span>
        <span style={{ fontSize: "0.75rem", color: "#9ca3af" }}>{time}</span>
        {message.effect && (
          <span style={{ fontSize: "0.688rem", color: "#8b5cf6", fontStyle: "italic" }}>
            {message.effect}
          </span>
        )}
      </div>

      {/* Edit history indicator */}
      {message.edit_history && message.edit_history.length > 0 && (
        <div style={{ fontSize: "0.688rem", color: "#9ca3af", fontStyle: "italic", marginBottom: "0.25rem" }}>
          Edited
        </div>
      )}

      <div style={{
        fontSize: "0.875rem", color: "#1f2937", lineHeight: 1.5,
        whiteSpace: "pre-wrap", textAlign: mine ? "right" : "left",
      }}>
        {highlight ? highlightText(message.text || "", highlight) : message.text || ""}
      </div>

      {/* Tapback reactions */}
      {message.reactions && message.reactions.length > 0 && (
        <div style={{ display: "flex", gap: "0.375rem", marginTop: "0.25rem" }}>
          {message.reactions.map((r, i) => (
            <span key={i} style={{
              fontSize: "0.75rem", background: "#f3f4f6",
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
