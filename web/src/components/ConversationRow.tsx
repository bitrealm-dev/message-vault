import { useRef, useState, type ReactNode } from "react";
import type { Conversation } from "../lib/types";
import { useListColumnResizing } from "./ListColumnResizeContext";

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

/** Month + year for conversation date ranges (e.g. "Sep 2020"). */
function formatMonthYear(iso: string): string {
  return new Date(iso).toLocaleDateString([], { month: "short", year: "numeric" });
}

function formatDateRange(start: string | null, end: string | null): string | null {
  if (!start || !end) return null;
  return `${formatMonthYear(start)} – ${formatMonthYear(end)}`;
}

function isLargeGroup(conv: Conversation): boolean {
  return conv.is_group && conv.participants.length > 7;
}

/** Short service label for group meta (no message counts). */
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

function ParticipantCount({ count }: { count: number }) {
  return (
    <span style={{ display: "inline-flex", alignItems: "center", gap: "0.35rem" }}>
      {count}
      <GroupIcon />
    </span>
  );
}

/** Meta line: participant count, optional service, optional date range. */
function MetaLine({
  participantCount,
  service,
  range,
}: {
  participantCount: number;
  service?: string | null;
  range?: string | null;
}) {
  const serviceLabel = service ? formatServiceLabel(service) : null;
  return (
    <span
      style={{
        display: "inline-flex",
        alignItems: "center",
        gap: "0.65rem",
        minWidth: 0,
        flexWrap: "wrap",
      }}
    >
      <ParticipantCount count={participantCount} />
      {serviceLabel ? (
        <>
          <span aria-hidden="true" style={{ opacity: 0.55 }}>·</span>
          <span>{serviceLabel}</span>
        </>
      ) : null}
      {range ? (
        <>
          <span aria-hidden="true" style={{ opacity: 0.55 }}>·</span>
          <span>{range}</span>
        </>
      ) : null}
    </span>
  );
}

/** Plain-text title used for rename baseline (no icon). */
function displayNameText(conv: Conversation): string {
  if (conv.label) return conv.label;

  if (!conv.is_group) {
    const p = conv.participants[0];
    return p?.name || p?.handle || "(unknown)";
  }

  if (conv.participants.length <= 7) {
    return conv.participants.map((p) => p.name || p.handle).join(", ");
  }

  const parts = [`${conv.participants.length}`];
  const serviceLabel = formatServiceLabel(conv.service);
  if (serviceLabel) parts.push(serviceLabel);
  const range = formatDateRange(conv.date_range_start, conv.date_range_end);
  if (range) parts.push(range);
  return parts.join(" · ");
}

function titleContent(conv: Conversation): ReactNode {
  if (conv.label) return conv.label;

  if (!conv.is_group) {
    const p = conv.participants[0];
    return p?.name || p?.handle || "(unknown)";
  }

  if (conv.participants.length <= 7) {
    // Each preferred name stays on one line; wrap between people.
    return (
      <span>
        {conv.participants.map((p, i) => {
          const label = p.name || p.handle;
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

  // Large groups: count + icon · service · date range
  return (
    <MetaLine
      participantCount={conv.participants.length}
      service={conv.service}
      range={formatDateRange(conv.date_range_start, conv.date_range_end)}
    />
  );
}

/** Small-group name lists wrap; other titles stay single-line. */
function titleWraps(conv: Conversation): boolean {
  return (
    conv.is_group &&
    !conv.label &&
    conv.participants.length >= 2 &&
    conv.participants.length <= 7
  );
}

function subtitleContent(conv: Conversation): ReactNode {
  if (!conv.is_group) {
    const p = conv.participants[0];
    return p ? `${p.handle} · ${p.service}` : null;
  }

  // Large groups put everything in the title line.
  if (isLargeGroup(conv)) return null;

  // Small groups: N + icon · service
  return (
    <MetaLine
      participantCount={conv.participants.length}
      service={conv.service}
    />
  );
}

/** Trailing last-message date: hidden for unlabeled large groups (range is in the title). */
function showTrailingDate(conv: Conversation): boolean {
  return !isLargeGroup(conv) || !!conv.label;
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
    // Clicking the name alone must not rename — only persist a real change.
    if (next === editBaselineRef.current.trim()) {
      setEditing(false);
      return;
    }
    // Store locally — the API endpoint for persisting labels is follow-up (Tier 4)
    conversation.label = next || null;
    setEditing(false);
  };

  const sub = subtitleContent(conversation);
  const columnResizing = useListColumnResizing();
  // Freeze wrapping while the column width is dragged — avoids per-frame reflow jitter.
  const wraps = titleWraps(conversation) && !columnResizing;

  return (
    <button
      onClick={onClick}
      style={{
        display: "flex",
        width: "100%",
        // Content-driven height so dynamic virtual rows measure wrap correctly.
        // height:100% kept the previous estimated row size and caused overlaps.
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
              flex: 1, marginRight: "0.5rem",
              ...(wraps
                ? { whiteSpace: "normal", overflow: "visible", lineHeight: 1.35 }
                : { overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }),
            }}
          >
            {titleContent(conversation)}
            {conversation.label && <span style={{ fontSize: "0.688rem", color: "var(--muted)", marginLeft: "0.25rem" }}>(renamed)</span>}
          </span>
        )}
        {showTrailingDate(conversation) ? (
          <span style={{ fontSize: "0.75rem", color: "var(--muted)", flexShrink: 0 }}>
            {formatDate(conversation.last_message_at)}
          </span>
        ) : null}
      </div>
      {sub ? (
        <div style={{ fontSize: "0.75rem", color: "var(--muted)" }}>
          {sub}
        </div>
      ) : null}
      </div>
    </button>
  );
}
