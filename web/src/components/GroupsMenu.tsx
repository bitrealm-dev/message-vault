import { useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import {
  isReservedGroupName,
  reservedGroupError,
} from "../lib/contactGroups";
import type { MembershipCheckState } from "../lib/membershipChecks";
import { shouldIgnoreOutsideDismiss } from "../lib/portaledOverlay";
import { popupShadow } from "../lib/uiStyles";
import { PeopleGroupIcon } from "./icons";

export type GroupCheckState = MembershipCheckState;

/** Assign or remove groups (or tags) on the selected rows. */
export default function GroupsMenu({
  allGroups,
  checks,
  onToggle,
  onCreate,
  onClearAll,
  disabled = false,
  ariaLabel = "Groups",
  title = "Groups",
  searchPlaceholder = "Search groups…",
  emptyText = "No groups",
  createButtonLabel = "Create group",
  createTitle = "Create group",
  isReserved = isReservedGroupName,
  reservedError = reservedGroupError,
  icon,
}: {
  allGroups: string[];
  checks: Record<string, GroupCheckState>;
  onToggle?: (name: string) => void;
  onCreate?: (name: string) => void;
  onClearAll?: () => void;
  disabled?: boolean;
  ariaLabel?: string;
  title?: string;
  searchPlaceholder?: string;
  emptyText?: string;
  createButtonLabel?: string;
  createTitle?: string;
  isReserved?: (name: string) => boolean;
  reservedError?: (name: string) => string;
  icon?: ReactNode;
}) {
  const [open, setOpen] = useState(false);
  const [mode, setMode] = useState<"list" | "create">("list");
  const [query, setQuery] = useState("");
  const [newName, setNewName] = useState("");
  const [createError, setCreateError] = useState<string | null>(null);
  const rootRef = useRef<HTMLDivElement>(null);
  const searchRef = useRef<HTMLInputElement>(null);
  const nameRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (!open) return;
    const onPointerDown = (e: MouseEvent) => {
      if (shouldIgnoreOutsideDismiss(e, rootRef.current)) return;
      setOpen(false);
      setMode("list");
    };
    document.addEventListener("mousedown", onPointerDown, true);
    return () => document.removeEventListener("mousedown", onPointerDown, true);
  }, [open]);

  useEffect(() => {
    if (!open) return;
    if (mode === "list") {
      setQuery("");
      requestAnimationFrame(() => searchRef.current?.focus());
    } else {
      setNewName("");
      setCreateError(null);
      requestAnimationFrame(() => nameRef.current?.focus());
    }
  }, [open, mode]);

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (!q) return allGroups;
    return allGroups.filter((g) => g.toLowerCase().includes(q));
  }, [allGroups, query]);

  const hasAnyMembership = Object.values(checks).some(
    (state) => state === "on" || state === "mixed",
  );

  const saveNew = () => {
    if (disabled || !onCreate) return;
    const name = newName.trim();
    if (!name) return;
    if (isReserved(name)) {
      setCreateError(reservedError(name));
      return;
    }
    onCreate(name);
    setNewName("");
    setMode("list");
  };

  return (
    <div ref={rootRef} className="relative">
      <button
        type="button"
        aria-label={ariaLabel}
        aria-expanded={open}
        disabled={disabled}
        title={title}
        onClick={() => {
          if (disabled) return;
          setOpen((v) => !v);
          setMode("list");
        }}
        className={`flex h-7 w-7 cursor-pointer items-center justify-center rounded-md border border-border bg-elevated text-muted hover:text-text disabled:cursor-default disabled:opacity-40 ${
          open ? "text-accent" : ""
        }`}
      >
        {icon ?? <PeopleGroupIcon size={16} />}
      </button>
      {open && mode === "list" ? (
        <div
          data-mv-overlay=""
          className={`absolute top-full right-0 z-[100] mt-1 w-64 rounded-xl border border-border bg-popover ${popupShadow}`}
        >
          <div className="border-b border-border p-2">
            <input
              ref={searchRef}
              type="search"
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              placeholder={searchPlaceholder}
              className="box-border w-full rounded border border-border bg-elevated px-2 py-1.5 text-[0.813rem] text-text outline-none"
            />
          </div>
          <div className="max-h-56 overflow-y-auto py-1">
            {filtered.length === 0 ? (
              <p className="px-3 py-2 text-[0.75rem] text-muted">{emptyText}</p>
            ) : (
              filtered.map((name) => {
                const state = checks[name] ?? "off";
                return (
                  <label
                    key={name}
                    className="flex cursor-pointer items-center gap-2 px-3 py-1.5 text-[0.813rem] text-text hover:bg-hover"
                  >
                    <input
                      type="checkbox"
                      checked={state === "on"}
                      ref={(el) => {
                        if (el) el.indeterminate = state === "mixed";
                      }}
                      disabled={disabled}
                      onChange={() => onToggle?.(name)}
                      className="size-3.5 accent-accent"
                    />
                    <span className="truncate">{name}</span>
                  </label>
                );
              })
            )}
          </div>
          <div className="border-t border-border py-1">
            <button
              type="button"
              disabled={disabled}
              onClick={() => setMode("create")}
              className="flex w-full cursor-pointer items-center gap-2 border-none bg-transparent px-3 py-1.5 text-left text-[0.813rem] text-text hover:bg-hover disabled:opacity-50"
            >
              <span className="text-muted">+</span>
              {createButtonLabel}
            </button>
            {onClearAll ? (
              <button
                type="button"
                disabled={disabled || !hasAnyMembership}
                onClick={() => onClearAll()}
                className="flex w-full cursor-pointer items-center gap-2 border-none bg-transparent px-3 py-1.5 text-left text-[0.813rem] text-text hover:bg-hover disabled:opacity-50"
              >
                Clear all
              </button>
            ) : null}
          </div>
        </div>
      ) : null}
      {open && mode === "create" ? (
        <div
          data-mv-overlay=""
          className={`absolute top-full right-0 z-[100] mt-1 w-64 rounded-xl border border-border bg-popover p-3 ${popupShadow}`}
        >
          <h3 className="text-[0.875rem] font-semibold text-text">{createTitle}</h3>
          <input
            ref={nameRef}
            type="text"
            value={newName}
            onChange={(e) => setNewName(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") {
                e.preventDefault();
                saveNew();
              }
            }}
            placeholder="Name"
            disabled={disabled}
            className="mt-2 box-border w-full rounded border border-border bg-elevated px-2 py-1.5 text-[0.813rem] text-text"
          />
          {createError ? (
            <p className="mt-1 text-[0.75rem] text-danger">{createError}</p>
          ) : null}
          <div className="mt-3 flex items-center gap-2">
            <button
              type="button"
              disabled={disabled || !newName.trim()}
              onClick={saveNew}
              className="cursor-pointer rounded-md bg-accent px-3 py-1 text-[0.813rem] font-medium text-[#1c1c1e] disabled:opacity-40"
            >
              Save
            </button>
            <button
              type="button"
              onClick={() => setMode("list")}
              className="cursor-pointer rounded-md bg-elevated px-3 py-1 text-[0.813rem] text-text hover:bg-hover"
            >
              Cancel
            </button>
          </div>
        </div>
      ) : null}
    </div>
  );
}
