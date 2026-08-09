"use client";

import { formatSourceLabel } from "@/lib/sourceLabels";

export function MessageSourcePicker({
  sources,
  messageSources,
  sourceCounts,
  source,
  onSourceChange,
}: {
  sources: string[];
  messageSources: string[];
  sourceCounts: { all: number; bySource: Record<string, number> };
  source: string | null;
  onSourceChange: (id: string | null) => void;
}) {
  if (sources.length === 0) return null;

  const options = [
    {
      id: null as string | null,
      label: "Combined",
      enabled: true,
      count: sourceCounts.all,
    },
    ...sources.map((id) => ({
      id,
      label: formatSourceLabel(id),
      enabled: messageSources.includes(id),
      count: sourceCounts.bySource[id] ?? 0,
    })),
  ];

  return (
    <div className="mb-5">
      <h3 className="text-[15px] font-semibold text-text">
        Message Sources
      </h3>
      <div className="mt-2 flex flex-wrap items-start gap-x-0 gap-y-2">
        {options.map((opt, i) => {
          const active = opt.id === null ? source === null : source === opt.id;
          const disabled = !opt.enabled;
          const countLabel = opt.count.toLocaleString();
          return (
            <span key={opt.id ?? "all"} className="flex items-start">
              {i > 0 && (
                <span
                  className="mx-2 pt-0.5 text-[13px] text-muted/50"
                  aria-hidden
                >
                  |
                </span>
              )}
              <button
                type="button"
                disabled={disabled}
                aria-disabled={disabled}
                title={`${opt.label}: ${countLabel} messages`}
                onClick={() => {
                  if (disabled) return;
                  onSourceChange(opt.id);
                }}
                className={`group flex min-w-0 flex-col items-start text-left ${
                  disabled ? "cursor-default" : ""
                }`}
              >
                <span
                  className={`text-[13px] font-medium leading-tight ${
                    disabled
                      ? "text-muted/40"
                      : active
                        ? "text-accent"
                        : "text-text group-hover:text-accent"
                  }`}
                >
                  {opt.label}
                </span>
                <span
                  className={`mt-0.5 inline-block w-[6ch] text-[11px] leading-tight tabular-nums ${
                    disabled ? "text-muted/30" : "text-muted"
                  }`}
                >
                  {countLabel}
                </span>
              </button>
            </span>
          );
        })}
      </div>
    </div>
  );
}
