import { useState } from "react";
import { normalizeHex, type ThemeSeeds } from "../../lib/theme";

export function ColorRow({
  label,
  value,
  onChange,
}: {
  label: string;
  value: string;
  onChange: (hex: string) => void;
}) {
  const [text, setText] = useState(value);
  const [prevValue, setPrevValue] = useState(value);
  if (value !== prevValue) {
    setPrevValue(value);
    setText(value);
  }

  return (
    <div style={{ display: "flex", alignItems: "center", gap: "0.75rem" }}>
      <label
        style={{
          width: "7rem",
          flexShrink: 0,
          fontSize: "13px",
          color: "var(--muted)",
        }}
      >
        {label}
      </label>
      <input
        type="color"
        value={normalizeHex(value) ?? "#000000"}
        onChange={(e) => onChange(e.target.value)}
        aria-label={label}
        style={{
          height: "2.25rem",
          width: "3rem",
          cursor: "pointer",
          borderRadius: "0.375rem",
          border: "1px solid var(--border)",
          background: "var(--panel)",
          padding: "2px",
        }}
      />
      <input
        type="text"
        value={text}
        spellCheck={false}
        onChange={(e) => setText(e.target.value)}
        onBlur={() => {
          const hex = normalizeHex(text);
          if (hex) onChange(hex);
          else setText(value);
        }}
        onKeyDown={(e) => {
          if (e.key === "Enter") {
            (e.target as HTMLInputElement).blur();
          }
        }}
        style={{
          minWidth: 0,
          flex: 1,
          borderRadius: "0.375rem",
          border: "1px solid var(--border)",
          background: "var(--bg)",
          padding: "0.375rem 0.625rem",
          fontFamily: "ui-monospace, monospace",
          fontSize: "13px",
          color: "var(--text)",
          outline: "none",
        }}
      />
    </div>
  );
}

export function formatCompare(seeds: ThemeSeeds): string {
  return [
    seeds.lightHeader,
    seeds.lightAccent,
    seeds.darkHeader,
    seeds.darkAccent,
  ]
    .map((h) => h.toLowerCase())
    .join(",");
}
