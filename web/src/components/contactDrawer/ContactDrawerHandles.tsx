import { type ReactNode, useEffect, useMemo, useState } from "react";
import {
  Cell,
  Column,
  ResizableTableContainer,
  Row,
  type SortDescriptor,
  Table,
  TableBody,
  TableHeader,
} from "react-aria-components";
import type { CachedContactDetail, CachedContactHandle } from "../../lib/contactDetailCache";
import Button from "../Button";
import ConfirmDialog from "../ConfirmDialog";
import DataCard, { dataCardHeaderRowClass } from "../DataCard";
import AddIdentityDialog from "./AddIdentityDialog";
import {
  type ContactBrowseKind,
  emptyHandleRow,
  formatHandleServiceLabel,
  sumHandleTotals,
} from "./contactDrawerTypes";
import { renderHandleSummaryRow, renderHandleTableRow } from "./HandleTableRow";
import { SortableColumn } from "./handleTableHelpers";
import { conversationCount, removeIdentityConfirmBody, sortValue } from "./handleTableLogic";
import { tdClass, thClass } from "./handleTableStyles";
import { columnInitialWidth, headerLabelMinWidth } from "./headerLabelMinWidth";
import { useHandleMutations } from "./useHandleMutations";

type BrowseFn = (args: { kind: ContactBrowseKind; handle?: string; service?: string }) => void;

const ACTIONS_COL_WIDTH = 40;

const twoLineHeader = (line1: string, line2: string) => (
  <>
    <span className="sr-only">{`${line1} ${line2}`}</span>
    <span className="flex flex-col items-start leading-tight" aria-hidden="true">
      <span className="whitespace-nowrap">{line1}</span>
      <span className="whitespace-nowrap">{line2}</span>
    </span>
  </>
);

type ColumnSize = {
  width: number;
  min: number;
};

function collectColumnWidths(
  handleRows: CachedContactDetail["handles"],
  loading: boolean,
): {
  service: ColumnSize;
  handle: ColumnSize;
  alias: ColumnSize;
  startDate: ColumnSize;
  endDate: ColumnSize;
  conversations: ColumnSize;
  directMessages: ColumnSize;
  groupMessages: ColumnSize;
} {
  const serviceMin = headerLabelMinWidth("Service");
  const handleMin = headerLabelMinWidth("Identity");
  const aliasMin = headerLabelMinWidth("Alias");
  const startMin = headerLabelMinWidth("First Seen");
  const endMin = headerLabelMinWidth("Last Seen");
  const threadsMin = headerLabelMinWidth("Threads");
  // Two-line headers: min from the longest line ("Messages").
  const messagesMin = headerLabelMinWidth("Messages");
  const dateCol = columnInitialWidth(Math.max(startMin, endMin), ["2020-12-31"]);
  const totals = sumHandleTotals(handleRows);

  const serviceTexts = [
    "Summary",
    ...handleRows.map((h) => formatHandleServiceLabel(h.handle, h.service)),
  ];
  const handleTexts = ["—", ...handleRows.map((h) => h.handle)];
  const aliasTexts = ["—", ...handleRows.map((h) => (loading ? "—" : h.name_alias?.trim() || "—"))];
  const threadTexts = [
    conversationCount(totals).toLocaleString(),
    ...handleRows.map((h) => conversationCount(h).toLocaleString()),
  ];
  const directTexts = [
    totals.individual_message_count.toLocaleString(),
    ...handleRows.map((h) => h.individual_message_count.toLocaleString()),
  ];
  const groupTexts = [
    totals.group_message_count.toLocaleString(),
    ...handleRows.map((h) => h.group_message_count.toLocaleString()),
  ];

  return {
    service: { width: columnInitialWidth(serviceMin, serviceTexts), min: serviceMin },
    handle: { width: columnInitialWidth(handleMin, handleTexts), min: handleMin },
    alias: { width: columnInitialWidth(aliasMin, aliasTexts), min: aliasMin },
    startDate: { width: dateCol, min: startMin },
    endDate: { width: dateCol, min: endMin },
    conversations: { width: columnInitialWidth(threadsMin, threadTexts), min: threadsMin },
    directMessages: { width: columnInitialWidth(messagesMin, directTexts), min: messagesMin },
    groupMessages: { width: columnInitialWidth(messagesMin, groupTexts), min: messagesMin },
  };
}

