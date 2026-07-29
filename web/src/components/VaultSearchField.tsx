"use client";

import { useEffect, useRef, useState } from "react";
import { AdvancedSearchForm } from "./AdvancedSearchForm";
import { ChevronDownIcon, SearchIcon } from "./icons";

/** Vault-wide search field with optional advanced form dropdown. */
export function VaultSearchField({
  value,
  onChange,
  onSubmit,
  sources,
  labels,
}: {
  value: string;
  onChange: (value: string) => void;
  onSubmit: (value: string) => void;
  sources: string[];
  labels: string[];
}) {
  const [advancedOpen, setAdvancedOpen] = useState(false);
  const wrapRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!advancedOpen) return;
    const onDoc = (e: MouseEvent) => {
      if (!wrapRef.current?.contains(e.target as Node)) {
        setAdvancedOpen(false);
      }
    };
    document.addEventListener("mousedown", onDoc);
    return () => document.removeEventListener("mousedown", onDoc);
  }, [advancedOpen]);

  return (
    <div ref={wrapRef} className="relative w-full">
      <div className="flex w-full items-center gap-1">
        <div className="flex min-w-0 flex-1 items-center gap-2 rounded-md border border-border bg-elevated px-2.5 py-1.5 focus-within:border-accent">
          <SearchIcon className="size-4 shrink-0 text-muted" />
          <input
            type="search"
            value={value}
            aria-label="Search vault"
            placeholder="Search vault"
            onChange={(e) => onChange(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") {
                e.preventDefault();
                onSubmit(value);
                setAdvancedOpen(false);
              }
            }}
            className="min-w-0 flex-1 bg-transparent text-[13px] text-text outline-none placeholder:text-muted"
          />
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
      {advancedOpen ? (
        <div className="absolute top-full right-0 left-0 z-30 mt-1 overflow-hidden rounded-md border border-border bg-panel shadow-lg">
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
        </div>
      ) : null}
    </div>
  );
}
