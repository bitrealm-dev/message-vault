import type { Conversation } from "../lib/types";

function formatDate(iso: string): string {
  const d = new Date(iso);
  const now = new Date();
  const diffDays = Math.floor((now.getTime() - d.getTime()) / 86400000);

  if (diffDays === 0) {
    return d.toLocaleTimeString([], { hour: "numeric", minute: "2-digit" });
  }
  if (diffDays === 1) return "yesterday";
  if (diffDays < 7) return `${diffDays}d ago`;
  if (d.getFullYear() === now.getFullYear()) {
    return d.toLocaleDateString([], { month: "short", day: "numeric" });
  }
  return d.toLocaleDateString([], { month: "short", day: "numeric", year: "numeric" });
}

function displayName(conv: Conversation): string {
  if (conv.label) return conv.label;

  if (!conv.is_group) {
    const p = conv.participants[0];
    return p?.name || p?.handle || "(unknown)";
  }

  if (conv.participants.length <= 7) {
    return conv.participants.map((p) => p.name || p.handle).join(", ");
  }

  return `${conv.participants.length} participants · ${conv.message_count} messages`;
}

function subtitle(conv: Conversation): string {
  if (conv.is_group) {
    const parts = [conv.service];
    if (conv.date_range_start && conv.date_range_end) {
      const s = new Date(conv.date_range_start);
      const e = new Date(conv.date_range_end);
      const fmt = (d: Date) =>
        d.toLocaleDateString([], { month: "short", year: "numeric" });
      parts.push(`${fmt(s)} – ${fmt(e)}`);
    }
    return parts.join(" · ");
  }
  const p = conv.participants[0];
  return p ? `${p.handle} · ${p.service}` : "";
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
  return (
    <button
      onClick={onClick}
      style={{
        display: "flex", width: "100%", textAlign: "left", border: "none",
        background: isSelected ? "#e5e7eb" : "transparent",
        padding: "0.5rem 0.75rem", cursor: "pointer",
        borderBottom: "1px solid #f3f4f6", gap: "0.5rem", alignItems: "flex-start",
      }}
    >
      {onCheckChange && (
        <input
          type="checkbox"
          checked={checked || false}
          onChange={(e) => { e.stopPropagation(); onCheckChange(conversation.id); }}
          onClick={(e) => e.stopPropagation()}
          style={{ marginTop: "2px", flexShrink: 0 }}
        />
      )}
      <div style={{ flex: 1, minWidth: 0 }}>
      <div style={{
        display: "flex", justifyContent: "space-between", alignItems: "baseline",
        marginBottom: "2px",
      }}>
        <span style={{
          fontSize: "0.875rem", fontWeight: 500, color: "#1f2937",
          overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap",
          flex: 1, marginRight: "0.5rem",
        }}>
          {displayName(conversation)}
        </span>
        <span style={{ fontSize: "0.75rem", color: "#9ca3af", flexShrink: 0 }}>
          {formatDate(conversation.last_message_at)}
        </span>
      </div>
      <div style={{ fontSize: "0.75rem", color: "#6b7280" }}>
        {subtitle(conversation)}
      </div>
      </div>
    </button>
  );
}
