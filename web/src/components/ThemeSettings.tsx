"use client";

import { normalizeHex, type ThemeSeeds } from "@/lib/theme";
import { useState } from "react";
import { useTheme } from "./useTheme";

const SEED_FIELDS: {
  key: keyof ThemeSeeds;
  label: string;
}[] = [
  { key: "lightHeader", label: "Light header" },
  { key: "lightAccent", label: "Light accent" },
  { key: "darkHeader", label: "Dark header" },
  { key: "darkAccent", label: "Dark accent" },
];

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
    <div className="flex items-center gap-3">
      <label className="w-28 shrink-0 text-[13px] text-muted">{label}</label>
      <input
        type="color"
        value={normalizeHex(value) ?? "#000000"}
        onChange={(e) => onChange(e.target.value)}
        className="h-9 w-12 cursor-pointer rounded border border-border bg-panel p-0.5"
        aria-label={label}
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
        className="min-w-0 flex-1 rounded-md border border-border bg-bg px-2.5 py-1.5 font-mono text-[13px] text-text outline-none focus:border-accent"
      />
    </div>
  );
}

export function ThemeSettings() {
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
    <section className="max-w-xl">
      <h2 className="text-[12px] font-semibold tracking-wider text-muted uppercase">
        Theme
      </h2>
      <p className="mt-1 text-[13px] text-muted">
        Four colors define your theme. Surfaces and bubbles are derived from
        the active header and accent. Changes save automatically.
      </p>

      <div
        role="radiogroup"
        aria-label="Color mode"
        className="mt-4 grid gap-2 sm:grid-cols-2"
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
              className={`overflow-hidden rounded-lg border text-left transition-colors ${
                active
                  ? "border-accent"
                  : "border-border hover:border-muted"
              }`}
            >
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
                  className="h-6 flex-1 rounded-sm"
                  style={{
                    background:
                      opt.value === "light"
                        ? seeds.lightAccent
                        : seeds.darkAccent,
                  }}
                />
                <span
                  className="h-6 w-10 rounded-sm opacity-80"
                  style={{
                    background:
                      opt.value === "light" ? "#ffffff" : "#121416",
                  }}
                />
              </div>
              <div className="flex items-center gap-2 bg-panel px-3 py-2">
                <span
                  className={`flex h-5 w-5 shrink-0 items-center justify-center rounded-full border-2 ${
                    active
                      ? "border-accent bg-accent text-sent-text"
                      : "border-border bg-panel"
                  }`}
                  aria-hidden
                >
                  {active ? (
                    <svg
                      viewBox="0 0 16 16"
                      className="h-3 w-3"
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
                <div className="text-[14px] font-medium text-text">
                  {opt.label}
                </div>
              </div>
            </button>
          );
        })}
      </div>

      <label className="mt-4 flex cursor-pointer items-center gap-2.5 text-[14px] text-text">
        <input
          type="checkbox"
          className="checkbox-list"
          checked={matchSystem}
          onChange={(e) => {
            if (e.target.checked) setMode("system");
            else setMode(resolvedMode);
          }}
        />
        Switch light/dark to match system
      </label>

      <div className="mt-6 space-y-3">
        <div className="text-lg font-semibold tracking-wide text-muted uppercase">
          Colors
        </div>
        {SEED_FIELDS.map((field) => (
          <ColorRow
            key={field.key}
            label={field.label}
            value={seeds[field.key]}
            onChange={(hex) => patchSeed(field.key, hex)}
          />
        ))}
      </div>

      <div className="mt-6">
        <div className="text-[12px] font-semibold tracking-wide text-muted uppercase">
          Share theme
        </div>
        <p className="mt-1 text-[13px] text-muted">
          Copy or paste four hex values: light header, light accent, dark
          header, dark accent.
        </p>
        <div className="mt-2 flex gap-2">
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
            className={`min-w-0 flex-1 rounded-md border bg-bg px-2.5 py-1.5 font-mono text-[12px] text-text outline-none focus:border-accent ${
              shareError ? "border-danger" : "border-border"
            }`}
            aria-invalid={shareError}
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
            className="shrink-0 rounded-md border border-border bg-panel px-3 py-1.5 text-[13px] text-text hover:bg-hover"
          >
            {copied ? "Copied" : "Copy"}
          </button>
        </div>
      </div>

      <div className="mt-6">
        <div className="text-[12px] font-semibold tracking-wide text-muted uppercase">
          Tried and true
        </div>
        <div className="mt-3 flex flex-wrap gap-3">
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
                className={`relative h-10 w-10 rounded-full border-2 transition-transform hover:scale-105 ${
                  active ? "border-accent" : "border-border"
                }`}
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
