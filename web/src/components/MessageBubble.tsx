import type { Message } from "../lib/types";

export default function MessageBubble({ message }: { message: Message }) {
  const time = new Date(message.sent_at).toLocaleString([], {
    month: "short", day: "numeric", year: "numeric",
    hour: "numeric", minute: "2-digit",
  });

  return (
    <div style={{ padding: "0.5rem 1.5rem", borderBottom: "1px solid #f3f4f6" }}>
      <div style={{ display: "flex", gap: "0.5rem", marginBottom: "0.25rem" }}>
        <span style={{ fontSize: "0.75rem", fontWeight: 600, color: "#374151" }}>
          {message.sender.name || message.sender.handle}
        </span>
        <span style={{ fontSize: "0.75rem", color: "#9ca3af" }}>{time}</span>
      </div>
      <div style={{ fontSize: "0.875rem", color: "#1f2937", lineHeight: 1.5, whiteSpace: "pre-wrap" }}>
        {message.body}
      </div>
    </div>
  );
}
