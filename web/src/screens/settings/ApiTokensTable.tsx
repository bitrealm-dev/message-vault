import { Cell, Column, Row, Table, TableBody, TableHeader } from "react-aria-components";
import Button from "../../components/Button";
import { PencilIcon, TrashIcon } from "../../components/icons";
import type { ApiTokenItem } from "./apiTokensUtils";
import {
  displayKeyHint,
  formatTokenDate,
  iconBtnClass,
  scopesLabel,
  tdClass,
  tdMuted,
  thClass,
} from "./apiTokensUtils";

export default function ApiTokensTable({
  items,
  busy,
  composing,
  onRename,
  onRevoke,
}: {
  items: ApiTokenItem[];
  busy: boolean;
  composing: boolean;
  onRename: (item: ApiTokenItem) => void;
  onRevoke: (item: ApiTokenItem) => void;
}) {
  return (
    <div className="overflow-hidden rounded-xl border border-border bg-elevated">
      <Table
        aria-label="API keys"
        selectionMode="none"
        className="w-full table-fixed border-collapse text-left outline-none"
      >
        <TableHeader className="border-b border-border">
          <Column isRowHeader className={`${thClass} w-[22%]`}>
            Name
          </Column>
          <Column className={`${thClass} w-[24%]`}>Key</Column>
          <Column className={`${thClass} w-[18%]`}>Scope</Column>
          <Column className={`${thClass} w-[14%]`}>Created</Column>
          <Column className={`${thClass} w-[14%]`}>Last Used</Column>
          <Column className={`${thClass} w-[8%]`} />
        </TableHeader>
        <TableBody
          items={items}
          dependencies={[busy]}
          renderEmptyState={() =>
            composing ? null : (
              <div className="px-5 py-6 text-[0.75rem] text-muted">No API keys yet.</div>
            )
          }
          className="outline-none"
        >
          {(item) => (
            <Row id={item.id} className="border-b border-border last:border-b-0 outline-none">
              <Cell className={`${tdClass} truncate font-medium`}>
                <span className="block truncate" title={item.label}>
                  {item.label}
                </span>
              </Cell>
              <Cell className={`${tdMuted} truncate font-mono text-[0.688rem]`}>
                <span className="block truncate" title="Masked API key">
                  {displayKeyHint(item.token_hint)}
                </span>
              </Cell>
              <Cell className={tdClass}>{scopesLabel(item.scopes)}</Cell>
              <Cell className={tdMuted}>{formatTokenDate(item.created_at)}</Cell>
              <Cell className={tdMuted}>{formatTokenDate(item.last_accessed_at)}</Cell>
              <Cell className={`${tdClass}`}>
                <div className="flex items-center justify-end gap-1">
                  <Button
                    variant="secondary"
                    disabled={busy}
                    title="Edit API Key"
                    aria-label="Edit API Key"
                    onClick={() => onRename(item)}
                    className={`${iconBtnClass} hover:!text-text`}
                  >
                    <PencilIcon />
                  </Button>
                  <Button
                    variant="secondary"
                    disabled={busy}
                    title="Revoke API Key"
                    aria-label="Revoke API Key"
                    onClick={() => onRevoke(item)}
                    className={`${iconBtnClass} hover:!text-danger`}
                  >
                    <TrashIcon />
                  </Button>
                </div>
              </Cell>
            </Row>
          )}
        </TableBody>
      </Table>
    </div>
  );
}
