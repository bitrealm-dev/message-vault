import { useEffect, useMemo, useState, type ReactNode } from "react";
import {
  Table,
  TableHeader,
  TableBody,
  Column,
  Row,
  Cell,
  type SortDescriptor,
} from "react-aria-components";
import { apiClient } from "../../lib/api";
import type { CachedContactDetail, CachedContactHandle } from "../../lib/contactDetailCache";
import Button from "../Button";
import Select, { ListBoxItem, selectItemClassName } from "../Select";
import {
  emptyHandleRow,
  formatHandleDate,
  formatHandleServiceLabel,
  handleServiceSelectValue,
  HANDLE_SERVICE_OPTIONS,
  sumHandleTotals,
  type ContactBrowseKind,
} from "./contactDrawerTypes";

type BrowseFn = (args: {
  kind: ContactBrowseKind;
  handle?: string;
}) => void;

const thClass =
  "px-2 py-2 text-center text-[0.688rem] font-semibold uppercase tracking-[0.04em] text-muted outline-none cursor-pointer hover:text-text data-hovered:text-text";
const tdClass = "px-3 py-2.5 align-middle text-center text-[0.813rem] leading-snug text-text";
const tdCenterClass = tdClass;
const linkClass =
  "border-none bg-transparent p-0 text-[0.813rem] font-semibold leading-snug text-accent no-underline cursor-pointer hover:underline";
const mutedClass = "text-[0.813rem] leading-snug text-muted";
const iconBtnClass =
  "!inline-flex !aspect-square !h-7 !w-7 !min-h-7 !min-w-7 !shrink-0 !items-center !justify-center !rounded-sm !border-transparent !bg-transparent !p-0 !font-normal !leading-none !text-muted hover:!border-border hover:!bg-elevated hover:!text-text data-hovered:!border-border data-hovered:!bg-elevated data-hovered:!text-text data-pressed:!border-border data-pressed:!bg-hover";
const iconBtnDangerClass = `${iconBtnClass} hover:!text-danger data-hovered:!text-danger data-pressed:!text-danger`;
/** Keep edit controls inside the cell (no min-width that spills into Handle). */
const serviceSelectTriggerClass =
  "!box-border !h-7 !min-h-7 !w-full !rounded !px-1.5 !py-0 !text-[0.813rem] !font-normal !leading-none !bg-elevated";
const serviceSelectValueClass = "!text-[0.813rem] !font-normal !leading-none";
const handleEditInputClass =
  "box-border h-7 w-full min-w-0 max-w-full rounded border border-border bg-elevated px-1.5 py-0 text-[0.813rem] font-normal leading-none text-text outline-none focus:border-accent";

function serviceSelectItemClassName(state: {
  isFocused: boolean;
  isSelected: boolean;
}): string {
  return selectItemClassName(state).replace("text-[0.875rem]", "text-[0.813rem] font-normal");
}

function SortableColumn({
  id,
  widthClass,
  isRowHeader,
  children,
}: {
  id: string;
  widthClass: string;
  isRowHeader?: boolean;
  children: ReactNode;
}) {
  return (
    <Column
      id={id}
      isRowHeader={isRowHeader}
      allowsSorting
      className={`${thClass} ${widthClass}`}
    >
      {({ sortDirection }) => (
        <span className="relative mx-auto inline-flex max-w-full items-center justify-center">
          <span className="text-center leading-tight">{children}</span>
          <span
            aria-hidden="true"
            className={`absolute top-1/2 left-[calc(100%+0.25rem)] -translate-y-1/2 text-[0.55rem] leading-none ${
              sortDirection ? "text-accent" : "invisible"
            }`}
          >
            {sortDirection === "descending" ? "▼" : "▲"}
          </span>
        </span>
      )}
    </Column>
  );
}

function conversationCount(h: {
  individual_conversations: number;
  group_conversations: number;
}): number {
  return h.individual_conversations + h.group_conversations;
}

