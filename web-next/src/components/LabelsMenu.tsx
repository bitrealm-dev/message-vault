"use client";

import { isReservedLabelName } from "@/lib/reservedLabels";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { IconHoverTarget } from "./IconHoverLabel";
import {
  ChevronDownIcon,
  EraserIcon,
  PeopleGroupIcon,
  SearchIcon,
} from "./icons";
import { useDismissible } from "./useDismissible";

export type LabelCheckState = "on" | "off" | "mixed";

export function LabelsMenu({
  allLabels,
  checks,
  onToggle,
  onCreate,
  onClearAll,
  onOpenChange,
  onModeChange,
  disabled = false,
  /** Render the trigger as a compact icon-only button with a hover tooltip. */
  iconOnly = false,
  /** Show "Labels" text + chevron (for contact edit dialog). */
  labeled = false,
  /** When set, render only the panel at this fixed position (no toolbar trigger). */
  fixedPosition = null,
}: {
  allLabels: string[];
  /** Per-label membership across the current contact or selection. */
  checks: Record<string, LabelCheckState>;
  onToggle?: (name: string) => void;
  /** Called when a new label is created; should add it to the current target(s). */
  onCreate?: (name: string) => void;
  /** Remove all label memberships from the current target(s). */
  onClearAll?: () => void;
  onOpenChange?: (open: boolean) => void;
  /** Fired when switching between the label list and the create form. */
  onModeChange?: (mode: "list" | "create") => void;
  disabled?: boolean;
  iconOnly?: boolean;
  labeled?: boolean;
  fixedPosition?: { x: number; y: number } | null;
}) {
  const [open, setOpen] = useState(fixedPosition != null);
  const [mode, setMode] = useState<"list" | "create">("list");
  const [query, setQuery] = useState("");
  const [newName, setNewName] = useState("");
  const [localLabels, setLocalLabels] = useState<string[]>(allLabels);
  /** Viewport-fixed panel position anchored to the trigger (avoids pane clipping). */
  const [menuPos, setMenuPos] = useState<{
    top: number;
    left?: number;
    right?: number;
  } | null>(null);
  const triggerRef = useRef<HTMLButtonElement>(null);
  const rootRef = useRef<HTMLDivElement>(null);
  const searchRef = useRef<HTMLInputElement>(null);
  const nameRef = useRef<HTMLInputElement>(null);
  const checkRefs = useRef<Map<string, HTMLInputElement>>(new Map());
  const onOpenChangeRef = useRef(onOpenChange);
  onOpenChangeRef.current = onOpenChange;
  const onModeChangeRef = useRef(onModeChange);
  onModeChangeRef.current = onModeChange;
  const isFixed = fixedPosition != null;

  const setMenuMode = useCallback((next: "list" | "create") => {
    setMode(next);
    // Notify parent synchronously so hover-dismiss can pin before the list
    // unmounts (unmount would otherwise fire mouseleave and close the flyout).
    onModeChangeRef.current?.(next);
  }, []);

  const closeMenu = useCallback(() => {
    setOpen(false);
    setMenuMode("list");
    setMenuPos(null);
    onOpenChangeRef.current?.(false);
  }, [setMenuMode]);

  const openMenu = () => {
    const rect = triggerRef.current?.getBoundingClientRect();
    if (rect) {
      if (labeled) {
        // Open to the right of the trigger so membership names under the
        // button stay visible while the user toggles groups.
        const panelW = 256;
        const left = Math.min(
          rect.right + 4,
          window.innerWidth - panelW - 8,
        );
        setMenuPos({ top: rect.top, left: Math.max(8, left) });
      } else if (iconOnly) {
        setMenuPos({
          top: rect.bottom + 4,
          right: window.innerWidth - rect.right,
        });
      } else {
        setMenuPos({ top: rect.bottom + 4, left: rect.left });
      }
    }
    setOpen(true);
    setMenuMode("list");
    onOpenChangeRef.current?.(true);
  };

  useEffect(() => {
    if (!isFixed) return;
    setOpen(true);
    setMenuMode("list");
    onOpenChangeRef.current?.(true);
  }, [isFixed, setMenuMode]);

  useEffect(() => {
    setLocalLabels((prev) => {
      const merged = new Set([...allLabels, ...prev]);
      return [...merged].sort((a, b) =>
        a.localeCompare(b, undefined, { sensitivity: "base" }),
      );
    });
  }, [allLabels]);

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (!q) return localLabels;
    return localLabels.filter((g) => g.toLowerCase().includes(q));
  }, [localLabels, query]);

  useDismissible({
    open: open && !isFixed,
    onDismiss: closeMenu,
    refs: [rootRef],
    escape: "capture",
    onEscape: (e) => {
      e.preventDefault();
      e.stopImmediatePropagation();
      if (mode === "create") {
        setMenuMode("list");
        setNewName("");
        return false;
      }
    },
  });

  // Fixed flyout: Escape returns to the list before the parent closes the panel.
  useEffect(() => {
    if (!isFixed || !open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
      if (mode !== "create") return;
      e.preventDefault();
      e.stopPropagation();
      setMenuMode("list");
      setNewName("");
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [isFixed, open, mode, setMenuMode]);

  useEffect(() => {
    if (!open) return;
    if (mode === "list") {
      setQuery("");
      requestAnimationFrame(() => searchRef.current?.focus());
    } else {
      setNewName("");
      requestAnimationFrame(() => nameRef.current?.focus());
    }
  }, [open, mode]);

  useEffect(() => {
    for (const name of localLabels) {
      const el = checkRefs.current.get(name);
      if (!el) continue;
      el.indeterminate = checks[name] === "mixed";
    }
  }, [checks, localLabels, open, filtered]);

  const toggle = (name: string) => {
    if (disabled || !onToggle) return;
    onToggle(name);
  };

  const saveNewGroup = () => {
    if (disabled || !onCreate) return;
    const name = newName.trim();
    if (!name) return;
    if (isReservedLabelName(name)) return;

    const existing = localLabels.find(
      (g) => g.toLowerCase() === name.toLowerCase(),
    );
    const resolved = existing ?? name;

    if (!existing) {
      setLocalLabels((prev) =>
        [...prev, resolved].sort((a, b) =>
          a.localeCompare(b, undefined, { sensitivity: "base" }),
        ),
      );
    }

    onCreate(resolved);
    setNewName("");
    setMenuMode("list");
  };

  const hasAnyMembership = Object.values(checks).some(
    (state) => state === "on" || state === "mixed",
  );

  const clearAll = () => {
    if (disabled || !onClearAll || !hasAnyMembership) return;
    onClearAll();
  };

  const panelClass = isFixed
    ? "w-64 rounded-xl border border-border bg-popover shadow-xl"
    : "fixed z-[220] w-64 rounded-xl border border-border bg-popover shadow-xl";

  const panelStyle =
    !isFixed && menuPos
      ? { top: menuPos.top, left: menuPos.left, right: menuPos.right }
      : undefined;

  return (
    <div
      ref={rootRef}
      className={
        isFixed
          ? "fixed z-50"
          : labeled
            ? "relative flex w-full shrink-0 items-center"
            : "relative inline-flex shrink-0 items-center"
      }
      style={
        isFixed
          ? { left: fixedPosition!.x, top: fixedPosition!.y }
          : undefined
      }
    >
      {!isFixed &&
        (iconOnly ? (
          <IconHoverTarget label="Labels" placement="bottom" hidden={open}>
            <button
              ref={triggerRef}
              type="button"
              aria-label="Labels"
              aria-expanded={open}
              disabled={disabled}
              onClick={() => {
                if (disabled) return;
                if (open) closeMenu();
                else openMenu();
              }}
              className={`flex h-7 w-7 items-center justify-center rounded-md border border-border transition-colors disabled:opacity-40 ${
                open
                  ? "bg-accent/20 text-accent"
                  : "bg-elevated text-muted hover:text-text"
              }`}
            >
              <PeopleGroupIcon className="size-5 shrink-0" />
            </button>
          </IconHoverTarget>
        ) : labeled ? (
          <button
            ref={triggerRef}
            type="button"
            aria-label="Labels"
            aria-expanded={open}
            disabled={disabled}
            onClick={() => {
              if (disabled) return;
              if (open) closeMenu();
              else openMenu();
            }}
            className={`inline-flex h-8 w-full items-center justify-between gap-2 rounded-md px-3 text-[13px] leading-none transition-colors disabled:opacity-40 ${
              open
                ? "bg-accent/20 text-accent"
                : "bg-elevated text-muted hover:text-text"
            }`}
          >
            <span>Labels</span>
            <ChevronDownIcon className="size-3.5 shrink-0 opacity-70" />
          </button>
        ) : (
          <IconHoverTarget label="Labels" placement="bottom" hidden={open}>
            <button
              ref={triggerRef}
              type="button"
              aria-label="Labels"
              aria-expanded={open}
              disabled={disabled}
              onClick={() => {
                if (disabled) return;
                if (open) closeMenu();
                else openMenu();
              }}
              className={`inline-flex h-7 items-center gap-1.5 rounded-md px-2.5 text-[12px] leading-none transition-colors disabled:opacity-40 ${
                open
                  ? "bg-accent/20 text-accent"
                  : "bg-elevated text-muted hover:text-text"
              }`}
            >
              <PeopleGroupIcon className="size-4 shrink-0" />
              <ChevronDownIcon className="size-3.5 shrink-0 opacity-70" />
            </button>
          </IconHoverTarget>
        ))}

      {open && mode === "list" && (
        <div className={panelClass} style={panelStyle}>
          <div className="border-b border-border/80 p-2">
            <div className="flex items-center gap-2 rounded-md border border-accent bg-elevated px-2 py-1.5">
              <SearchIcon className="size-5 shrink-0 text-muted" />
              <input
                ref={searchRef}
                type="search"
                value={query}
                onChange={(e) => setQuery(e.target.value)}
                placeholder="Search labels…"
                className="min-w-0 flex-1 bg-transparent text-[13px] text-text outline-none placeholder:text-muted"
              />
            </div>
          </div>

          <div className="max-h-56 overflow-y-auto py-1">
            {filtered.length === 0 ? (
              <p className="px-3 py-2 text-[12px] text-muted">No labels</p>
            ) : (
              filtered.map((name) => {
                const state = checks[name] ?? "off";
                return (
                  <label
                    key={name}
                    className="flex cursor-pointer items-center gap-2.5 px-3 py-1.5 text-[13px] text-text hover:bg-hover-strong"
                  >
                    <input
                      ref={(el) => {
                        if (el) checkRefs.current.set(name, el);
                        else checkRefs.current.delete(name);
                      }}
                      type="checkbox"
                      checked={state === "on"}
                      disabled={disabled}
                      onChange={() => toggle(name)}
                      className="size-3.5 rounded border-border accent-accent"
                    />
                    <span className="truncate">{name}</span>
                  </label>
                );
              })
            )}
          </div>

          <div className="border-t border-border/80 py-1">
            <button
              type="button"
              disabled={disabled}
              onClick={() => setMenuMode("create")}
              className="flex w-full items-center gap-2.5 px-3 py-1.5 text-left text-[13px] text-text hover:bg-hover-strong disabled:opacity-50"
            >
              <span className="flex size-3.5 items-center justify-center text-[15px] leading-none text-muted">
                +
              </span>
              <span>Create label</span>
            </button>
            {onClearAll && (
              <button
                type="button"
                disabled={disabled || !hasAnyMembership}
                onClick={clearAll}
                className="flex w-full items-center gap-2.5 px-3 py-1.5 text-left text-[13px] text-text hover:bg-hover-strong disabled:opacity-50"
              >
                <EraserIcon className="size-3.5 shrink-0 opacity-80" />
                <span>Clear all</span>
              </button>
            )}
          </div>
        </div>
      )}

      {open && mode === "create" && (
        <div className={`${panelClass} p-3`} style={panelStyle}>
          <h3 className="text-[14px] font-semibold text-text">Create label</h3>
          <input
            ref={nameRef}
            type="text"
            value={newName}
            onChange={(e) => setNewName(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") {
                e.preventDefault();
                saveNewGroup();
              }
            }}
            placeholder="Name"
            disabled={disabled}
            className="mt-2.5 w-full rounded-md border border-accent bg-elevated px-2.5 py-1.5 text-[13px] text-text outline-none placeholder:text-muted"
          />
          <div className="mt-3 flex items-center gap-2">
            <button
              type="button"
              disabled={disabled || !newName.trim()}
              onClick={saveNewGroup}
              className="rounded-md bg-accent px-3 py-1 text-[13px] font-medium text-[#1c1c1e] transition-opacity disabled:opacity-40"
            >
              Save
            </button>
            <button
              type="button"
              onClick={() => {
                setMenuMode("list");
                setNewName("");
              }}
              className="rounded-md bg-elevated px-3 py-1 text-[13px] text-text hover:bg-hover"
            >
              Cancel
            </button>
          </div>
        </div>
      )}
    </div>
  );
}

