"use client";

import { useCallback, useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
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
  const panelRef = useRef<HTMLDivElement>(null);
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
