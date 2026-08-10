import { useState, type CSSProperties } from "react";
import {
  Checkbox,
  Radio,
  RadioGroup,
  ToggleButton,
} from "react-aria-components";
import { type ThemeMode, type ThemeSeeds } from "../lib/theme";
import { useTheme } from "../lib/ThemeProvider";
import { ColorRow, formatCompare } from "./theme/ThemeColorRow";

const SEED_FIELDS: {
  key: keyof ThemeSeeds;
  label: string;
}[] = [
  { key: "lightHeader", label: "Light header" },
  { key: "lightAccent", label: "Light accent" },
  { key: "darkHeader", label: "Dark header" },
  { key: "darkAccent", label: "Dark accent" },
];

const MODE_OPTIONS = [
  { value: "light" as const, label: "Light" },
  { value: "dark" as const, label: "Dark" },
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

/** Check glyph shared by the mode radio dots and the system-mode checkbox. */
function CheckIcon({ className }: { className?: string }) {
  return (
    <svg
      viewBox="0 0 16 16"
      fill="none"
      stroke="currentColor"
      strokeWidth="2.25"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden
      className={className}
    >
      <path d="M3.5 8.5 6.5 11.5 12.5 4.5" />
    </svg>
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

      <RadioGroup
        // Track the resolved mode so one card is always selected — a group
        // value matching no radio ("system") would drop every radio's
        // tabindex and make the picker unreachable by keyboard.
        value={resolvedMode}
        onChange={(value) => setMode(value as ThemeMode)}
        aria-label="Color mode"
        className="mt-4 grid grid-cols-[repeat(auto-fit,minmax(10rem,1fr))] gap-2"
      >
        {MODE_OPTIONS.map((opt) => (
          <Radio
            key={opt.value}
            value={opt.value}
            className={({ isSelected }) =>
              `cursor-pointer overflow-hidden rounded-lg border text-left outline-none
               data-focus-visible:ring-2 data-focus-visible:ring-accent
               ${isSelected ? "border-accent" : "border-border"}`
            }
          >
            {({ isSelected }) => (
              <>
                <div
                  className="flex h-20 items-end gap-1 px-3 pb-2"
                  style={{
                    background:
                      opt.value === "light"
                        ? seeds.lightHeader
                        : seeds.darkHeader,
                  }}
                >
                  <span
                    className="h-6 flex-1 rounded-[2px]"
                    style={{
                      background:
                        opt.value === "light"
                          ? seeds.lightAccent
                          : seeds.darkAccent,
                    }}
                  />
                  <span className="h-6 w-10 rounded-[2px] bg-text opacity-80" />
                </div>
                <div className="flex items-center gap-2 bg-panel px-3 py-2">
                  <span
                    aria-hidden
                    className={`flex h-5 w-5 shrink-0 items-center justify-center rounded-full border-2 ${
                      isSelected
                        ? "border-accent bg-accent text-sent-text"
                        : "border-border bg-panel"
                    }`}
                  >
                    {isSelected ? <CheckIcon className="h-3 w-3" /> : null}
                  </span>
                  <span className="text-[0.875rem] font-medium text-text">
                    {opt.label}
                  </span>
                </div>
              </>
            )}
          </Radio>
        ))}
      </RadioGroup>

      <Checkbox
        isSelected={matchSystem}
        onChange={(checked) => {
          if (checked) setMode("system");
          else setMode(resolvedMode);
        }}
        className="mt-4 flex cursor-pointer items-center gap-2.5 text-[0.875rem] text-text outline-none data-focus-visible:ring-2 data-focus-visible:ring-accent"
      >
        {({ isSelected }) => (
          <>
            <span
              className={`flex h-4 w-4 shrink-0 items-center justify-center rounded border ${
                isSelected
                  ? "border-accent bg-accent text-[color:var(--checkbox-check)]"
                  : "border-border bg-elevated"
              }`}
            >
              {isSelected ? <CheckIcon className="h-3.5 w-3.5" /> : null}
            </span>
            Match your device theme
          </>
        )}
      </Checkbox>

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
        <div className="mt-3 flex flex-wrap gap-3">
          {presets.map((preset) => {
            const active =
              formatCompare(seeds) === formatCompare(preset.seeds);
            return (
              <ToggleButton
                key={preset.id}
                isSelected={active}
                onChange={() => applyPreset(preset)}
                aria-label={preset.label}
                ref={(el) => {
                  // React Aria's filterDOMProps drops `title`; set it
                  // directly so hover tooltips keep working.
                  if (el && el.title !== preset.label) el.title = preset.label;
                }}
                className={({ isSelected }) =>
                  `relative h-10 w-10 cursor-pointer rounded-full border-2 outline-none
                   focus-visible:ring-2 focus-visible:ring-accent
                   ${isSelected ? "border-accent" : "border-transparent"}`
                }
                style={{
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
