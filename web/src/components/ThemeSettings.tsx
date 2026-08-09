import { useState, type CSSProperties } from "react";
import { normalizeHex, type ThemeSeeds } from "../lib/theme";
import { useTheme } from "../lib/ThemeProvider";

const SEED_FIELDS: {
  key: keyof ThemeSeeds;
  label: string;
}[] = [
  { key: "lightHeader", label: "Light header" },
  { key: "lightAccent", label: "Light accent" },
  { key: "darkHeader", label: "Dark header" },
  { key: "darkAccent", label: "Dark accent" },
];

const sectionTitle: CSSProperties = {
  margin: 0,
  fontSize: "12px",
  fontWeight: 600,
  letterSpacing: "0.05em",
  textTransform: "uppercase",
  color: "var(--muted)",
};

const mutedText: CSSProperties = {
  margin: "0.25rem 0 0",
  fontSize: "13px",
  color: "var(--muted)",
};

function ColorRow({
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

export default function ThemeSettings() {
  const {
    mode,
    setMode,
    seeds,
    patchSeed,
    shareString,
    setShareString,
    applyPreset,
    resolvedMode,
    presets,
  } = useTheme();

  const [shareDraft, setShareDraft] = useState(shareString);
  const [prevShareString, setPrevShareString] = useState(shareString);
  const [shareError, setShareError] = useState(false);
  const [copied, setCopied] = useState(false);

  if (shareString !== prevShareString) {
    setPrevShareString(shareString);
    setShareDraft(shareString);
    setShareError(false);
  }

  const matchSystem = mode === "system";

  return (
    <section style={{ maxWidth: "36rem" }}>
      <h2 style={sectionTitle}>Theme</h2>
      <p style={mutedText}>
        Choose colors for light and dark mode. Changes save automatically.
      </p>

      <div
        role="radiogroup"
        aria-label="Color mode"
        style={{
          marginTop: "1rem",
          display: "grid",
          gap: "0.5rem",
          gridTemplateColumns: "repeat(auto-fit, minmax(10rem, 1fr))",
        }}
      >
        {(
          [
            { value: "light" as const, label: "Light" },
            { value: "dark" as const, label: "Dark" },
          ] as const
        ).map((opt) => {
          const active = resolvedMode === opt.value;
          return (
            <button
              key={opt.value}
              type="button"
              role="radio"
              aria-checked={active}
              onClick={() => setMode(opt.value)}
              style={{
                overflow: "hidden",
                borderRadius: "0.5rem",
                border: `1px solid ${active ? "var(--accent)" : "var(--border)"}`,
                textAlign: "left",
                padding: 0,
                cursor: "pointer",
                background: "transparent",
                color: "inherit",
              }}
            >
              <div
                style={{
                  display: "flex",
                  height: "5rem",
                  alignItems: "flex-end",
                  gap: "0.25rem",
                  padding: "0 0.75rem 0.5rem",
                  background:
                    opt.value === "light"
                      ? seeds.lightHeader
                      : seeds.darkHeader,
                }}
              >
                <span
                  style={{
                    height: "1.5rem",
                    flex: 1,
                    borderRadius: "2px",
                    background:
                      opt.value === "light"
                        ? seeds.lightAccent
                        : seeds.darkAccent,
                  }}
                />
                <span
                  style={{
                    height: "1.5rem",
                    width: "2.5rem",
                    borderRadius: "2px",
                    opacity: 0.8,
                    background:
                      opt.value === "light" ? "#ffffff" : "#121416",
                  }}
                />
              </div>
              <div
                style={{
                  display: "flex",
                  alignItems: "center",
                  gap: "0.5rem",
                  background: "var(--panel)",
                  padding: "0.5rem 0.75rem",
                }}
              >
                <span
                  aria-hidden
                  style={{
                    display: "flex",
                    height: "1.25rem",
                    width: "1.25rem",
                    flexShrink: 0,
                    alignItems: "center",
                    justifyContent: "center",
                    borderRadius: "999px",
                    border: `2px solid ${active ? "var(--accent)" : "var(--border)"}`,
                    background: active ? "var(--accent)" : "var(--panel)",
                    color: "var(--sent-text)",
                  }}
                >
                  {active ? (
                    <svg
                      viewBox="0 0 16 16"
                      width="12"
                      height="12"
                      fill="none"
                      stroke="currentColor"
                      strokeWidth="2.25"
                      strokeLinecap="round"
                      strokeLinejoin="round"
                    >
                      <path d="M3.5 8.5 6.5 11.5 12.5 4.5" />
                    </svg>
                  ) : null}
                </span>
                <div
                  style={{
                    fontSize: "14px",
                    fontWeight: 500,
                    color: "var(--text)",
                  }}
                >
                  {opt.label}
                </div>
              </div>
            </button>
          );
        })}
      </div>

      <label
        style={{
          marginTop: "1rem",
          display: "flex",
          cursor: "pointer",
          alignItems: "center",
          gap: "0.625rem",
          fontSize: "14px",
          color: "var(--text)",
        }}
      >
        <input
          type="checkbox"
          className="checkbox-list"
          checked={matchSystem}
          onChange={(e) => {
            if (e.target.checked) setMode("system");
            else setMode(resolvedMode);
          }}
        />
        Match your device theme
      </label>

      <div style={{ marginTop: "1.5rem", display: "flex", flexDirection: "column", gap: "0.75rem" }}>
        <div style={{ ...sectionTitle, fontSize: "12px" }}>Colors</div>
        {SEED_FIELDS.map((field) => (
          <ColorRow
            key={field.key}
            label={field.label}
            value={seeds[field.key]}
            onChange={(hex) => patchSeed(field.key, hex)}
          />
        ))}
      </div>

      <div style={{ marginTop: "1.5rem" }}>
        <div style={sectionTitle}>Share theme</div>
        <p style={mutedText}>Copy or paste the four color codes below.</p>
        <div style={{ marginTop: "0.5rem", display: "flex", gap: "0.5rem" }}>
          <input
            type="text"
            value={shareDraft}
            spellCheck={false}
            onChange={(e) => {
              setShareDraft(e.target.value);
              setShareError(false);
            }}
            onBlur={() => {
              if (shareDraft.trim() === shareString) return;
              const ok = setShareString(shareDraft);
              setShareError(!ok);
              if (!ok) setShareDraft(shareString);
            }}
            aria-invalid={shareError}
            style={{
              minWidth: 0,
              flex: 1,
              borderRadius: "0.375rem",
              border: `1px solid ${shareError ? "var(--danger)" : "var(--border)"}`,
              background: "var(--bg)",
              padding: "0.375rem 0.625rem",
              fontFamily: "ui-monospace, monospace",
              fontSize: "12px",
              color: "var(--text)",
              outline: "none",
            }}
          />
          <button
            type="button"
            onClick={async () => {
              try {
                await navigator.clipboard.writeText(shareString);
                setCopied(true);
                window.setTimeout(() => setCopied(false), 1500);
              } catch {
                /* ignore */
              }
            }}
            style={{
              flexShrink: 0,
              borderRadius: "0.375rem",
              border: "1px solid var(--border)",
              background: "var(--panel)",
              padding: "0.375rem 0.75rem",
              fontSize: "13px",
              color: "var(--text)",
              cursor: "pointer",
            }}
          >
            {copied ? "Copied" : "Copy"}
          </button>
        </div>
        {shareError ? (
          <p
            style={{ margin: "0.25rem 0 0", fontSize: "12px", color: "var(--danger)" }}
            role="alert"
          >
            Enter four valid color codes.
          </p>
        ) : null}
      </div>

      <div style={{ marginTop: "1.5rem" }}>
        <div style={sectionTitle}>Tried and true</div>
        <div
          style={{
            marginTop: "0.75rem",
            display: "flex",
            flexWrap: "wrap",
            gap: "0.75rem",
          }}
        >
          {presets.map((preset) => {
            const active =
              formatCompare(seeds) === formatCompare(preset.seeds);
            return (
              <button
                key={preset.id}
                type="button"
                title={preset.label}
                aria-label={preset.label}
                aria-pressed={active}
                onClick={() => applyPreset(preset)}
                style={{
                  position: "relative",
                  height: "2.5rem",
                  width: "2.5rem",
                  borderRadius: "999px",
                  border: `2px solid ${active ? "var(--accent)" : "var(--border)"}`,
                  cursor: "pointer",
                  padding: 0,
                  background: `conic-gradient(
                    ${preset.seeds.lightHeader} 0deg 90deg,
                    ${preset.seeds.lightAccent} 90deg 180deg,
                    ${preset.seeds.darkHeader} 180deg 270deg,
                    ${preset.seeds.darkAccent} 270deg 360deg
                  )`,
                }}
              />
            );
          })}
        </div>
      </div>
    </section>
  );
}

function formatCompare(seeds: ThemeSeeds): string {
  return [
    seeds.lightHeader,
    seeds.lightAccent,
    seeds.darkHeader,
    seeds.darkAccent,
  ]
    .map((h) => h.toLowerCase())
    .join(",");
}
