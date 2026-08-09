import { useEffect, useRef, useState } from "react";
import { apiClient } from "../lib/api";

const OPERATORS = [
  "from:", "to:", "with:", "within:", "label:", "handle:",
  "has:", "after:", "before:", "source:", "subject:", "is:",
];

interface ContactName {
  id: string;
  name: string;
}

export default function GlobalSearch({
  value,
  onChange,
  onSubmit,
  mode = "search",
}: {
  value: string;
  onChange: (v: string) => void;
  onSubmit: (q: string) => void;
  /** `filter` = live contact list filter (no vault operators). */
  mode?: "search" | "filter";
}) {
  const isFilter = mode === "filter";
  const [open, setOpen] = useState(false);
  const [highlight, setHighlight] = useState(0);
  const [contacts, setContacts] = useState<ContactName[]>([]);
  const inputRef = useRef<HTMLInputElement>(null);

  const lastToken = value.split(/\s+/).pop() || "";
  const colonIdx = lastToken.indexOf(":");
  const completingValue = colonIdx !== -1;
  const opPrefix = completingValue ? lastToken.slice(0, colonIdx + 1) : "";
  const valuePart = completingValue ? lastToken.slice(colonIdx + 1).replace(/^"|"$/g, "") : "";

  useEffect(() => {
    if (isFilter || !completingValue) {
      setContacts([]);
      return;
    }
    const ac = new AbortController();
    const t = window.setTimeout(() => {
      const params = new URLSearchParams({
        q: valuePart,
        limit: "20",
        offset: "0",
      });
      apiClient
        .get<{ contacts: ContactName[] }>(`/v1/export/contacts?${params}`, {
          signal: ac.signal,
        })
        .then((res) =>
          setContacts(
            (res.contacts || []).map((c) => ({
              ...c,
              id: String(c.id),
            })),
          ),
        )
        .catch(() => {
          if (!ac.signal.aborted) setContacts([]);
        });
    }, 150);
    return () => {
      window.clearTimeout(t);
      ac.abort();
    };
  }, [isFilter, completingValue, valuePart]);

  const suggestions: string[] = isFilter
    ? []
    : completingValue
      ? contacts.map((c) => c.name).slice(0, 6)
      : OPERATORS.filter((op) => op.startsWith(lastToken.toLowerCase())).slice(0, 6);

  const applySuggestion = (s: string) => {
    const tokens = value.split(/\s+/);
    tokens.pop();
    const next = completingValue
      ? tokens.concat(`${opPrefix}"${s}"`).join(" ")
      : tokens.concat(`${s} `).join(" ");
    onChange(next);
    setOpen(false);
    inputRef.current?.focus();
  };

  return (
    <div style={{ position: "relative" }}>
      <input
        ref={inputRef}
        type="search"
        value={value}
        onChange={(e) => {
          onChange(e.target.value);
          if (!isFilter) {
            setOpen(true);
            setHighlight(0);
          }
        }}
        onFocus={() => {
          if (!isFilter) setOpen(true);
        }}
        onBlur={() => setTimeout(() => setOpen(false), 150)}
        onKeyDown={(e) => {
          if (e.key === "Enter") {
            if (!isFilter && open && suggestions.length > 0) {
              applySuggestion(suggestions[Math.min(highlight, suggestions.length - 1)]);
            } else {
              onSubmit(value);
            }
          } else if (!isFilter && e.key === "ArrowDown") {
            e.preventDefault();
            setOpen(true);
            setHighlight((h) => (suggestions.length ? (h + 1) % suggestions.length : 0));
          } else if (!isFilter && e.key === "ArrowUp") {
            e.preventDefault();
            setHighlight((h) => (suggestions.length ? (h - 1 + suggestions.length) % suggestions.length : 0));
          } else if (e.key === "Escape") {
            setOpen(false);
          }
        }}
        placeholder={
          isFilter
            ? "Filter by name or handle…"
            : "Search vault — try from: or has:"
        }
        style={{
          width: "100%", padding: "0.375rem 0.5rem", fontSize: "0.813rem",
          border: "1px solid var(--border)", borderRadius: "4px",
          boxSizing: "border-box",
          background: "var(--bg)",
          color: "var(--text)",
        }}
      />
      {!isFilter && open && suggestions.length > 0 && (
        <div style={{
          position: "absolute", top: "100%", left: 0, right: 0,
          background: "var(--panel)", border: "1px solid var(--border)",
          borderRadius: "4px", boxShadow: "0 4px 6px rgba(0,0,0,0.1)",
          zIndex: 30, maxHeight: "200px", overflow: "auto",
        }}>
          {suggestions.map((s, i) => (
            <button
              key={s}
              type="button"
              onMouseDown={(e) => { e.preventDefault(); applySuggestion(s); }}
              style={{
                display: "block", width: "100%", textAlign: "left", border: "none",
                background: i === highlight ? "var(--hover)" : "transparent",
                padding: "0.375rem 0.75rem", fontSize: "0.813rem", cursor: "pointer",
                color: "var(--text)",
              }}
            >
              {s}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
