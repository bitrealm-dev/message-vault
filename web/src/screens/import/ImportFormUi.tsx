import type { CSSProperties, ReactNode } from "react";
import type { AttachmentMediaMode } from "../../lib/types";

export const ATTACHMENT_OPTIONS: { id: AttachmentMediaMode; label: string }[] = [
  { id: "copy", label: "Copy" },
  { id: "convert", label: "Convert" },
  { id: "compress", label: "Compress & Convert" },
  { id: "skip", label: "Skip" },
];

export const RESOLUTION_OPTIONS = ["720p", "1080p", "4k"];

export const fieldStyle: CSSProperties = {
  width: "100%",
  padding: "0.4rem 0.6rem",
  fontSize: "0.875rem",
  borderRadius: "6px",
  border: "1px solid var(--border)",
  boxSizing: "border-box",
  background: "var(--bg)",
  color: "var(--text)",
};

export const labelStyle: CSSProperties = {
  display: "block",
  fontSize: "0.875rem",
  fontWeight: 500,
  marginBottom: "0.35rem",
};

export const hintStyle: CSSProperties = {
  fontSize: "0.75rem",
  color: "var(--muted)",
  marginTop: "0.25rem",
};

export const sectionGap: CSSProperties = { marginBottom: "1.1rem" };

export const collapsibleHeaderStyle: CSSProperties = {
  display: "flex",
  alignItems: "center",
  gap: "0.5rem",
  width: "100%",
  padding: "0.35rem 0 0.5rem",
  margin: 0,
  border: "none",
  borderBottom: "1px solid var(--border)",
  borderRadius: 0,
  background: "transparent",
  fontSize: "0.9375rem",
  fontWeight: 600,
  color: "var(--text)",
  cursor: "pointer",
  textAlign: "left",
};

export function StackedField({
  label,
  children,
  trailing,
}: {
  label: string;
  children: ReactNode;
  trailing?: ReactNode;
}) {
  return (
    <div style={sectionGap}>
      <div style={{ display: "flex", alignItems: "baseline", gap: "0.75rem" }}>
        <label style={{ ...labelStyle, flex: 1, marginBottom: "0.35rem" }}>{label}</label>
        {trailing}
      </div>
      {children}
    </div>
  );
}

export function CollapsibleSection({
  title,
  open,
  onToggle,
  children,
}: {
  title: string;
  open: boolean;
  onToggle: () => void;
  children: ReactNode;
}) {
  return (
    <div style={{ marginBottom: open ? "0.75rem" : "1.25rem" }}>
      <button type="button" onClick={onToggle} style={collapsibleHeaderStyle} aria-expanded={open}>
        <span
          style={{
            display: "inline-block",
            transform: open ? "rotate(90deg)" : "none",
            transition: "transform 0.15s ease",
            fontSize: "0.75rem",
            color: "var(--muted)",
            lineHeight: 1,
          }}
          aria-hidden
        >
          ▶
        </span>
        <span>{title}</span>
      </button>
      {open ? (
        <div style={{ marginTop: "0.75rem", marginLeft: "0.75rem" }}>{children}</div>
      ) : null}
    </div>
  );
}
