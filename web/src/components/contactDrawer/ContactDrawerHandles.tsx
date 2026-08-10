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
      <h3 style={{ fontSize: "0.75rem", color: "var(--muted)", textTransform: "uppercase", marginBottom: "0.5rem" }}>Handles</h3>
      {handleRows.length === 0 ? (
        <div style={{ fontSize: "0.813rem", color: "var(--muted)", marginBottom: "0.5rem" }}>
          {loading ? "Loading…" : "No handles"}
        </div>
      ) : (
        <div style={{ marginBottom: "0.75rem" }}>
          {handleRows.map((h, i) => (
            <div
              key={`${h.handle}-${i}`}
              style={{
                display: "flex",
                alignItems: "center",
                gap: "0.75rem",
                padding: "0.375rem 0",
                borderBottom: "1px solid var(--border)",
                fontSize: "0.875rem",
              }}
            >
              <span style={{ color: "var(--muted)", minWidth: "5.5rem", flexShrink: 0 }}>
                {inferService(h.handle, h.service)}
              </span>
              <span style={{ flex: 1, minWidth: 0, overflow: "hidden", textOverflow: "ellipsis" }}>
                {h.handle}
              </span>
            </div>
          ))}
        </div>
      )}

      <div
        style={{
          display: "flex",
          gap: "0.5rem",
          flexWrap: "wrap",
          marginBottom: "0.35rem",
          alignItems: "center",
        }}
      >
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
          style={{
            flex: 1,
            minWidth: 0,
            padding: "0.375rem 0.5rem",
            fontSize: "0.813rem",
            border: "1px solid var(--border)",
            borderRadius: "4px",
            backgroundColor: "var(--elevated)",
            color: "var(--text)",
          }}
        />
        <Button
          variant="primary"
          onClick={addHandle}
          disabled={!newHandle.trim() || loading}
          style={{ fontSize: "0.813rem", padding: "0.25rem 0.75rem" }}
        >
          Add
        </Button>
      </div>
    </>
  );
}
