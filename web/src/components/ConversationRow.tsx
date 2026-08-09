import { useRef, useState } from "react";
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
  const [editing, setEditing] = useState(false);
  const [labelValue, setLabelValue] = useState(conversation.label || "");
  const editBaselineRef = useRef(conversation.label || "");
  const cancelEditRef = useRef(false);

  const startEditing = (e: React.MouseEvent) => {
    e.stopPropagation();
    const baseline = conversation.label || displayName(conversation);
    editBaselineRef.current = baseline;
    cancelEditRef.current = false;
    setLabelValue(baseline);
    setEditing(true);
  };

  const handleSaveLabel = () => {
    if (cancelEditRef.current) {
      cancelEditRef.current = false;
      setEditing(false);
      return;
    }
    const next = labelValue.trim();
    // Clicking the name alone must not rename — only persist a real change.
    if (next === editBaselineRef.current.trim()) {
      setEditing(false);
      return;
    }
    // Store locally — the API endpoint for persisting labels is follow-up (Tier 4)
    conversation.label = next || null;
    setEditing(false);
  };

  return (
    <button
      onClick={onClick}
      style={{
        display: "flex", width: "100%", textAlign: "left", border: "none",
        background: isSelected ? "var(--hover)" : "transparent",
        padding: "0.5rem 0.75rem", cursor: "pointer",
        borderBottom: "1px solid var(--border)", gap: "0.5rem", alignItems: "flex-start",
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
        {editing ? (
          <input
            type="text"
            value={labelValue}
            onChange={(e) => setLabelValue(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") handleSaveLabel();
              if (e.key === "Escape") {
                cancelEditRef.current = true;
                setEditing(false);
              }
            }}
            onBlur={handleSaveLabel}
            onClick={(e) => e.stopPropagation()}
            autoFocus
            style={{
              fontSize: "0.875rem",
              fontWeight: 500,
              width: "100%",
              padding: "0.125rem 0.25rem",
              background: "var(--bg)",
              color: "var(--text)",
              border: "1px solid var(--border)",
              borderRadius: "4px",
            }}
          />
        ) : (
          <span
            onClick={startEditing}
            title="Click to rename"
            style={{
              cursor: "pointer",
              fontSize: "0.875rem", fontWeight: 500, color: "var(--text)",
              overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap",
              flex: 1, marginRight: "0.5rem",
            }}
          >
            {displayName(conversation)}
            {conversation.label && <span style={{ fontSize: "0.688rem", color: "var(--muted)", marginLeft: "0.25rem" }}>(renamed)</span>}
          </span>
        )}
        <span style={{ fontSize: "0.75rem", color: "var(--muted)", flexShrink: 0 }}>
          {formatDate(conversation.last_message_at)}
        </span>
      </div>
      <div style={{ fontSize: "0.75rem", color: "var(--muted)" }}>
        {subtitle(conversation)}
      </div>
      </div>
    </button>
  );
}
