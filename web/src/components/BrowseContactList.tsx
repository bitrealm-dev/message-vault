"use client";

import {
  contactAvatarColor,
  contactInitials,
} from "@/lib/contactInitials";
import { formatPhoneDisplay } from "@/lib/phoneE164";
import type { ContactListItem } from "@/lib/types";
import {
  useRef,
  useState,
  type ChangeEvent,
  type MouseEvent,
  type ReactNode,
  type RefObject,
} from "react";
import { CountBadge } from "./CountBadge";
import { ListHistoryMenu, type ListHistoryMenuItem } from "./history";
import { IconHoverTarget } from "./IconHoverLabel";
import {
  GroupMessagesOutlineIcon,
  PencilIcon,
  XIcon,
} from "./icons";
import { PaneSearchField } from "./PaneSearchField";
import { SortByMenu, type SortMode, type SortOrder } from "./SortByMenu";
import { useDateTimeFormat } from "./useDateTimeFormat";
import { useMessageBadgePrefs } from "./useMessageBadgePrefs";

export function BrowseContactList({
  sectionLabel,
  selectAllRef,
  allGroupSelected,
  visibleCount,
  sortedCount,
  query,
  onQueryChange,
  onToggleSelectAll,
  onNewContact,
  onImportVcf,
  vaultReadOnly = false,
  labelsMenu,
  onEdit,
  editDisabled = false,
  onTrashContact,
  deleteDisabled = false,
  sort,
  sortOrder,
  onSortChange,
  grouped,
  contactId,
  contextMenuId = null,
  selectedIds,
  onSelectColumnClick,
  onNamePhoneClick,
  onContextMenu,
}: {
  sectionLabel: string;
  selectAllRef: RefObject<HTMLInputElement | null>;
  allGroupSelected: boolean;
  visibleCount: number;
  sortedCount: number;
  query: string;
  onQueryChange: (q: string) => void;
  onToggleSelectAll: () => void;
  onNewContact: (anchorEl: HTMLElement) => void;
  /** Upload a .vcf and import contacts (Contacts section). */
  onImportVcf?: (file: File) => Promise<void>;
  vaultReadOnly?: boolean;
  /** Icon-only LabelsMenu element rendered first in the toolbar cluster. */
  labelsMenu?: ReactNode;
  onEdit?: (anchorEl: HTMLElement) => void;
  editDisabled?: boolean;
  onTrashContact?: () => void;
  deleteDisabled?: boolean;
  sort: SortMode;
  sortOrder: SortOrder;
  onSortChange: (next: { sort: SortMode; order: SortOrder }) => void;
  grouped: [string, ContactListItem[]][];
  contactId: number | null;
  /** Right-clicked contact while its context menu is open. */
  contextMenuId?: number | null;
  selectedIds: Set<number>;
  onSelectColumnClick: (id: number, e: MouseEvent) => void;
  onNamePhoneClick: (id: number, e: MouseEvent | { shiftKey: boolean; metaKey: boolean; ctrlKey: boolean }) => void;
  onContextMenu: (id: number, x: number, y: number) => void;
}) {
  const vcfInputRef = useRef<HTMLInputElement>(null);
  const [vcfImporting, setVcfImporting] = useState(false);
  const {
    showMessageBadge,
    showGroupMessageBadge,
    showContactInitials,
    showContactDateRange,
  } = useMessageBadgePrefs();
  const { formatDateRange } = useDateTimeFormat();
  const onVcfPicked = async (e: ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    e.target.value = "";
    if (!file || !onImportVcf) return;
    setVcfImporting(true);
    try {
      await onImportVcf(file);
    } finally {
      setVcfImporting(false);
    }
  };

  const menuItems: ListHistoryMenuItem[] = [
    {
      key: "new-contact",
      label: "New",
      icon: <NewContactIcon className="size-5 shrink-0 opacity-80" />,
      onClick: (triggerEl) => {
        if (triggerEl) onNewContact(triggerEl);
      },
    },
    ...(onImportVcf
      ? [
          {
            key: "import-vcf",
            label: vcfImporting ? "Importing…" : "Import VCF",
            icon: <ImportVcfIcon className="size-5 shrink-0 opacity-80" />,
            disabled: vcfImporting,
            onClick: () => {
              vcfInputRef.current?.click();
            },
          } satisfies ListHistoryMenuItem,
        ]
      : []),
    ...(onEdit
      ? [
          {
            key: "edit",
            label: "Edit",
            icon: <PencilIcon className="size-5 shrink-0 opacity-80" />,
            disabled: editDisabled,
            onClick: (triggerEl) => {
              if (triggerEl) onEdit(triggerEl);
            },
          } satisfies ListHistoryMenuItem,
        ]
      : []),
    ...(onTrashContact
      ? [
          {
            key: "delete",
            label: "Delete",
            icon: <XIcon className="size-5 shrink-0 opacity-80" />,
            disabled: deleteDisabled,
            danger: true,
            onClick: () => onTrashContact(),
          } satisfies ListHistoryMenuItem,
        ]
      : []),
  ];

  return (
    <aside className="flex h-full min-h-0 w-full flex-col bg-sidebar">
      {onImportVcf && (
        <input
          ref={vcfInputRef}
          type="file"
          accept=".vcf,.vcard,text/vcard,text/x-vcard"
          className="hidden"
          onChange={(e) => void onVcfPicked(e)}
        />
      )}

      <div className="flex h-[45px] shrink-0 items-center border-b border-border px-3">
        <PaneSearchField
          value={query}
          onChange={onQueryChange}
          placeholder="Filter contacts"
        />
      </div>
      <div className="flex h-[45px] shrink-0 items-center justify-between overflow-visible border-b border-border px-3">
        <label className="flex min-w-0 items-center gap-2">
          <IconHoverTarget label="Select all" placement="bottom">
            <input
              ref={selectAllRef}
              type="checkbox"
              checked={allGroupSelected}
              disabled={visibleCount === 0}
              aria-label={`Select all ${sectionLabel}`}
              onChange={onToggleSelectAll}
              className="checkbox-list"
            />
          </IconHoverTarget>
          <span className="truncate text-[13px] text-muted tabular-nums">
            {selectedIds.size > 0 ? selectedIds.size : ""}
          </span>
        </label>
        <div className="flex shrink-0 items-center gap-1.5 overflow-visible">
          {!vaultReadOnly && labelsMenu}
          <SortByMenu sort={sort} order={sortOrder} onChange={onSortChange} />
          <ListHistoryMenu items={vaultReadOnly ? [] : menuItems} />
        </div>
      </div>
      <div className="min-h-0 flex-1 overflow-y-auto [scrollbar-gutter:stable]">
        {sortedCount === 0 && (
          <p className="px-3 py-4 text-[12px] text-muted">No matches</p>
        )}
        {grouped.map(([letter, items]) => (
          <div key={letter || "all"}>
            {!query.trim() && letter && (
              <div className="sticky top-0 z-10 border-b border-border bg-sidebar px-3 py-1 text-[11px] font-semibold text-muted">
                {letter}
              </div>
            )}
            {items.map((c, i) => {
              const menuTarget = contextMenuId != null && c.id === contextMenuId;
              const active = c.id === contactId || menuTarget;
              const checked = selectedIds.has(c.id);
              const showInsetDivider = i < items.length - 1;
              const selectionActive = selectedIds.size >= 1;
              return (
                <div
                  key={c.id}
                  role={selectionActive ? "button" : undefined}
                  tabIndex={selectionActive ? 0 : undefined}
                  onClick={
                    selectionActive
                      ? (e) => onNamePhoneClick(c.id, e)
                      : undefined
                  }
                  onKeyDown={
                    selectionActive
                      ? (e) => {
                          if (e.key === "Enter" || e.key === " ") {
                            e.preventDefault();
                            onNamePhoneClick(c.id, {
                              shiftKey: e.shiftKey,
                              metaKey: e.metaKey,
                              ctrlKey: e.ctrlKey,
                            });
                          }
                        }
                      : undefined
                  }
                  onContextMenu={(e) => {
                    e.preventDefault();
                    onContextMenu(c.id, e.clientX, e.clientY);
                  }}
                  onMouseDown={(e) => {
                    if (e.shiftKey) e.preventDefault();
                  }}
                  className={`group relative flex w-full items-start gap-1.5 py-3 pr-3 pl-0 select-none outline-none focus:outline-none focus-visible:outline-none ${
                    selectionActive ? "cursor-pointer" : ""
                  } ${
                    checked
                      ? "bg-accent/40 hover:bg-accent/50"
                      : active
                        ? "bg-accent/20 hover:bg-accent/25"
                        : "hover:bg-hover-strong"
                  }`}
                >
                  {active && !checked && (
                    <span
                      aria-hidden
                      className="absolute top-1 bottom-1 left-0 w-1 rounded-full bg-accent/80"
                    />
                  )}
                  {checked && (
                    <span
                      aria-hidden
                      className="absolute top-1 bottom-1 left-0 w-1 rounded-full bg-accent"
                    />
                  )}
                  <button
                    type="button"
                    aria-pressed={checked}
                    aria-label={`Select ${c.displayName}`}
                    onClick={(e) => onSelectColumnClick(c.id, e)}
                    onMouseDown={(e) => {
                      e.stopPropagation();
                      if (e.shiftKey) e.preventDefault();
                    }}
                    className="group/select flex w-10 shrink-0 cursor-pointer items-center justify-center self-stretch -my-3 outline-none focus:outline-none focus-visible:outline-none"
                  >
                    {showContactInitials ? (
                      <span
                        aria-hidden
                        className={`flex size-7 items-center justify-center rounded-full text-[11px] font-semibold text-white ${
                          checked
                            ? "hidden"
                            : selectionActive
                              ? "group-hover:hidden"
                              : "group-hover/select:hidden"
                        }`}
                        style={{
                          backgroundColor: contactAvatarColor({
                            displayName: c.displayName,
                            preferredHandle: c.preferredHandle,
                            firstName: c.firstName,
                            lastName: c.lastName,
                          }),
                        }}
                      >
                        {contactInitials(c)}
                      </span>
                    ) : null}
                    <span
                      className={
                        !showContactInitials || checked
                          ? "inline-flex"
                          : selectionActive
                            ? "hidden group-hover:inline-flex"
                            : "hidden group-hover/select:inline-flex"
                      }
                    >
                      <input
                        type="checkbox"
                        checked={checked}
                        readOnly
                        tabIndex={-1}
                        aria-hidden
                        className="checkbox-list pointer-events-none"
                      />
                    </span>
                  </button>
                  <button
                    type="button"
                    onClick={(e) => {
                      e.stopPropagation();
                      onNamePhoneClick(c.id, e);
                    }}
                    onMouseDown={(e) => {
                      if (e.shiftKey) e.preventDefault();
                    }}
                    className="flex min-w-0 flex-1 items-stretch justify-between gap-2 self-stretch text-left outline-none focus:outline-none focus-visible:outline-none"
                  >
                    <span className="min-w-0 flex-1 self-start">
                      <span className="block truncate text-[13px] font-semibold text-text">
                        {c.displayName}
                      </span>
                      {(() => {
                        const formattedHandle = c.preferredHandle
                          ? formatPhoneDisplay(c.preferredHandle)
                          : "";
                        const showHandle =
                          !!formattedHandle &&
                          formattedHandle !== c.displayName;
                        const dateLabel =
                          showContactDateRange && c.dateStart && c.dateEnd
                            ? formatDateRange(c.dateStart, c.dateEnd, " – ")
                            : null;
                        const showGroupIcon =
                          showGroupMessageBadge && c.groupMessageCount > 0;
                        const showCountBadge =
                          showMessageBadge && c.messageCount > 0;
                        const rowLabels = (c.labels ?? []).filter(Boolean);
                        const hasBottomLine =
                          !!dateLabel ||
                          showGroupIcon ||
                          showCountBadge ||
                          rowLabels.length > 0;
                        if (!showHandle && !hasBottomLine) return null;
                        return (
                          <>
                            {showHandle ? (
                              <span className="block truncate text-[12px] text-muted">
                                {formattedHandle}
                              </span>
                            ) : null}
                            {rowLabels.length > 0 ? (
                              <span className="mt-0.5 flex min-w-0 flex-wrap gap-1">
                                {rowLabels.slice(0, 3).map((label) => (
                                  <span
                                    key={label}
                                    className="max-w-[7rem] truncate rounded bg-elevated px-1.5 py-0.5 text-[10px] font-medium text-muted"
                                    title={label}
                                  >
                                    {label}
                                  </span>
                                ))}
                                {rowLabels.length > 3 ? (
                                  <span className="rounded bg-elevated px-1.5 py-0.5 text-[10px] font-medium text-muted">
                                    +{rowLabels.length - 3}
                                  </span>
                                ) : null}
                              </span>
                            ) : null}
                            {dateLabel || showGroupIcon || showCountBadge ? (
                              <span className="mt-0.5 flex min-w-0 items-center justify-between gap-2">
                                <span className="inline-flex shrink-0 items-center gap-1.5">
                                  {/* Reserve the icon slot whenever the setting is on so badges align across rows. */}
                                  {showGroupMessageBadge && (
                                    <span
                                      title={
                                        showGroupIcon
                                          ? "In group messages"
                                          : undefined
                                      }
                                      className="inline-flex items-center"
                                    >
                                      <GroupMessagesOutlineIcon
                                        className={`size-3.5 shrink-0 text-muted opacity-80 ${
                                          showGroupIcon ? "" : "invisible"
                                        }`}
                                      />
                                    </span>
                                  )}
                                  {showCountBadge && (
                                    <CountBadge
                                      count={c.messageCount}
                                      title="1:1 messages"
                                    />
                                  )}
                                </span>
                                {dateLabel ? (
                                  <span className="min-w-0 truncate text-right text-[11px] text-muted tabular-nums">
                                    {dateLabel}
                                  </span>
                                ) : null}
                              </span>
                            ) : null}
                          </>
                        );
                      })()}
                    </span>
                  </button>
                  {showInsetDivider && (
                    <span
                      aria-hidden
                      className="pointer-events-none absolute right-3 bottom-0 left-3 h-px bg-border/60"
                    />
                  )}
                </div>
              );
            })}
          </div>
        ))}
      </div>
    </aside>
  );
}

export function NewContactIcon({ className }: { className?: string }) {
  return (
    <svg
      className={className}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.75"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden
    >
      <circle cx="7.25" cy="8" r="3" />
      <path d="M2.25 19.25c.65-3 2.85-4.75 5-4.75s4.35 1.75 5 4.75" />
      <path d="M19 9v6M16 12h6" strokeWidth="2" />
    </svg>
  );
}

function ImportVcfIcon({ className }: { className?: string }) {
  return (
    <svg
      className={className}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.75"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden
    >
      <path d="M12 3v12" />
      <path d="m7 10 5 5 5-5" />
      <path d="M5 19h14" />
    </svg>
  );
}