function sortValue(h: CachedContactHandle, column: string): string | number {
  switch (column) {
    case "service":
      return formatHandleServiceLabel(h.handle, h.service).toLowerCase();
    case "handle":
      return h.handle.toLowerCase();
    case "start_date":
      return formatHandleDate(h.start_date) ?? "";
    case "end_date":
      return formatHandleDate(h.end_date) ?? "";
    case "conversations":
      return conversationCount(h);
    case "direct_messages":
      return h.individual_message_count;
    case "group_messages":
      return h.group_message_count;
    default:
      return "";
  }
}

function handleDateCell(iso: string | null | undefined): string {
  return formatHandleDate(iso) ?? "—";
}

function TrashIcon() {
  return (
    <svg
      width="13"
      height="13"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      <path d="M3 6h18" />
      <path d="M8 6V4h8v2" />
      <path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6" />
      <path d="M10 11v6" />
      <path d="M14 11v6" />
    </svg>
  );
}

function PencilIcon() {
  return (
    <svg
      width="13"
      height="13"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      <path d="M12 20h9" />
      <path d="M16.5 3.5a2.1 2.1 0 0 1 3 3L7 19l-4 1 1-4Z" />
    </svg>
  );
}

function CountCell({
  value,
  onClick,
}: {
  value: number;
  onClick?: () => void;
}) {
  const text = value.toLocaleString();
  if (value > 0 && onClick) {
    return (
      <button type="button" className={linkClass} onClick={onClick}>
        {text}
      </button>
    );
  }
  return <span className={value === 0 ? mutedClass : undefined}>{text}</span>;
}

function AddHandleTableRow({
  newService,
  setNewService,
  newHandle,
  setNewHandle,
  busy,
  onSave,
  onCancel,
}: {
  newService: string;
  setNewService: (s: string) => void;
  newHandle: string;
  setNewHandle: (s: string) => void;
  busy: boolean;
  onSave: () => void;
  onCancel: () => void;
}) {
  return (
    <Row id="handles-add" className="outline-none">
      <Cell className={`${tdClass} overflow-hidden`}>
        <Select
          selectedKey={newService}
          onSelectionChange={(k) => setNewService(String(k))}
          aria-label="New handle service"
          triggerClassName={serviceSelectTriggerClass}
          valueClassName={serviceSelectValueClass}
          className="block w-full min-w-0 max-w-full"
        >
          {HANDLE_SERVICE_OPTIONS.map((s) => (
            <ListBoxItem key={s.value} id={s.value} className={serviceSelectItemClassName}>
              {s.label}
            </ListBoxItem>
          ))}
        </Select>
      </Cell>
      <Cell className={`${tdClass} overflow-hidden`}>
        <input
          type="text"
          value={newHandle}
          onChange={(e) => setNewHandle(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") {
              e.preventDefault();
              onSave();
            } else if (e.key === "Escape") {
              e.preventDefault();
              e.stopPropagation();
              onCancel();
            }
          }}
          placeholder="user#1234, @handle…"
          className={handleEditInputClass}
          autoFocus
        />
      </Cell>
      <Cell className={`${tdCenterClass} text-muted`}>—</Cell>
      <Cell className={`${tdCenterClass} text-muted`}>—</Cell>
      <Cell className={`${tdCenterClass} text-muted`}>—</Cell>
      <Cell className={`${tdCenterClass} text-muted`}>—</Cell>
      <Cell className={`${tdCenterClass} text-muted`}>—</Cell>
      <Cell className={`${tdClass} whitespace-nowrap`}>
        <div className="flex items-center justify-center gap-1">
          <Button
            variant="ghost"
            disabled={!newHandle.trim() || busy}
            title="Save"
            aria-label="Save"
            onClick={onSave}
            className={iconBtnClass}
          >
            ✓
          </Button>
          <Button
            variant="ghost"
            title="Cancel"
            aria-label="Cancel"
            onClick={onCancel}
            className={iconBtnClass}
          >
            ×
          </Button>
        </div>
      </Cell>
    </Row>
  );
}

