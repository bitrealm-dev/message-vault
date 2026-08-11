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
import ConfirmDialog from "../ConfirmDialog";
import DataCard, {
  dataCardBodyCellClass,
  dataCardHeaderCellClass,
  dataCardHeaderRowClass,
} from "../DataCard";
import { TrashIcon } from "../icons";
import Select, { ListBoxItem, selectItemClassName } from "../Select";
import {
  emptyHandleRow,
  formatHandleDate,
  formatHandleServiceLabel,
  HANDLE_SERVICE_OPTIONS,
  handleServiceSelectValue,
  inferService,
  sumHandleTotals,
  type ContactBrowseKind,
} from "./contactDrawerTypes";

type BrowseFn = (args: {
  kind: ContactBrowseKind;
  handle?: string;
  service?: string;
}) => void;

const thClass = dataCardHeaderCellClass;
const tdClass = dataCardBodyCellClass;
const tdCenterClass = tdClass;
const tdRightClass = `${tdClass} text-right`;
const thRightClass = `${thClass} text-right`;
const linkClass =
  "border-none bg-transparent p-0 text-[0.813rem] font-semibold leading-snug text-accent underline decoration-accent/80 underline-offset-2 cursor-pointer outline-none hover:decoration-accent hover:opacity-90 focus-visible:rounded-sm focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent";
const mutedClass = "text-[0.813rem] leading-snug text-muted";
const iconBtnDangerClass =
  "!inline-flex !aspect-square !h-7 !w-7 !min-h-7 !min-w-7 !shrink-0 !items-center !justify-center !rounded-sm !border-transparent !bg-transparent !p-0 !font-normal !leading-none !text-muted hover:!border-danger-soft-border hover:!bg-danger-soft-bg hover:!text-danger data-hovered:!border-danger-soft-border data-hovered:!bg-danger-soft-bg data-hovered:!text-danger data-pressed:!border-danger-soft-border data-pressed:!bg-danger-soft-bg data-pressed:!text-danger";