export function ContactDrawerHandles({
  contactId,
  handleRows,
  loading,
  onHandlesChanged,
  onBrowse,
  title = "Contact Identity",
  intro,
  toolbarExtra,
}: {
  contactId: string;
  handleRows: CachedContactDetail["handles"];
  loading: boolean;
  onHandlesChanged: () => void;
  onBrowse?: BrowseFn;
  title?: ReactNode;
  intro?: ReactNode;
  toolbarExtra?: ReactNode;
}) {
  const [sortDescriptor, setSortDescriptor] = useState<SortDescriptor | null>(null);
  const {
    adding,
    setAdding,
    busy,
    removeTarget,
    setRemoveTarget,
    requestRemoveHandle,
    confirmRemoveHandle,
    confirmAdd,
  } = useHandleMutations({ contactId, onHandlesChanged });

  const totals = sumHandleTotals(handleRows);

  const columnWidths = useMemo(
    () => collectColumnWidths(handleRows, loading),
    [handleRows, loading],
  );
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
    void contactId;
    setSortDescriptor(null);
  }, [contactId]);

  const footerAsHandle: CachedContactHandle = {
    ...emptyHandleRow(""),
    ...totals,
  };

  // Remount when contact or loading flips so defaultWidth applies to real data after stubs.
  const tableKey = `${contactId}:${loading ? "loading" : "ready"}`;

  return (
    <DataCard
      title={title}
      intro={intro}
      toolbar={toolbarExtra}
      bodyClassName="min-w-0 overflow-x-hidden"
    >
      <div className="mb-2 flex justify-end">
        <Button
          variant="primary"
          disabled={loading || busy}
          onClick={() => setAdding(true)}
          className="!px-2.5 !py-1 !text-[0.75rem]"
        >
          Add identity
        </Button>
      </div>
      <ResizableTableContainer key={tableKey} className="w-full overflow-x-auto">
        <Table
          aria-label="Contact handles"
          className="border-collapse text-left"
          sortDescriptor={sortDescriptor ?? undefined}
          onSortChange={setSortDescriptor}
        >
          <TableHeader className={dataCardHeaderRowClass}>
            <SortableColumn
              id="service"
              isRowHeader
              align="left"
              allowsResizing
              defaultWidth={columnWidths.service.width}
              minWidth={columnWidths.service.min}
            >
              Service
            </SortableColumn>
            <SortableColumn
              id="handle"
              align="left"
              allowsResizing
              defaultWidth={columnWidths.handle.width}
              minWidth={columnWidths.handle.min}
            >
              Identity
            </SortableColumn>
            <SortableColumn
              id="name_alias"
              align="left"
              allowsResizing
              defaultWidth={columnWidths.alias.width}
              minWidth={columnWidths.alias.min}
            >
              Alias
            </SortableColumn>
            <SortableColumn
              id="start_date"
              align="left"
              allowsResizing
              defaultWidth={columnWidths.startDate.width}
              minWidth={columnWidths.startDate.min}
            >
              <span className="whitespace-nowrap">First Seen</span>
            </SortableColumn>
            <SortableColumn
              id="end_date"
              align="left"
              allowsResizing
              defaultWidth={columnWidths.endDate.width}
              minWidth={columnWidths.endDate.min}
            >
              <span className="whitespace-nowrap">Last Seen</span>
            </SortableColumn>
            <SortableColumn
              id="conversations"
              align="left"
              allowsResizing
              defaultWidth={columnWidths.conversations.width}
              minWidth={columnWidths.conversations.min}
            >
              Threads
            </SortableColumn>
            <SortableColumn
              id="direct_messages"
              align="left"
              allowsResizing
              defaultWidth={columnWidths.directMessages.width}
              minWidth={columnWidths.directMessages.min}
            >
              {twoLineHeader("Direct", "Messages")}
            </SortableColumn>
            <SortableColumn
              id="group_messages"
              align="left"
              allowsResizing
              defaultWidth={columnWidths.groupMessages.width}
              minWidth={columnWidths.groupMessages.min}
            >
              {twoLineHeader("Group", "Messages")}
            </SortableColumn>
            <Column
              id="actions"
              width={ACTIONS_COL_WIDTH}
              minWidth={ACTIONS_COL_WIDTH}
              defaultWidth={ACTIONS_COL_WIDTH}
              className={`${thClass} !cursor-default`}
            />
          </TableHeader>
          {handleRows.length === 0 ? (
            <TableBody className="[&_tr]:border-b [&_tr]:border-border">
              <Row id="handles-empty" className="outline-none">
                <Cell className={`${tdClass} !text-left text-muted`}>
                  {loading ? "Loading…" : "No handles"}
                </Cell>
                <Cell className={tdClass} />
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
              {(h) =>
                renderHandleTableRow(h, {
                  busy,
                  loading,
                  onBrowse,
                  onRequestRemove: requestRemoveHandle,
                })
              }
            </TableBody>
          )}
          <TableBody className="border-t-2 border-border">
            {renderHandleSummaryRow(footerAsHandle, onBrowse, loading)}
          </TableBody>
        </Table>
      </ResizableTableContainer>
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
