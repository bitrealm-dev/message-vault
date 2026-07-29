"use client";

import { formatPhoneDisplay } from "@/lib/phoneE164";
import type { VaultOwner } from "@/lib/vaultOwner";
import { IconHoverTarget } from "./IconHoverLabel";
import { ListHistoryMenu } from "./history";
import { ChevronDownIcon, PeopleGroupIcon } from "./icons";
import { PaneSearchField } from "./PaneSearchField";

/** Contact-list chrome for vault owner — matches Panel 2; most controls inert. */
export function MyContactPane({ owner }: { owner: VaultOwner }) {
  const displayName = owner.display_name || "Me";
  const preferredHandle = owner.phones[0]
    ? formatPhoneDisplay(owner.phones[0])
    : "";
  const letter = (displayName.trim().charAt(0) || "#").toUpperCase();

  const toolbarBtn =
    "flex h-7 w-7 items-center justify-center rounded-md border border-border bg-elevated text-muted";
  /** Match LabelsMenu trigger used on All contacts (icon + chevron, not square). */
  const groupsToolbarBtn =
    "inline-flex h-7 items-center gap-1.5 rounded-md bg-elevated px-2.5 text-muted opacity-40";

  return (
    <aside className="flex h-full min-h-0 w-full flex-col bg-sidebar">
      <div className="flex h-[45px] shrink-0 items-center border-b border-border px-3">
        <PaneSearchField
          value=""
          placeholder="Search Group Messages"
          disabled
        />
      </div>
      <div className="flex h-[45px] shrink-0 items-center justify-between overflow-visible border-b border-border px-3">
        <label className="flex min-w-0 items-center gap-2">
          <IconHoverTarget label="Select all (unavailable)" placement="bottom">
            <input
              type="checkbox"
              checked={false}
              disabled
              aria-label="Select all (unavailable)"
              className="checkbox-list opacity-40"
            />
          </IconHoverTarget>
        </label>
        <div className="flex shrink-0 items-center gap-1.5 overflow-visible">
          <IconHoverTarget label="Labels (unavailable)" placement="bottom">
            <span className={groupsToolbarBtn} aria-hidden>
              <PeopleGroupIcon className="size-4 shrink-0" />
              <ChevronDownIcon className="size-3.5 shrink-0 opacity-70" />
            </span>
          </IconHoverTarget>
          <IconHoverTarget label="Sort (unavailable)" placement="bottom">
            <span className={`${toolbarBtn} opacity-40`} aria-hidden>
              <SortArrowsIcon />
            </span>
          </IconHoverTarget>
          <ListHistoryMenu />
        </div>
      </div>
      <div className="min-h-0 flex-1 overflow-y-auto [scrollbar-gutter:stable]">
        <div>
          <div className="sticky top-0 z-10 border-b border-border bg-sidebar px-3 py-1 text-[11px] font-semibold text-muted">
            {letter}
          </div>
          <div
            className="relative flex w-full items-start gap-1.5 bg-accent/20 py-2 pr-3 pl-0 select-none"
            aria-label={`My contact information: ${displayName}`}
          >
            <span
              aria-hidden
              className="absolute top-1 bottom-1 left-0 w-1 rounded-full bg-accent/80"
            />
            <span className="flex w-10 shrink-0 items-center justify-center self-stretch -my-2">
              <input
                type="checkbox"
                checked={false}
                disabled
                tabIndex={-1}
                aria-hidden
                className="checkbox-list opacity-40"
              />
            </span>
            <span className="flex min-w-0 flex-1 items-start gap-2 text-left">
              <span className="min-w-0">
                <span className="block truncate text-[13px] font-semibold text-text">
                  {displayName}
                </span>
                {preferredHandle && preferredHandle !== displayName ? (
                  <span className="block truncate text-[12px] text-muted">
                    {preferredHandle}
                  </span>
                ) : (
                  <span className="block h-[1.5rem] text-[12px]" aria-hidden />
                )}
              </span>
            </span>
          </div>
        </div>
      </div>
    </aside>
  );
}

function SortArrowsIcon() {
  return (
    <svg width="18" height="18" viewBox="0 0 16 16" fill="none" aria-hidden>
      <path
        d="M5 3v10M5 3l-2.5 2.5M5 3l2.5 2.5M11 13V3M11 13l-2.5-2.5M11 13l2.5-2.5"
        stroke="currentColor"
        strokeWidth="1.5"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  );
}
