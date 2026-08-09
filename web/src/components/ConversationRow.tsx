import { useRef, useState, type ReactNode } from "react";
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
      style={{ display: "inline-block", verticalAlign: "-1px", flexShrink: 0 }}
    >
      <path d="M17 21v-2a4 4 0 0 0-4-4H5a4 4 0 0 0-4 4v2" />
      <circle cx="9" cy="7" r="4" />
      <path d="M23 21v-2a4 4 0 0 0-3-3.87" />
      <path d="M16 3.13a4 4 0 0 1 0 7.75" />
    </svg>
  );
}

function participantLabel(p: { name: string | null; handle: string }): string {
  return p.name || p.handle;
}

/** Plain-text title used for rename baseline. */
function displayNameText(conv: Conversation): string {
  if (conv.label) return conv.label;
  if (!conv.is_group) {
    const p = conv.participants[0];
    return p?.name || p?.handle || "(unknown)";
  }
  return conv.participants.map(participantLabel).join(", ");
}

const NAME_LINE_HEIGHT = 1.35;

/** Comma-separated names; each name stays whole; at most two lines then ellipsis. */
function GroupNames({ conv }: { conv: Conversation }) {
  return (
    <span
      style={{
        display: "-webkit-box",
        WebkitBoxOrient: "vertical",
        WebkitLineClamp: 2,
        overflow: "hidden",
        lineHeight: NAME_LINE_HEIGHT,
        wordBreak: "normal",
        overflowWrap: "break-word",
      }}
    >
      {conv.participants.map((p, i) => {
        const label = participantLabel(p);
        return (
          <span key={`${p.handle}-${i}`}>
            {i > 0 ? ", " : null}
            <span style={{ whiteSpace: "nowrap" }}>{label}</span>
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

function Dot() {
  return (
    <span aria-hidden="true" style={{ opacity: 0.55, margin: "0 0.15rem" }}>
      ·
    </span>
  );
}

/** Bottom-left for groups: service · icon + count. */
function GroupMeta({ conv }: { conv: Conversation }) {
  const serviceLabel = formatServiceLabel(conv.service);
  return (
    <span
      style={{
        display: "inline-flex",
        alignItems: "center",
        gap: "0.35rem",
        minWidth: 0,
        flexWrap: "wrap",
      }}
    >
      {serviceLabel ? <span>{serviceLabel}</span> : null}
      {serviceLabel ? <Dot /> : null}
      <GroupIcon />
      <span>{conv.participants.length}</span>
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
  const [editing, setEditing] = useState(false);
  const [labelValue, setLabelValue] = useState(conversation.label || "");
  const editBaselineRef = useRef(conversation.label || "");
  const cancelEditRef = useRef(false);

  const startEditing = (e: React.MouseEvent) => {
    e.stopPropagation();
    const baseline = conversation.label || displayNameText(conversation);
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
    if (next === editBaselineRef.current.trim()) {
      setEditing(false);
      return;
    }
    // Store locally — the API endpoint for persisting labels is follow-up (Tier 4)
    conversation.label = next || null;
    setEditing(false);
  };

  const columnResizing = useListColumnResizing();
  const isGroup = conversation.is_group;
  const wraps = isGroup && !conversation.label && !columnResizing;
  const dateSpan = formatDateSpan(
    conversation.date_range_start,
    conversation.last_message_at || conversation.date_range_end,
  );
  const bottomLeft = isGroup ? (
    <GroupMeta conv={conversation} />
  ) : (
    directService(conversation)
  );

  return (
    <button
      onClick={onClick}
      style={{
        display: "flex",
        width: "100%",
        boxSizing: "border-box",
        textAlign: "left",
        border: "none",
        background: isSelected ? "var(--hover)" : "transparent",
        padding: "0.5rem 0.75rem",
        cursor: "pointer",
        borderBottom: "1px solid var(--border)",
        gap: "0.5rem",
        alignItems: "flex-start",
      }}
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
          style={{ marginTop: "2px", flexShrink: 0 }}
        />
      )}
      <div
        style={{
          flex: 1,
          minWidth: 0,
          display: "flex",
          flexDirection: "column",
          gap: "2px",
        }}
      >
        {editing ? (
          <input
            type="text"
            value={labelValue}
            onChange={(e) => setLabelValue(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") handleSaveLabel();
              if (e.key === "Escape") {
                cancelEditRef.current = true;
                handleSaveLabel();
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
              fontSize: "0.875rem",
              fontWeight: 500,
              color: "var(--text)",
              minWidth: 0,
              ...(wraps
                ? { overflow: "hidden" }
                : {
                    overflow: "hidden",
                    textOverflow: "ellipsis",
                    whiteSpace: "nowrap",
                  }),
            }}
          >
            {titleContent(conversation)}
            {conversation.label ? (
              <span
                style={{
                  fontSize: "0.688rem",
                  color: "var(--muted)",
                  marginLeft: "0.25rem",
                }}
              >
                (renamed)
              </span>
            ) : null}
          </span>
        )}

        <div
          style={{
            display: "flex",
            justifyContent: "space-between",
            alignItems: "baseline",
            gap: "0.5rem",
            fontSize: "0.75rem",
            color: "var(--muted)",
          }}
        >
          <span style={{ minWidth: 0, overflow: "hidden", textOverflow: "ellipsis" }}>
            {bottomLeft}
          </span>
          {dateSpan ? (
            <span style={{ flexShrink: 0, textAlign: "right" }}>{dateSpan}</span>
          ) : null}
        </div>
      </div>
    </button>
  );
}
