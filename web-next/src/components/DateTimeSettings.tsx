"use client";

import {
  DATE_SYNTAX_ROWS,
  TIME_SYNTAX_ROWS,
  formatDateOnly,
  strftime,
  parseInstant,
  validateDatePattern,
  validateTimePattern,
  type DateFormatMode,
  type TimeFormatMode,
} from "@/lib/dateTimeFormat";
import { useEffect, useMemo, useRef, useState } from "react";
import { useDateTimeFormat } from "./useDateTimeFormat";

const SAMPLE_TS = "2024-07-17T14:05:09";

type ActiveSyntax = "date" | "time" | null;

function ModeButton({
  selected,
  onClick,
  children,
}: {
  selected: boolean;
  onClick: () => void;
  children: React.ReactNode;
}) {
  return (
    <button
      type="button"
      role="radio"
      aria-checked={selected}
      onClick={onClick}
      className={`rounded-md border px-3 py-1.5 text-[13px] transition-colors ${
        selected
          ? "border-accent bg-accent/15 text-text"
          : "border-border bg-panel text-muted hover:bg-hover hover:text-text"
      }`}
    >
      {children}
    </button>
  );
}

/** Tall enough for the longer Dates list; Times uses the same height. */
const SYNTAX_CARD_MIN_H = "min-h-[28rem]";

function SyntaxCard({
  title,
  rows,
}: {
  title: string;
  rows: { token: string; desc: string }[];
}) {
  return (
    <div
      className={`rounded-lg border border-border bg-elevated p-3 ${SYNTAX_CARD_MIN_H}`}
      // Keep custom input focused when clicking help (avoids blur dismiss).
      onMouseDown={(e) => e.preventDefault()}
    >
      <div className="mb-2 text-[12px] font-semibold tracking-wide text-text/70 uppercase">
        {title}
      </div>
      <div className="text-[12px]">
        {rows.map((row, i) => (
          <div
            key={row.token}
            className={`flex gap-2 px-1.5 py-1 ${
              i % 2 === 0 ? "bg-hover/40" : ""
            }`}
          >
            <code className="w-8 shrink-0 font-mono text-accent">{row.token}</code>
            <span className="text-text/85">{row.desc}</span>
          </div>
        ))}
      </div>
    </div>
  );
}

