"use client";

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { AdvancedSearchForm } from "./AdvancedSearchForm";
import { ChevronDownIcon, SearchIcon } from "./icons";

type Suggestion = {
  kind: "contact" | "label";
  value: string;
};

const OPERATOR_VALUE_RE =
  /(?:^|\s)((?:with|from|to|within|label|in|handle):)(?:"([^"]*)"|([^\s]*))$/i;

function quoteIfNeeded(value: string): string {
  return /\s/.test(value) ? `"${value.replace(/"/g, "")}"` : value;
}

function suggestionsForQuery(
  query: string,
  contacts: string[],
  labels: string[],
): { suggestions: Suggestion[]; replaceFrom: number } | null {
  const m = query.match(OPERATOR_VALUE_RE);
  if (!m || m.index == null) return null;
  const opToken = m[1]!;
  const partial = (m[2] ?? m[3] ?? "").toLowerCase();
  const op = opToken.slice(0, -1).toLowerCase();
  const pool =
    op === "within" || op === "label"
      ? labels.map((value) => ({ kind: "label" as const, value }))
      : contacts.map((value) => ({ kind: "contact" as const, value }));
  const suggestions = pool
    .filter((s) => s.value.trim())
    .filter(
      (s) => !partial || s.value.toLowerCase().includes(partial),
    )
    .filter(
      (s, i, arr) =>
        arr.findIndex(
          (o) => o.value.toLowerCase() === s.value.toLowerCase(),
        ) === i,
    )
    .slice(0, 8);
  if (suggestions.length === 0) return null;
  return { suggestions, replaceFrom: m.index + m[0].search(opToken) };
}

