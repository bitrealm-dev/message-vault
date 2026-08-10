import { useEffect, useRef, useState, type CSSProperties, type ReactNode } from "react";
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

function CalendarIcon() {
  return (
    <svg
      width="15"
      height="15"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden
    >
      <rect x="3" y="4" width="18" height="18" rx="2" />
      <path d="M16 2v4" />
      <path d="M8 2v4" />
      <path d="M3 10h18" />
    </svg>
  );
}

/** Format typed digits as mm/dd/yyyy with slashes kept. */
export function formatMmDdYyyyInput(raw: string): string {
  const digits = raw.replace(/\D/g, "").slice(0, 8);
  if (digits.length <= 2) return digits;
  if (digits.length <= 4) return `${digits.slice(0, 2)}/${digits.slice(2)}`;
  return `${digits.slice(0, 2)}/${digits.slice(2, 4)}/${digits.slice(4)}`;
}

/** `YYYY-MM-DD` → `mm/dd/yyyy`. */
export function isoToDisplay(iso: string): string {
  const m = /^(\d{4})-(\d{2})-(\d{2})$/.exec(iso.trim());
  if (!m) return "";
  return `${m[2]}/${m[3]}/${m[1]}`;
}

/** `mm/dd/yyyy` → `YYYY-MM-DD`, or null if incomplete/invalid. */
export function displayToIso(display: string): string | null {
  const m = /^(\d{2})\/(\d{2})\/(\d{4})$/.exec(display.trim());
  if (!m) return null;
  const month = Number(m[1]);
  const day = Number(m[2]);
  const year = Number(m[3]);
  if (month < 1 || month > 12 || day < 1 || day > 31 || year < 1000) return null;
  const dt = new Date(year, month - 1, day);
  if (
    dt.getFullYear() !== year ||
    dt.getMonth() !== month - 1 ||
    dt.getDate() !== day
  ) {
    return null;
  }
  return `${String(year).padStart(4, "0")}-${m[1]}-${m[2]}`;
}

/**
 * Text field for typing mm/dd/yyyy (slashes auto-inserted) plus a calendar
 * button that opens a native date picker. Parent value is ISO YYYY-MM-DD or "".
 */
export function DateField({
  label,
  value,
  onChange,
}: {
  label: string;
  value: string;
  onChange: (value: string) => void;
}) {
  const pickerRef = useRef<HTMLInputElement>(null);
  const [text, setText] = useState(() => (value ? isoToDisplay(value) : ""));

  useEffect(() => {
    const next = value ? isoToDisplay(value) : "";
    setText((prev) => {
      const prevIso = displayToIso(prev);
      if (value && prevIso === value) return prev;
      if (!value && prev === "") return prev;
      if (!value && displayToIso(prev) === null && prev !== "") return prev;
      return next;
    });
  }, [value]);

  const commitText = (next: string) => {
    setText(next);
    if (next === "") {
      onChange("");
      return;
    }
    const iso = displayToIso(next);
    if (iso) onChange(iso);
    else if (value) onChange("");
  };

  const openPicker = () => {
    const el = pickerRef.current;
    if (!el) return;
    try {
      el.showPicker();
    } catch {
      el.focus();
      el.click();
    }
  };

  return (
    <div style={{ flex: "1 1 12rem", minWidth: "10rem" }}>
      <label style={{ ...labelStyle, marginBottom: "0.35rem" }}>{label}</label>
      <div style={{ position: "relative" }}>
        <input
          type="text"
          inputMode="numeric"
          autoComplete="off"
          placeholder="mm/dd/yyyy"
          value={text}
          onChange={(e) => commitText(formatMmDdYyyyInput(e.target.value))}
          style={{
            ...fieldStyle,
            paddingRight: "2.25rem",
          }}
        />
        <input
          ref={pickerRef}
          type="date"
          value={value && /^\d{4}-\d{2}-\d{2}$/.test(value) ? value : ""}
          onChange={(e) => {
            const iso = e.target.value;
            onChange(iso);
            setText(iso ? isoToDisplay(iso) : "");
          }}
          className="mv-date-field"
          tabIndex={-1}
          aria-hidden
          style={{
            position: "absolute",
            width: 1,
            height: 1,
            opacity: 0,
            pointerEvents: "none",
            overflow: "hidden",
            clip: "rect(0 0 0 0)",
          }}
        />
        <button
          type="button"
          onClick={openPicker}
          title="Pick a date"
          aria-label={`Pick ${label}`}
          style={{
            position: "absolute",
            right: "0.25rem",
            top: "50%",
            transform: "translateY(-50%)",
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            width: "1.75rem",
            height: "1.75rem",
            padding: 0,
            border: "none",
            borderRadius: "4px",
            background: "transparent",
            color: "var(--muted)",
            cursor: "pointer",
          }}
        >
          <CalendarIcon />
        </button>
      </div>
    </div>
  );
}

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