export function ContactDrawerHandles({
  contactId,
  handleRows,
  loading,
  onHandlesChanged,
  onBrowse,
}: {
  contactId: string;
  handleRows: CachedContactDetail["handles"];
  loading: boolean;
  onHandlesChanged: () => void;
  onBrowse?: BrowseFn;
}) {
  const [adding, setAdding] = useState(false);
  const [newHandle, setNewHandle] = useState("");
  const [newService, setNewService] = useState("phone");
  const [editingHandle, setEditingHandle] = useState<string | null>(null);
  const [editHandle, setEditHandle] = useState("");
  const [editService, setEditService] = useState("phone");
  const [busy, setBusy] = useState(false);
  const [sortDescriptor, setSortDescriptor] = useState<SortDescriptor | null>(null);

  const totals = sumHandleTotals(handleRows);

  const sortedRows = useMemo(() => {
    type RowItem = CachedContactHandle & { id: string };
    const rows: RowItem[] = handleRows.map((h, i) => ({
      ...h,
      id: `${h.handle}-${i}`,
    }));
    if (!sortDescriptor?.column) return rows;
    const col = String(sortDescriptor.column);
    const dir = sortDescriptor.direction === "descending" ? -1 : 1;
    return [...rows].sort((a, b) => {
      const av = sortValue(a, col);
      const bv = sortValue(b, col);
      if (av < bv) return -1 * dir;
      if (av > bv) return 1 * dir;
      return a.handle.localeCompare(b.handle);
    });
  }, [handleRows, sortDescriptor]);

  useEffect(() => {
    setAdding(false);
    setNewHandle("");
    setNewService("phone");
    setEditingHandle(null);
    setEditHandle("");
    setBusy(false);
    setSortDescriptor(null);
  }, [contactId]);

  const startEdit = (h: CachedContactHandle) => {
    setAdding(false);
    setEditingHandle(h.handle);
    setEditHandle(h.handle);
    setEditService(handleServiceSelectValue(h.handle, h.service));
  };

  const cancelEdit = () => {
    setEditingHandle(null);
    setEditHandle("");
  };

  const saveEdit = async () => {
    if (!editingHandle || !editHandle.trim() || busy) return;
    setBusy(true);
    try {
      await apiClient.post(`/v1/export/contacts/${contactId}`, {
        update_handle: {
          previous_handle: editingHandle,
          handle: editHandle.trim(),
          service: editService,
        },
      });
      cancelEdit();
      onHandlesChanged();
    } catch {
      /* keep edit open for retry */
    } finally {
      setBusy(false);
    }
  };

  const removeHandle = async (handle: string) => {
    if (busy) return;
    if (!window.confirm(`Unlink ${handle} from this contact?`)) return;
    setBusy(true);
    try {
      await apiClient.post(`/v1/export/contacts/${contactId}`, {
        remove_handle: { handle },
      });
      if (editingHandle === handle) cancelEdit();
      onHandlesChanged();
    } catch {
      /* ignore */
    } finally {
      setBusy(false);
    }
  };

  const saveAdd = async () => {
    if (!newHandle.trim() || busy) return;
    setBusy(true);
    try {
      await apiClient.post(`/v1/export/contacts/${contactId}`, {
        add_handle: { handle: newHandle.trim(), service: newService },
      });
      setNewHandle("");
      setAdding(false);
      onHandlesChanged();
    } catch {
      /* keep add row open */
    } finally {
      setBusy(false);
    }
  };

  const cancelAdd = () => {
    setAdding(false);
    setNewHandle("");
  };

  const footerAsHandle: CachedContactHandle = {
    ...emptyHandleRow(""),
    ...totals,
  };

  return (
    <div className="w-full max-w-4xl rounded-lg border border-border bg-elevated p-4">
      <div className="mb-3 flex justify-end pr-5">
        <Button
          variant="primary"
          disabled={loading || busy || adding}
          onClick={() => {
            cancelEdit();
            setAdding(true);
            setNewHandle("");
            setNewService("phone");
          }}
          className="!px-2.5 !py-1 !text-[0.75rem]"
        >
          Add
        </Button>
      </div>

      <div className="overflow-x-auto">
      <Table
        aria-label="Contact handles"
        className="w-full border-collapse text-left table-fixed"
        sortDescriptor={sortDescriptor ?? undefined}
        onSortChange={setSortDescriptor}
      >
        <TableHeader className="border-b border-border">
          <SortableColumn id="service" isRowHeader widthClass="w-[15%]">
            Service
          </SortableColumn>
          <SortableColumn id="handle" widthClass="w-[15%]">
            Identity
          </SortableColumn>
          <SortableColumn id="start_date" widthClass="w-[11%]">
            First Seen
          </SortableColumn>
          <SortableColumn id="end_date" widthClass="w-[11%]">
            Last Seen
          </SortableColumn>
          <SortableColumn id="conversations" widthClass="w-[12%]">
            Threads
          </SortableColumn>
          <SortableColumn id="direct_messages" widthClass="w-[8%]">
            Direct
            <br />
            Messages
          </SortableColumn>
          <SortableColumn id="group_messages" widthClass="w-[8%]">
            Group
            <br />
            Messages
          </SortableColumn>
          <Column className={`${thClass} w-[14%] !cursor-default`} />
        </TableHeader>
        {adding ? (
          <TableBody
            className="[&_tr]:border-b [&_tr]:border-border"
            dependencies={[newHandle, newService, busy]}
          >
            <AddHandleTableRow
              newService={newService}
              setNewService={setNewService}
              newHandle={newHandle}
              setNewHandle={setNewHandle}
              busy={busy}
              onSave={() => void saveAdd()}
              onCancel={cancelAdd}
            />
          </TableBody>
        ) : null}
        {handleRows.length === 0 && !adding ? (
          <TableBody className="[&_tr]:border-b [&_tr]:border-border">
            <Row id="handles-empty" className="outline-none">
              <Cell className={`${tdClass} text-muted`}>
                {loading ? "Loading…" : "No handles"}
              </Cell>
              <Cell className={tdClass} />
              <Cell className={tdClass} />
              <Cell className={tdClass} />
              <Cell className={tdClass} />
              <Cell className={tdClass} />
              <Cell className={tdClass} />
              <Cell className={tdClass} />
            </Row>
          </TableBody>
        ) : null}
        {handleRows.length > 0 ? (
          <TableBody
            items={sortedRows}
            dependencies={[editingHandle, editHandle, editService, busy, sortDescriptor]}
            className="[&_tr]:border-b [&_tr]:border-border"
          >
            {(h) => {
              const editing = editingHandle === h.handle;
              const convos = conversationCount(h);
              return (
                <Row id={h.id} className="outline-none">
                  <Cell className={`${tdClass} overflow-hidden`}>
                    {editing ? (
                      <Select
                        selectedKey={editService}
                        onSelectionChange={(k) => setEditService(String(k))}
                        aria-label="Handle service"
                        triggerClassName={serviceSelectTriggerClass}
                        valueClassName={serviceSelectValueClass}
                        className="block w-full min-w-0 max-w-full"
                      >
                        {HANDLE_SERVICE_OPTIONS.map((s) => (
                          <ListBoxItem key={s.value} id={s.value} className={serviceSelectItemClassName}>
                            {s.label}
                          </ListBoxItem>
                        ))}
                      </Select>
                    ) : (
                      <span className="text-muted">
                        {formatHandleServiceLabel(h.handle, h.service)}
                      </span>
                    )}
                  </Cell>
                  <Cell className={`${tdClass} overflow-hidden`}>
                    {editing ? (
                      <input
                        type="text"
                        value={editHandle}
                        onChange={(e) => setEditHandle(e.target.value)}
                        onKeyDown={(e) => {
                          if (e.key === "Enter") {
                            e.preventDefault();
                            void saveEdit();
                          } else if (e.key === "Escape") {
                            e.preventDefault();
                            e.stopPropagation();
                            cancelEdit();
                          }
                        }}
                        className={handleEditInputClass}
                        autoFocus
                      />
                    ) : (
                      <span className="break-all" title={h.handle}>
                        {h.handle}
                      </span>
                    )}
                  </Cell>
                  <Cell className={`${tdCenterClass} whitespace-nowrap text-muted`}>
                    {handleDateCell(h.start_date)}
                  </Cell>
                  <Cell className={`${tdCenterClass} whitespace-nowrap text-muted`}>
                    {handleDateCell(h.end_date)}
                  </Cell>
                  <Cell className={tdCenterClass}>
                    <CountCell
                      value={convos}
                      onClick={
                        onBrowse
                          ? () => onBrowse({ kind: "all", handle: h.handle })
                          : undefined
                      }
                    />
                  </Cell>
                  <Cell className={tdCenterClass}>
                    <CountCell value={h.individual_message_count} />
                  </Cell>
                  <Cell className={tdCenterClass}>
                    <CountCell value={h.group_message_count} />
                  </Cell>
                  <Cell className={`${tdClass} whitespace-nowrap`}>
                    {editing ? (
                      <div className="flex items-center justify-center gap-1">
                        <Button
                          variant="ghost"
                          disabled={busy || !editHandle.trim()}
                          title="Save"
                          aria-label="Save"
                          onClick={() => void saveEdit()}
                          className={iconBtnClass}
                        >
                          ✓
                        </Button>
                        <Button
                          variant="ghost"
                          title="Cancel"
                          aria-label="Cancel"
                          onClick={cancelEdit}
                          className={iconBtnClass}
                        >
                          ×
                        </Button>
                      </div>
                    ) : (
                      <div className="flex items-center justify-center gap-1">
                        <Button
                          variant="ghost"
                          disabled={busy || loading}
                          title="Edit handle"
                          aria-label="Edit handle"
                          onClick={() => startEdit(h)}
                          className={iconBtnClass}
                        >
                          <PencilIcon />
                        </Button>
                        <Button
                          variant="ghost"
                          disabled={busy || loading}
                          title="Unlink handle"
                          aria-label="Unlink handle"
                          onClick={() => void removeHandle(h.handle)}
                          className={iconBtnDangerClass}
                        >
                          <TrashIcon />
                        </Button>
                      </div>
                    )}
                  </Cell>
                </Row>
              );
            }}
          </TableBody>
        ) : null}
        <TableBody className="border-t border-border">
          <Row id="handles-total" className="outline-none">
            <Cell className={`${tdClass} font-semibold text-muted`}>Total</Cell>
            <Cell className={`${tdClass} text-muted`}>—</Cell>
            <Cell className={`${tdCenterClass} whitespace-nowrap text-muted`}>
              {handleDateCell(footerAsHandle.start_date)}
            </Cell>
            <Cell className={`${tdCenterClass} whitespace-nowrap text-muted`}>
              {handleDateCell(footerAsHandle.end_date)}
            </Cell>
            <Cell className={tdCenterClass}>
              <CountCell value={conversationCount(totals)} />
            </Cell>
            <Cell className={tdCenterClass}>
              <CountCell value={totals.individual_message_count} />
            </Cell>
            <Cell className={tdCenterClass}>
              <CountCell value={totals.group_message_count} />
            </Cell>
            <Cell className={tdClass} />
          </Row>
        </TableBody>
      </Table>
      </div>
    </div>
  );
}