/** Vault-wide search field with optional advanced form dropdown. */
export function VaultSearchField({
  value,
  onChange,
  onSubmit,
  sources,
  labels,
  contacts = [],
}: {
  value: string;
  onChange: (value: string) => void;
  onSubmit: (value: string) => void;
  sources: string[];
  labels: string[];
  /** Contact display names for `with:` / `from:` / `to:` / `handle:` autocomplete. */
  contacts?: string[];
}) {
  const [advancedOpen, setAdvancedOpen] = useState(false);
  const [suggestIndex, setSuggestIndex] = useState(0);
  const wrapRef = useRef<HTMLDivElement>(null);
  const panelRef = useRef<HTMLDivElement>(null);
  const suggestRef = useRef<HTMLDivElement>(null);
  const [panelPosition, setPanelPosition] = useState<{
    left: number;
    top: number;
    width: number;
    maxHeight: number;
  } | null>(null);
  const positionPanel = useCallback(() => {
    const rect = wrapRef.current?.getBoundingClientRect();
    if (!rect) return;
    const gutter = 8;
    const width = Math.min(520, window.innerWidth - gutter * 2);
    setPanelPosition({
      left: Math.max(
        gutter,
        Math.min(rect.left, window.innerWidth - width - gutter),
      ),
      top: rect.bottom + 4,
      width,
      maxHeight: Math.max(180, window.innerHeight - rect.bottom - 12),
    });
  }, []);

  const suggestState = useMemo(
    () =>
      advancedOpen ? null : suggestionsForQuery(value, contacts, labels),
    [advancedOpen, value, contacts, labels],
  );
  const suggestions = suggestState?.suggestions ?? [];

  useEffect(() => {
    setSuggestIndex(0);
  }, [value, suggestions.length]);

  const applySuggestion = useCallback(
    (suggestion: Suggestion) => {
      if (!suggestState) return;
      const prefix = value.slice(0, suggestState.replaceFrom);
      const opMatch = value
        .slice(suggestState.replaceFrom)
        .match(/^(with|from|to|within|label|in|handle):/i);
      const op = opMatch?.[0] ?? "";
      const next = `${prefix}${op}${quoteIfNeeded(suggestion.value)} `;
      onChange(next);
    },
    [onChange, suggestState, value],
  );

  useEffect(() => {
    if (!advancedOpen) return;
    positionPanel();
    const onDoc = (e: MouseEvent) => {
      const target = e.target as Node;
      if (
        !wrapRef.current?.contains(target) &&
        !panelRef.current?.contains(target)
      ) {
        setAdvancedOpen(false);
      }
    };
    const onViewportChange = () => positionPanel();
    document.addEventListener("mousedown", onDoc);
    window.addEventListener("resize", onViewportChange);
    window.addEventListener("scroll", onViewportChange, true);
    return () => {
      document.removeEventListener("mousedown", onDoc);
      window.removeEventListener("resize", onViewportChange);
      window.removeEventListener("scroll", onViewportChange, true);
    };
  }, [advancedOpen, positionPanel]);

  useEffect(() => {
    if (suggestions.length === 0) return;
    const onDoc = (e: MouseEvent) => {
      const target = e.target as Node;
      if (
        !wrapRef.current?.contains(target) &&
        !suggestRef.current?.contains(target)
      ) {
        // Suggestions close naturally when the operator token changes.
      }
    };
    document.addEventListener("mousedown", onDoc);
    return () => document.removeEventListener("mousedown", onDoc);
  }, [suggestions.length]);

  return (
    <div ref={wrapRef} className="relative w-full">
      <div className="flex w-full items-center gap-1">
        <div className="relative flex min-w-0 flex-1 items-center gap-2 rounded-md border border-border bg-elevated px-2.5 py-1.5 focus-within:border-accent">
          <SearchIcon className="size-4 shrink-0 text-muted" />
          <input
            type="search"
            value={value}
            aria-label="Search vault"
            aria-autocomplete="list"
            aria-expanded={suggestions.length > 0}
            placeholder="Search vault"
            onChange={(e) => onChange(e.target.value)}
            onKeyDown={(e) => {
              if (suggestions.length > 0) {
                if (e.key === "ArrowDown") {
                  e.preventDefault();
                  setSuggestIndex((i) => (i + 1) % suggestions.length);
                  return;
                }
                if (e.key === "ArrowUp") {
                  e.preventDefault();
                  setSuggestIndex(
                    (i) => (i - 1 + suggestions.length) % suggestions.length,
                  );
                  return;
                }
                if (e.key === "Tab" && !e.shiftKey) {
                  e.preventDefault();
                  applySuggestion(suggestions[suggestIndex]!);
                  return;
                }
              }
              if (e.key === "Enter") {
                e.preventDefault();
                onSubmit(value);
                setAdvancedOpen(false);
              }
            }}
            className="min-w-0 flex-1 bg-transparent text-[13px] text-text outline-none placeholder:text-muted"
          />
          {suggestions.length > 0 ? (
            <div
              ref={suggestRef}
              role="listbox"
              aria-label="Search suggestions"
              className="absolute top-full right-0 left-0 z-30 mt-1 max-h-56 overflow-y-auto rounded-md border border-border bg-panel shadow-lg"
            >
              {suggestions.map((s, i) => (
                <button
                  key={`${s.kind}:${s.value}`}
                  type="button"
                  role="option"
                  aria-selected={i === suggestIndex}
                  onMouseDown={(e) => {
                    e.preventDefault();
                    applySuggestion(s);
                  }}
                  className={`flex w-full items-center justify-between gap-2 px-2.5 py-1.5 text-left text-[13px] ${
                    i === suggestIndex
                      ? "bg-accent/20 text-text"
                      : "text-text hover:bg-hover"
                  }`}
                >
                  <span className="min-w-0 truncate">{s.value}</span>
                  <span className="shrink-0 text-[11px] text-muted capitalize">
                    {s.kind}
                  </span>
                </button>
              ))}
            </div>
          ) : null}
        </div>
        <button
          type="button"
          aria-label="Advanced search"
          aria-expanded={advancedOpen}
          onClick={() => setAdvancedOpen((o) => !o)}
          className={`flex h-[34px] w-8 shrink-0 items-center justify-center rounded-md border border-border bg-elevated text-muted transition-colors hover:bg-hover ${
            advancedOpen ? "border-accent text-accent" : ""
          }`}
        >
          <ChevronDownIcon
            className={`size-3.5 transition-transform ${
              advancedOpen ? "rotate-180" : ""
            }`}
          />
        </button>
      </div>
      {advancedOpen && panelPosition && typeof document !== "undefined"
        ? createPortal(
            <div
              ref={panelRef}
              className="fixed z-[200] overflow-y-auto rounded-md border border-border bg-panel shadow-lg"
              style={panelPosition}
            >
              <AdvancedSearchForm
                sources={sources}
                labels={labels}
                initialQuery={value}
                onCancel={() => setAdvancedOpen(false)}
                onSearch={(q) => {
                  onChange(q);
                  onSubmit(q);
                  setAdvancedOpen(false);
                }}
              />
            </div>,
            document.body,
          )
        : null}
    </div>
  );
}