export function DateTimeSettings() {
  const {
    dateMode,
    setDateMode,
    dateCustom,
    setDateCustom,
    timeMode,
    setTimeMode,
    timeCustom,
    setTimeCustom,
    datePattern,
    timePattern,
  } = useDateTimeFormat();

  const [dateDraft, setDateDraft] = useState(dateCustom);
  const [timeDraft, setTimeDraft] = useState(timeCustom);
  const [dateError, setDateError] = useState<string | null>(null);
  const [timeError, setTimeError] = useState<string | null>(null);
  const [activeSyntax, setActiveSyntax] = useState<ActiveSyntax>(null);

  const dateInputRef = useRef<HTMLInputElement>(null);
  const timeInputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    setDateDraft(dateCustom);
    setDateError(null);
  }, [dateCustom]);

  useEffect(() => {
    setTimeDraft(timeCustom);
    setTimeError(null);
  }, [timeCustom]);

  const previewDatePattern = useMemo(() => {
    if (dateMode !== "custom") return datePattern;
    const v = validateDatePattern(dateDraft);
    return v.ok ? dateDraft : datePattern;
  }, [dateMode, dateDraft, datePattern]);

  const previewTimePattern = useMemo(() => {
    if (timeMode !== "custom") return timePattern;
    const v = validateTimePattern(timeDraft);
    return v.ok ? timeDraft : timePattern;
  }, [timeMode, timeDraft, timePattern]);

  const sample = parseInstant(SAMPLE_TS)!;
  const datePreview = formatDateOnly(SAMPLE_TS, previewDatePattern);
  const timePreview = strftime(sample, previewTimePattern);

  const commitDateDraft = () => {
    const v = validateDatePattern(dateDraft);
    if (!v.ok) {
      setDateError(v.error);
      return;
    }
    setDateError(null);
    setDateCustom(dateDraft);
  };

  const commitTimeDraft = () => {
    const v = validateTimePattern(timeDraft);
    if (!v.ok) {
      setTimeError(v.error);
      return;
    }
    setTimeError(null);
    setTimeCustom(timeDraft);
  };

  const selectDateMode = (mode: DateFormatMode) => {
    setDateMode(mode);
    if (mode === "custom") {
      setActiveSyntax("date");
      // After the custom input mounts / paints.
      window.setTimeout(() => dateInputRef.current?.focus(), 0);
    } else {
      setActiveSyntax((cur) => (cur === "date" ? null : cur));
    }
  };

  const selectTimeMode = (mode: TimeFormatMode) => {
    setTimeMode(mode);
    if (mode === "custom") {
      setActiveSyntax("time");
      window.setTimeout(() => timeInputRef.current?.focus(), 0);
    } else {
      setActiveSyntax((cur) => (cur === "time" ? null : cur));
    }
  };

  const showSyntax = activeSyntax != null;

  return (
    <section>
      <div
        className={
          showSyntax
            ? "grid gap-6 lg:grid-cols-[minmax(0,1fr)_minmax(16rem,20rem)]"
            : "max-w-xl"
        }
      >
        <div className="min-w-0 max-w-xl">
          <h2 className="text-[12px] font-semibold tracking-wider text-muted uppercase">
            Date and time
          </h2>
          <p className="mt-1 text-[13px] text-muted">
            Choose how dates and times appear. Changes save automatically.
          </p>
          <h3 className="mt-5 text-[13px] font-medium text-text">Date</h3>

          <div
            role="radiogroup"
            aria-label="Date format"
            className="mt-3 flex flex-wrap gap-2"
          >
            <ModeButton
              selected={dateMode === "ymd"}
              onClick={() => selectDateMode("ymd")}
            >
              YYYY-MM-DD
            </ModeButton>
            <ModeButton
              selected={dateMode === "custom"}
              onClick={() => selectDateMode("custom")}
            >
              Custom
            </ModeButton>
          </div>

          {dateMode === "custom" ? (
            <div className="mt-3">
              <input
                ref={dateInputRef}
                type="text"
                value={dateDraft}
                spellCheck={false}
                onFocus={() => setActiveSyntax("date")}
                onChange={(e) => {
                  setDateDraft(e.target.value);
                  setDateError(null);
                }}
                onBlur={() => {
                  commitDateDraft();
                  setActiveSyntax((cur) => (cur === "date" ? null : cur));
                }}
                onKeyDown={(e) => {
                  if (e.key === "Enter") (e.target as HTMLInputElement).blur();
                }}
                className={`w-full rounded-md border bg-bg px-2.5 py-1.5 font-mono text-[13px] text-text outline-none focus:border-accent ${
                  dateError ? "border-danger" : "border-border"
                }`}
                aria-invalid={Boolean(dateError)}
                placeholder="%Y-%m-%d"
              />
              {dateError ? (
                <p className="mt-1 text-[12px] text-danger">{dateError}</p>
              ) : null}
            </div>
          ) : null}

          <p className="mt-2 font-mono text-[12px] text-muted">
            Preview: <span className="text-text">{datePreview}</span>
          </p>

          <h3 className="mt-8 text-[13px] font-medium text-text">Time</h3>

          <div
            role="radiogroup"
            aria-label="Time format"
            className="mt-3 flex flex-wrap gap-2"
          >
            {(
              [
                { value: "24h" as TimeFormatMode, label: "24-hour" },
                { value: "12h" as TimeFormatMode, label: "AM/PM" },
                { value: "custom" as TimeFormatMode, label: "Custom" },
              ] as const
            ).map((opt) => (
              <ModeButton
                key={opt.value}
                selected={timeMode === opt.value}
                onClick={() => selectTimeMode(opt.value)}
              >
                {opt.label}
              </ModeButton>
            ))}
          </div>

          {timeMode === "custom" ? (
            <div className="mt-3">
              <input
                ref={timeInputRef}
                type="text"
                value={timeDraft}
                spellCheck={false}
                onFocus={() => setActiveSyntax("time")}
                onChange={(e) => {
                  setTimeDraft(e.target.value);
                  setTimeError(null);
                }}
                onBlur={() => {
                  commitTimeDraft();
                  setActiveSyntax((cur) => (cur === "time" ? null : cur));
                }}
                onKeyDown={(e) => {
                  if (e.key === "Enter") (e.target as HTMLInputElement).blur();
                }}
                className={`w-full rounded-md border bg-bg px-2.5 py-1.5 font-mono text-[13px] text-text outline-none focus:border-accent ${
                  timeError ? "border-danger" : "border-border"
                }`}
                aria-invalid={Boolean(timeError)}
                placeholder="%H:%M:%S"
              />
              {timeError ? (
                <p className="mt-1 text-[12px] text-danger">{timeError}</p>
              ) : null}
            </div>
          ) : null}

          <p className="mt-2 font-mono text-[12px] text-muted">
            Preview: <span className="text-text">{timePreview}</span>
          </p>
        </div>

        {showSyntax ? (
          <div className="lg:sticky lg:top-4 lg:self-start">
            {activeSyntax === "date" ? (
              <SyntaxCard title="Dates" rows={DATE_SYNTAX_ROWS} />
            ) : null}
            {activeSyntax === "time" ? (
              <SyntaxCard title="Times" rows={TIME_SYNTAX_ROWS} />
            ) : null}
          </div>
        ) : null}
      </div>
    </section>
  );
}
