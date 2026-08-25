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
import { type ContactBrowseKind, emptyHandleRow, sumHandleTotals } from "./contactDrawerTypes";
import { renderHandleSummaryRow, renderHandleTableRow } from "./HandleTableRow";
import { SortableColumn } from "./handleTableHelpers";
import { removeIdentityConfirmBody, sortValue } from "./handleTableLogic";
import { tdClass, thClass } from "./handleTableStyles";
import { headerLabelMinWidth } from "./headerLabelMinWidth";
import { useHandleMutations } from "./useHandleMutations";

type BrowseFn = (args: { kind: ContactBrowseKind; handle?: string; service?: string }) => void;

const ACTIONS_COL_WIDTH = 40;

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

  return (
    <DataCard title={title} intro={intro} toolbar={toolbarExtra}>
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
      <ResizableTableContainer className="w-full overflow-x-auto">
        <Table
          aria-label="Contact handles"
          className="w-full border-collapse text-left"
          sortDescriptor={sortDescriptor ?? undefined}
          onSortChange={setSortDescriptor}
        >
          <TableHeader className={dataCardHeaderRowClass}>
            <SortableColumn
              id="service"
              isRowHeader
              align="left"
              allowsResizing
              defaultWidth="1.2fr"
              minWidth={headerLabelMinWidth("Service")}
            >
              Service
            </SortableColumn>
            <SortableColumn
              id="handle"
              align="left"
              allowsResizing
              defaultWidth="1.4fr"
              minWidth={headerLabelMinWidth("Identity")}
            >
              Identity
            </SortableColumn>
            <SortableColumn
              id="name_alias"
              align="left"
              allowsResizing
              defaultWidth="1.2fr"
              minWidth={headerLabelMinWidth("Alias")}
            >
              Alias
            </SortableColumn>
            <SortableColumn
              id="start_date"
              align="left"
              allowsResizing
              defaultWidth="1fr"
              minWidth={headerLabelMinWidth("First Seen")}
            >
              First Seen
            </SortableColumn>
            <SortableColumn
              id="end_date"
              align="left"
              allowsResizing
              defaultWidth="1fr"
              minWidth={headerLabelMinWidth("Last Seen")}
            >
              Last Seen
            </SortableColumn>
            <SortableColumn
              id="conversations"
              align="left"
              allowsResizing
              defaultWidth="1fr"
              minWidth={headerLabelMinWidth("Threads")}
            >
              Threads
            </SortableColumn>
            <SortableColumn
              id="direct_messages"
              align="left"
              allowsResizing
              defaultWidth="1.1fr"
              minWidth={headerLabelMinWidth("Direct Messages")}
            >
              Direct Messages
            </SortableColumn>
            <SortableColumn
              id="group_messages"
              align="left"
              allowsResizing
              defaultWidth="1.1fr"
              minWidth={headerLabelMinWidth("Group Messages")}
            >
              Group Messages
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
