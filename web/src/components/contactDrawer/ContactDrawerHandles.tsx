import { useState } from "react";
import { apiClient } from "../../lib/api";
import type { CachedContactDetail } from "../../lib/contactDetailCache";
import Button from "../Button";
import Select, { ListBoxItem, selectItemClassName } from "../Select";
import { inferService, SERVICES } from "./contactDrawerTypes";

export function ContactDrawerHandles({
  contactId,
  handleRows,
  loading,
  onHandlesChanged,
}: {
  contactId: string;
  handleRows: CachedContactDetail["handles"];
  loading: boolean;
  onHandlesChanged: () => void;
}) {
  const [newHandle, setNewHandle] = useState("");
  const [newService, setNewService] = useState("discord");

  const addHandle = async () => {
    if (!newHandle.trim()) return;
    try {
      await apiClient.post(`/v1/export/contacts/${contactId}`, {
        add_handle: { handle: newHandle.trim(), service: newService },
      });
      setNewHandle("");
      onHandlesChanged();
    } catch {
      // Leave the input in place so the user can retry
    }
  };

  return (
    <>
      <h3 className="mb-2 text-[0.75rem] uppercase text-muted">Handles</h3>
      {handleRows.length === 0 ? (
        <div className="mb-2 text-[0.813rem] text-muted">
          {loading ? "Loading…" : "No handles"}
        </div>
      ) : (
        <div className="mb-3">
          {handleRows.map((h, i) => (
            <div
              key={`${h.handle}-${i}`}
              className="flex items-center gap-3 border-b border-border py-1.5 text-[0.875rem]"
            >
              <span className="min-w-[5.5rem] shrink-0 text-muted">
                {inferService(h.handle, h.service)}
              </span>
              <span className="min-w-0 flex-1 truncate">
                {h.handle}
              </span>
            </div>
          ))}
        </div>
      )}

      <div className="mb-[0.35rem] flex flex-wrap items-center gap-2">
        <Select
          selectedKey={newService}
          onSelectionChange={(k) => setNewService(String(k))}
          aria-label="Handle service"
          triggerClassName="!text-[0.813rem]"
          className="w-[110px] shrink-0"
        >
          {SERVICES.map((s) => (
            <ListBoxItem key={s} id={s} className={selectItemClassName}>{s}</ListBoxItem>
          ))}
        </Select>
        <input
          type="text"
          value={newHandle}
          onChange={(e) => setNewHandle(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") {
              e.preventDefault();
              void addHandle();
            }
          }}
          placeholder="user#1234, @handle…"
          className="min-w-0 flex-1 rounded border border-border bg-elevated px-2 py-1.5 text-[0.813rem] text-text"
        />
        <Button
          variant="primary"
          onClick={addHandle}
          disabled={!newHandle.trim() || loading}
          className="!px-3 !py-1 !text-[0.813rem]"
        >
          Add
        </Button>
      </div>
    </>
  );
}
