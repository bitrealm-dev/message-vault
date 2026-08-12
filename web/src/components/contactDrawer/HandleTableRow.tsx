import { Cell, Row } from "react-aria-components";
import type { CachedContactHandle } from "../../lib/contactDetailCache";
import Button from "../Button";
import { TrashIcon } from "../icons";
import {
  formatHandleServiceLabel,
  inferService,
  type ContactBrowseKind,
} from "./contactDrawerTypes";
import { CountCell } from "./handleTableHelpers";
import { conversationCount, handleDateCell } from "./handleTableLogic";
import {
  iconBtnDangerClass,
  rowActionsRevealClass,
  tdCenterClass,
  tdClass,
  tdRightClass,
} from "./handleTableStyles";

type BrowseFn = (args: {
  kind: ContactBrowseKind;
  handle?: string;
  service?: string;
}) => void;

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
      <Cell className={`${tdClass} overflow-hidden text-muted`}>
        <span className="break-all" title={alias || undefined}>
          {alias || "—"}
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
            opts.onBrowse
              ? () =>
                  opts.onBrowse!({
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
            disabled={opts.busy || opts.loading}
            title="Remove identity"
            aria-label="Remove identity"
            onClick={() => opts.onRequestRemove(h)}
            className={iconBtnDangerClass}
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
) {
  return (
    <Row id="handles-total" className="outline-none">
      <Cell className={`${tdClass} font-semibold`}>Summary</Cell>
      <Cell className={`${tdClass} text-muted`}>—</Cell>
      <Cell className={`${tdClass} text-muted`}>—</Cell>
      <Cell className={`${tdCenterClass} whitespace-nowrap text-muted`}>
        {handleDateCell(totals.start_date)}
      </Cell>
      <Cell className={`${tdCenterClass} whitespace-nowrap text-muted`}>
        {handleDateCell(totals.end_date)}
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
  );
}
