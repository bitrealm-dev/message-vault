"use client";

import { useTheme } from "./useTheme";

const DERIVED_TOKENS = [
  { token: "--bg", utility: "bg-bg", label: "bg" },
  { token: "--panel", utility: "bg-panel", label: "panel" },
  { token: "--sidebar", utility: "bg-sidebar", label: "sidebar" },
  { token: "--elevated", utility: "bg-elevated", label: "elevated" },
  { token: "--popover", utility: "bg-popover", label: "popover" },
  { token: "--border", utility: "bg-border", label: "border" },
  { token: "--text", utility: "bg-text", label: "text" },
  { token: "--muted", utility: "bg-muted", label: "muted" },
  { token: "--received", utility: "bg-received", label: "received" },
  { token: "--accent", utility: "bg-accent", label: "accent / sent" },
] as const;

/** Live seeds + derived surfaces for the active theme. */
export function StyleGuidePreview() {
  const { seeds, resolvedMode, shareString } = useTheme();

  const activeHeader =
    resolvedMode === "dark" ? seeds.darkHeader : seeds.lightHeader;
  const activeAccent =
    resolvedMode === "dark" ? seeds.darkAccent : seeds.lightAccent;

  return (
    <section className="mt-10 max-w-3xl">
      <h3 className="text-[13px] font-semibold tracking-wide text-muted uppercase">
        Style guide
      </h3>
      <p className="mt-1 text-[14px] text-muted">
        Seeds and surfaces for the active mode. Reference:{" "}
        <code className="text-[12px] text-text">web/STYLE_GUIDE.md</code>
      </p>

      <div className="mt-6 rounded-lg border border-border bg-panel p-4">
        <div className="text-[12px] font-semibold tracking-wide text-muted uppercase">
          Seeds ({resolvedMode})
        </div>
        <div className="mt-3 flex flex-wrap gap-3">
          <SeedChip label="header" hex={activeHeader} />
          <SeedChip label="accent" hex={activeAccent} />
        </div>
        <div className="mt-2 font-mono text-[11px] text-muted break-all">
          {shareString}
        </div>

        <div className="mt-6 text-[12px] font-semibold tracking-wide text-muted uppercase">
          Derived
        </div>
        <div className="mt-3 flex flex-wrap gap-3">
          {DERIVED_TOKENS.map((item) => (
            <div key={item.token} className="w-[5.5rem]">
              <div
                className={`h-12 rounded-md border border-border ${item.utility}`}
                title={item.token}
              />
              <div className="mt-1 truncate text-[11px] font-medium text-text">
                {item.label}
              </div>
              <div className="truncate font-mono text-[10px] text-muted">
                {item.token}
              </div>
            </div>
          ))}
        </div>

        <div className="mt-6 flex flex-wrap items-end gap-3">
          <div className="rounded-lg bg-sent px-3 py-2 text-[13px] text-sent-text">
            Sent bubble
          </div>
          <div className="rounded-lg bg-received px-3 py-2 text-[13px] text-received-text">
            Received bubble
          </div>
          <button
            type="button"
            className="rounded-md bg-accent px-3 py-1.5 text-[13px] font-medium text-sent-text"
          >
            Accent action
          </button>
          <span className="text-[13px] text-danger">Danger text</span>
        </div>
      </div>
    </section>
  );
}

function SeedChip({ label, hex }: { label: string; hex: string }) {
  return (
    <div className="w-[5.5rem]">
      <div
        className="h-12 rounded-md border border-border"
        style={{ background: hex }}
        title={hex}
      />
      <div className="mt-1 truncate text-[11px] font-medium text-text">
        {label}
      </div>
      <div className="truncate font-mono text-[10px] text-muted">{hex}</div>
    </div>
  );
}
