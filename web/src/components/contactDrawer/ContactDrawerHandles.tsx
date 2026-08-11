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
import AddIdentityDialog from "./AddIdentityDialog";
import {
  emptyHandleRow,
  formatHandleDate,
  formatHandleServiceLabel,
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
/** Trash: show on row hover; on keyboard, when the button itself is focus-visible.
 * Avoid row focus-within — table row focus after click would leave trash stuck on. */
const rowActionsRevealClass =
  "opacity-100 [@media(hover:hover)]:opacity-0 [@media(hover:hover)]:group-hover/handle-row:opacity-100 [@media(hover:hover)]:group-data-hovered/handle-row:opacity-100 [@media(hover:hover)]:has-[:focus-visible]:opacity-100";

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

function removeIdentityConfirmBody(target: RemoveIdentityTarget): ReactNode {
  const { handle, serviceLabel, threadCount } = target;
  const emphasize = "font-medium text-accent";
  const serviceId = (
    <>
      <span className={emphasize}>{serviceLabel}</span>{" "}
      <span className={`${emphasize} break-all`}>{handle}</span>
    </>
  );
  if (threadCount <= 0) {
    return (
      <p className="mt-3 text-[0.875rem] leading-relaxed text-muted">
        Removing {serviceId} will unlink it from this contact.
      </p>
    );
  }
  const threadWord = threadCount === 1 ? "thread" : "threads";
  return (
    <p className="mt-3 text-[0.875rem] leading-relaxed text-muted">
      Removing {serviceId} will unlink {threadCount} {threadWord} from this
      contact. Unlinked data will not be deleted.
    </p>
  );
}

function sortValue(h: CachedContactHandle, col: string): string | number {
  switch (col) {
    case "service":
      return formatHandleServiceLabel(h.handle, h.service).toLowerCase();
    case "handle":
      return h.handle.toLowerCase();
    case "start_date":
      return h.start_date ?? "";
    case "end_date":
      return h.end_date ?? "";
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

  const confirmAdd = async (args: { handle: string; service: string }) => {
    if (busy) return;
    setBusy(true);
    try {
      await apiClient.post(`/v1/export/contacts/${contactId}`, {
        add_handle: { handle: args.handle, service: args.service },
      });
      setAdding(false);
      onHandlesChanged();
    } catch {
      /* keep dialog open for retry */
    } finally {
      setBusy(false);
    }
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
          disabled={loading || busy}
          onClick={() => setAdding(true)}
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
          <SortableColumn id="service" isRowHeader widthClass="w-[18%]">
            Service
          </SortableColumn>
          <SortableColumn id="handle" widthClass="w-[12%]">
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
          <Column className={`${thClass} w-[10%] !cursor-default`} />
        </TableHeader>
        {handleRows.length === 0 ? (
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
        ) : (
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
        )}
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
      <AddIdentityDialog
        open={adding}
        busy={busy}
        existingHandles={handleRows}
        onClose={() => {
          if (!busy) setAdding(false);
        }}
        onConfirm={(args) => void confirmAdd(args)}
      />
      <ConfirmDialog
        open={removeTarget !== null}
        title="Remove identity from contact?"
        body={removeTarget ? removeIdentityConfirmBody(removeTarget) : null}
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