/** Compact labeled Save / Cancel in the add-row actions column. */
const rowActionBtnClass = "!h-7 !min-h-7 !shrink-0 !px-2 !py-0 !text-[0.813rem] !leading-none";
/** Trash: show on row hover/focus; always visible when hover isn't available. */
const rowActionsRevealClass =
  "opacity-100 [@media(hover:hover)]:opacity-0 [@media(hover:hover)]:group-hover/handle-row:opacity-100 [@media(hover:hover)]:group-data-hovered/handle-row:opacity-100 [@media(hover:hover)]:group-focus-within/handle-row:opacity-100";
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
  align = "center",
  isRowHeader,
  children,
}: {
  id: string;
  widthClass: string;
  align?: "center" | "right";
  isRowHeader?: boolean;
  children: ReactNode;
}) {
  const justify = align === "right" ? "justify-end" : "justify-center";
  const textAlign = align === "right" ? "text-right" : "text-center";
  const headerAlign = align === "right" ? thRightClass : thClass;
  return (
    <Column
      id={id}
      isRowHeader={isRowHeader}
      allowsSorting
      className={`${headerAlign} ${widthClass}`}
    >
      {({ sortDirection }) => (
        <span className={`relative mx-auto inline-flex max-w-full items-center ${justify}`}>
          <span className={`${textAlign} leading-tight`}>{children}</span>
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

type RemoveIdentityTarget = {
  handle: string;
  /** Storage id (`phone` | `whatsapp`) for the mutation API. */
  service: string | null;
  serviceLabel: string;
  threadCount: number;
};

function removeIdentityConfirmBody(target: RemoveIdentityTarget): string {
  const { handle, serviceLabel, threadCount } = target;
  if (threadCount <= 0) {
    return `Removing ${handle} for ${serviceLabel} will unlink it from this contact.`;
  }
  const threadWord = threadCount === 1 ? "thread" : "threads";
  return `Removing ${handle} for ${serviceLabel} will unlink ${threadCount} ${threadWord} from this contact. Message threads will not be removed.`;
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
      <button
        type="button"
        className={linkClass}
        onClick={onClick}
        aria-label={`Open ${text} threads`}
      >
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
              if (newHandle.trim() && !busy) onSave();
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
      <Cell className={`${tdRightClass} text-muted`}>—</Cell>
      <Cell className={`${tdRightClass} text-muted`}>—</Cell>
      <Cell className={`${tdRightClass} text-muted`}>—</Cell>
      <Cell className={`${tdClass} whitespace-nowrap`}>
        <div className="flex items-center justify-end gap-1.5">
          <Button
            variant="primary"
            disabled={!newHandle.trim() || busy}
            title="Save"
            aria-label="Save"
            onClick={onSave}
            className={rowActionBtnClass}
          >
            Save
          </Button>
          <Button
            variant="secondary"
            title="Cancel"
            aria-label="Cancel"
            onClick={onCancel}
            className={rowActionBtnClass}
          >
            Cancel
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
  const [busy, setBusy] = useState(false);
  const [sortDescriptor, setSortDescriptor] = useState<SortDescriptor | null>(null);
  const [removeTarget, setRemoveTarget] = useState<RemoveIdentityTarget | null>(null);

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
    setBusy(false);
    setSortDescriptor(null);
    setRemoveTarget(null);
  }, [contactId]);

  const requestRemoveHandle = (h: CachedContactHandle) => {
    if (busy) return;
    setRemoveTarget({
      handle: h.handle,
      service: h.service,
      serviceLabel: formatHandleServiceLabel(h.handle, h.service),
      threadCount: conversationCount(h),
    });
  };

  const confirmRemoveHandle = async () => {
    if (!removeTarget || busy) return;
    const handle = removeTarget.handle;
    const service = handleServiceSelectValue(handle, removeTarget.service);
    setBusy(true);
    try {
      await apiClient.post(`/v1/export/contacts/${contactId}`, {
        remove_handle: { handle, service },
      });
      setRemoveTarget(null);
      onHandlesChanged();
    } catch {
      /* keep dialog open for retry */
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
    <DataCard
      toolbar={
        <Button
          variant="primary"
          disabled={loading || busy || adding}
          onClick={() => {
            setAdding(true);
            setNewHandle("");
            setNewService("phone");
          }}
          className="!px-2.5 !py-1 !text-[0.75rem]"
        >
          Add
        </Button>
      }
    >
      <Table
        aria-label="Contact handles"
        className="w-full border-collapse text-left table-fixed"
        sortDescriptor={sortDescriptor ?? undefined}
        onSortChange={setSortDescriptor}
      >
        <TableHeader className={dataCardHeaderRowClass}>
          <SortableColumn id="service" isRowHeader widthClass="w-[13%]">
            Service
          </SortableColumn>
          <SortableColumn id="handle" widthClass="w-[13%]">
            Identity
          </SortableColumn>
          <SortableColumn id="start_date" widthClass="w-[10%]">
            First Seen
          </SortableColumn>
          <SortableColumn id="end_date" widthClass="w-[10%]">
            Last Seen
          </SortableColumn>
          <SortableColumn id="conversations" widthClass="w-[12%]" align="right">
            Threads
          </SortableColumn>
          <SortableColumn id="direct_messages" widthClass="w-[9%]" align="right">
            Direct
            <br />
            Messages
          </SortableColumn>
          <SortableColumn id="group_messages" widthClass="w-[9%]" align="right">
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
            dependencies={[busy, sortDescriptor]}
            className="[&_tr]:border-b [&_tr]:border-border"
          >
            {(h) => {
              const convos = conversationCount(h);
              return (
                <Row id={h.id} className="group/handle-row outline-none">
                  <Cell className={`${tdClass} overflow-hidden`}>
                    <span>
                      {formatHandleServiceLabel(h.handle, h.service)}
                    </span>
                  </Cell>
                  <Cell className={`${tdClass} overflow-hidden`}>
                    <span className="break-all" title={h.handle}>
                      {h.handle}
                    </span>
                  </Cell>
                  <Cell className={`${tdCenterClass} whitespace-nowrap text-muted`}>
                    {handleDateCell(h.start_date)}
                  </Cell>
                  <Cell className={`${tdCenterClass} whitespace-nowrap text-muted`}>
                    {handleDateCell(h.end_date)}
                  </Cell>
                  <Cell className={tdRightClass}>
                    <CountCell
                      value={convos}
                      onClick={
                        onBrowse
                          ? () =>
                              onBrowse({
                                kind: "all",
                                handle: h.handle,
                                service: inferService(h.handle, h.service),
                              })
                          : undefined
                      }
                    />
                  </Cell>
                  <Cell className={tdRightClass}>
                    <CountCell value={h.individual_message_count} />
                  </Cell>
                  <Cell className={tdRightClass}>
                    <CountCell value={h.group_message_count} />
                  </Cell>
                  <Cell className={`${tdClass} whitespace-nowrap`}>
                    <div
                      className={`flex items-center justify-center ${rowActionsRevealClass}`}
                    >
                      <Button
                        variant="ghost"
                        disabled={busy || loading}
                        title="Remove identity"
                        aria-label="Remove identity"
                        onClick={() => requestRemoveHandle(h)}
                        className={iconBtnDangerClass}
                      >
                        <TrashIcon />
                      </Button>
                    </div>
                  </Cell>
                </Row>
              );
            }}
          </TableBody>
        ) : null}
        <TableBody className="border-t-2 border-border">
          <Row id="handles-total" className="outline-none">
            <Cell className={`${tdClass} font-semibold`}>Summary</Cell>
            <Cell className={`${tdClass} text-muted`}>—</Cell>
            <Cell className={`${tdCenterClass} whitespace-nowrap text-muted`}>
              {handleDateCell(footerAsHandle.start_date)}
            </Cell>
            <Cell className={`${tdCenterClass} whitespace-nowrap text-muted`}>
              {handleDateCell(footerAsHandle.end_date)}
            </Cell>
            <Cell className={tdRightClass}>
              <CountCell
                value={conversationCount(totals)}
                onClick={
                  onBrowse && conversationCount(totals) > 0
                    ? () => onBrowse({ kind: "all" })
                    : undefined
                }
              />
            </Cell>
            <Cell className={tdRightClass}>
              <CountCell value={totals.individual_message_count} />
            </Cell>
            <Cell className={tdRightClass}>
              <CountCell value={totals.group_message_count} />
            </Cell>
            <Cell className={tdClass} />
          </Row>
        </TableBody>
      </Table>
      <ConfirmDialog
        open={removeTarget !== null}
        title="Remove identity"
        body={removeTarget ? removeIdentityConfirmBody(removeTarget) : ""}
        confirmLabel="Remove identity"
        danger
        busy={busy}
        onClose={() => {
          if (!busy) setRemoveTarget(null);
        }}
        onConfirm={() => void confirmRemoveHandle()}
      />
    </DataCard>
  );
}
