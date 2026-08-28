import { Cell, Row } from "react-aria-components";
import type { CachedContactHandle } from "../../lib/contactDetailCache";
import Button from "../Button";
import { TrashIcon } from "../icons";
import {
  type ContactBrowseKind,
  formatHandleServiceLabel,
  inferService,
} from "./contactDrawerTypes";
import { CountCell } from "./handleTableHelpers";
import { conversationCount, handleDateCell } from "./handleTableLogic";
import { rowActionsRevealClass, tdClass, tdLeftClass, tdRightClass } from "./handleTableStyles";

type BrowseFn = (args: { kind: ContactBrowseKind; handle?: string; service?: string }) => void;

/** Last count column: room on the right for the hover trash control. */
const tdGroupMessagesClass = `${tdRightClass} relative !pr-9`;

/** Returns a RAC `Row` element (must stay a direct TableBody child). */
export function renderHandleTableRow(
  h: CachedContactHandle & { id: string },
  opts: {
    busy: boolean;
    loading: boolean;
    onBrowse?: BrowseFn;
    onRequestRemove: (h: CachedContactHandle) => void;
  },
) {
  const convos = conversationCount(h);
  const alias = h.name_alias?.trim() || "";
  const loading = opts.loading;
  return (
    <Row id={h.id} className="group/handle-row outline-none">
      <Cell className={tdLeftClass}>
        <span>{formatHandleServiceLabel(h.handle, h.service)}</span>
      </Cell>
      <Cell className={tdLeftClass}>
        <span className="break-all" title={h.handle}>
          {h.handle}
        </span>
      </Cell>
      <Cell className={`${tdLeftClass} text-muted`}>
        <span className="break-normal hyphens-none" title={alias || undefined}>
          {loading ? "—" : alias || "—"}
        </span>
      </Cell>
      <Cell className={`${tdClass} whitespace-nowrap text-muted`}>
        {loading ? "—" : handleDateCell(h.start_date)}
      </Cell>
      <Cell className={`${tdClass} whitespace-nowrap text-muted`}>
        {loading ? "—" : handleDateCell(h.end_date)}
      </Cell>
      <Cell className={tdRightClass}>
        <CountCell
          value={convos}
          loading={loading}
          onClick={
            opts.onBrowse
              ? () =>
                  opts.onBrowse?.({
                    kind: "all",
                    handle: h.handle,
                    service: inferService(h.handle, h.service),
                  })
              : undefined
          }
        />
      </Cell>
      <Cell className={tdRightClass}>
        <CountCell value={h.individual_message_count} loading={loading} />
      </Cell>
      <Cell className={tdGroupMessagesClass}>
        <CountCell value={h.group_message_count} loading={loading} />
        <div className={`absolute top-1/2 right-0 -translate-y-1/2 ${rowActionsRevealClass}`}>
          <Button
            variant="ghostDanger"
            size="icon"
            disabled={opts.busy || opts.loading}
            title="Remove identity"
            aria-label="Remove identity"
            onClick={() => opts.onRequestRemove(h)}
          >
            <TrashIcon />
          </Button>
        </div>
      </Cell>
    </Row>
  );
}

export function renderHandleSummaryRow(
  totals: CachedContactHandle,
  onBrowse?: BrowseFn,
  loading = false,
) {
  return (
    <Row id="handles-total" className="outline-none">
      <Cell className={`${tdLeftClass} font-semibold`}>Summary</Cell>
      <Cell className={`${tdLeftClass} text-muted`}>—</Cell>
      <Cell className={`${tdLeftClass} text-muted`}>—</Cell>
      <Cell className={`${tdClass} whitespace-nowrap text-muted`}>
        {loading ? "—" : handleDateCell(totals.start_date)}
      </Cell>
      <Cell className={`${tdClass} whitespace-nowrap text-muted`}>
        {loading ? "—" : handleDateCell(totals.end_date)}
      </Cell>
      <Cell className={tdRightClass}>
        <CountCell
          value={conversationCount(totals)}
          loading={loading}
          onClick={
            onBrowse && conversationCount(totals) > 0 ? () => onBrowse({ kind: "all" }) : undefined
          }
        />
      </Cell>
      <Cell className={tdRightClass}>
        <CountCell value={totals.individual_message_count} loading={loading} />
      </Cell>
      <Cell className={tdGroupMessagesClass}>
        <CountCell value={totals.group_message_count} loading={loading} />
      </Cell>
    </Row>
  );
}
