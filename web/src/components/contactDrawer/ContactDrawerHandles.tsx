import { useState } from "react";
import {
  Table,
  TableHeader,
  TableBody,
  Column,
  Row,
  Cell,
} from "react-aria-components";
import { apiClient } from "../../lib/api";
import type { CachedContactDetail, CachedContactHandle } from "../../lib/contactDetailCache";
import Button from "../Button";
import Select, { ListBoxItem, selectItemClassName } from "../Select";
import {
  emptyHandleRow,
  formatHandleServiceLabel,
  handleDateRangeLabel,
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
  "px-1.5 py-1 text-left text-[0.688rem] font-semibold uppercase tracking-[0.04em] text-muted";
const tdClass = "px-1.5 py-1.5 align-top text-[0.813rem] text-text";
const linkClass =
  "border-none bg-transparent p-0 text-[0.813rem] font-semibold leading-snug text-accent text-left no-underline cursor-pointer hover:underline";
const mutedClass = "text-[0.813rem] leading-snug text-muted";
const iconBtnClass =
  "!inline-flex !aspect-square !h-7 !w-7 !min-h-7 !min-w-7 !shrink-0 !items-center !justify-center !rounded-sm !p-0 !leading-none !text-muted";

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

function conversationCount(h: {
  individual_conversations: number;
  group_conversations: number;
}): number {
  return h.individual_conversations + h.group_conversations;
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

  const totals = sumHandleTotals(handleRows);

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

  const footerAsHandle: CachedContactHandle = {
    ...emptyHandleRow(""),
    ...totals,
  };

  return (
    <div>
      <div className="mb-2 flex items-center justify-between gap-2">
        <h3 className="m-0 text-[0.75rem] uppercase text-muted">Handles</h3>
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

      {handleRows.length === 0 && !adding ? (
        <div className="mb-2 text-[0.813rem] text-muted">
          {loading ? "Loading…" : "No handles"}
        </div>
      ) : (
        <Table aria-label="Contact handles" className="w-full border-collapse text-left">
          <TableHeader className="border-b border-border">
            <Column isRowHeader className={`${thClass} w-[12%]`}>
              Service
            </Column>
            <Column className={`${thClass} w-[18%]`}>Handle</Column>
            <Column className={`${thClass} w-[18%]`}>Date Range</Column>
            <Column className={`${thClass} w-[10%]`}>Conversations</Column>
            <Column className={`${thClass} w-[14%]`}>Direct Messages</Column>
            <Column className={`${thClass} w-[14%]`}>Group Messages</Column>
            <Column className={`${thClass} w-[10%]`} />
          </TableHeader>
          <TableBody
            items={handleRows.map((h, i) => ({ ...h, id: `${h.handle}-${i}` }))}
            dependencies={[editingHandle, editHandle, editService, busy]}
            className="[&_tr]:border-b [&_tr]:border-border"
          >
            {(h) => {
              const editing = editingHandle === h.handle;
              const convos = conversationCount(h);
              return (
                <Row id={h.id} className="outline-none">
                  <Cell className={tdClass}>
                    {editing ? (
                      <Select
                        selectedKey={editService}
                        onSelectionChange={(k) => setEditService(String(k))}
                        aria-label="Handle service"
                        triggerClassName="!text-[0.75rem]"
                        className="w-full min-w-0"
                      >
                        {HANDLE_SERVICE_OPTIONS.map((s) => (
                          <ListBoxItem key={s.value} id={s.value} className={selectItemClassName}>
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
                  <Cell className={tdClass}>
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
                            cancelEdit();
                          }
                        }}
                        className="box-border w-full rounded border border-border bg-elevated px-1.5 py-1 text-[0.813rem] text-text"
                        autoFocus
                      />
                    ) : (
                      <span className="truncate" title={h.handle}>
                        {h.handle}
                      </span>
                    )}
                  </Cell>
                  <Cell className={`${tdClass} whitespace-nowrap text-muted`}>
                    {handleDateRangeLabel(h)}
                  </Cell>
                  <Cell className={tdClass}>
                    <CountCell
                      value={convos}
                      onClick={
                        onBrowse
                          ? () => onBrowse({ kind: "all", handle: h.handle })
                          : undefined
                      }
                    />
                  </Cell>
                  <Cell className={tdClass}>
                    <CountCell value={h.individual_message_count} />
                  </Cell>
                  <Cell className={tdClass}>
                    <CountCell value={h.group_message_count} />
                  </Cell>
                  <Cell className={`${tdClass} whitespace-nowrap`}>
                    {editing ? (
                      <div className="flex items-center justify-end gap-1">
                        <Button
                          variant="secondary"
                          disabled={busy || !editHandle.trim()}
                          title="Save"
                          aria-label="Save"
                          onClick={() => void saveEdit()}
                          className={`${iconBtnClass} hover:!text-text`}
                        >
                          ✓
                        </Button>
                        <Button
                          variant="secondary"
                          title="Cancel"
                          aria-label="Cancel"
                          onClick={cancelEdit}
                          className={`${iconBtnClass} hover:!text-text`}
                        >
                          ×
                        </Button>
                      </div>
                    ) : (
                      <div className="flex items-center justify-end gap-1">
                        <Button
                          variant="secondary"
                          disabled={busy || loading}
                          title="Edit handle"
                          aria-label="Edit handle"
                          onClick={() => startEdit(h)}
                          className={`${iconBtnClass} hover:!text-text`}
                        >
                          <PencilIcon />
                        </Button>
                        <Button
                          variant="secondary"
                          disabled={busy || loading}
                          title="Unlink handle"
                          aria-label="Unlink handle"
                          onClick={() => void removeHandle(h.handle)}
                          className={`${iconBtnClass} hover:!text-danger`}
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
          {handleRows.length > 0 ? (
            <TableBody className="border-t border-border">
              <Row id="handles-total" className="outline-none">
                <Cell className={`${tdClass} font-semibold text-muted`}>Total</Cell>
                <Cell className={`${tdClass} text-muted`}>—</Cell>
                <Cell className={`${tdClass} whitespace-nowrap text-muted`}>
                  {handleDateRangeLabel(footerAsHandle)}
                </Cell>
                <Cell className={tdClass}>
                  <CountCell value={conversationCount(totals)} />
                </Cell>
                <Cell className={tdClass}>
                  <CountCell value={totals.individual_message_count} />
                </Cell>
                <Cell className={tdClass}>
                  <CountCell value={totals.group_message_count} />
                </Cell>
                <Cell className={tdClass} />
              </Row>
            </TableBody>
          ) : null}
        </Table>
      )}

      {adding ? (
        <div className="mt-2 flex flex-wrap items-center gap-2 border-b border-border pb-2">
          <Select
            selectedKey={newService}
            onSelectionChange={(k) => setNewService(String(k))}
            aria-label="New handle service"
            triggerClassName="!text-[0.75rem]"
            className="w-[110px] shrink-0"
          >
            {HANDLE_SERVICE_OPTIONS.map((s) => (
              <ListBoxItem key={s.value} id={s.value} className={selectItemClassName}>
                {s.label}
              </ListBoxItem>
            ))}
          </Select>
          <input
            type="text"
            value={newHandle}
            onChange={(e) => setNewHandle(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") {
                e.preventDefault();
                void saveAdd();
              } else if (e.key === "Escape") {
                e.preventDefault();
                setAdding(false);
                setNewHandle("");
              }
            }}
            placeholder="user#1234, @handle…"
            className="min-w-0 flex-1 rounded border border-border bg-elevated px-2 py-1.5 text-[0.813rem] text-text"
            autoFocus
          />
          <Button
            variant="primary"
            onClick={() => void saveAdd()}
            disabled={!newHandle.trim() || busy}
            className="!px-3 !py-1 !text-[0.813rem]"
          >
            Save
          </Button>
          <Button
            onClick={() => {
              setAdding(false);
              setNewHandle("");
            }}
            className="!px-3 !py-1 !text-[0.813rem]"
          >
            Cancel
          </Button>
        </div>
      ) : null}
    </div>
  );
}
