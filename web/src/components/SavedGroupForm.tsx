import { useState } from "react";
import ModalShell from "./ModalShell";
import Button from "./Button";

interface SavedGroupFormProps {
  onSave: (name: string, query: string) => void;
  onCancel: () => void;
  initial?: { name: string; query: string };
}

export default function SavedGroupForm({ onSave, onCancel, initial }: SavedGroupFormProps) {
  const [open, setOpen] = useState(true);
  const [name, setName] = useState(initial?.name || "");
  const [query, setQuery] = useState(initial?.query || "");

  const handleSave = () => {
    if (!name.trim() || !query.trim()) return;
    onSave(name.trim(), query.trim());
  };

  return (
    <ModalShell
      open={open}
      onOpenChange={(o) => {
        if (!o) onCancel();
      }}
      label={initial ? "Edit saved group" : "New saved group"}
      maxWidth="25rem"
    >
      <h3 className="mb-4 text-[1rem] text-text">
        {initial ? "Edit saved group" : "New saved group"}
      </h3>

      <label className="mb-1 block text-[0.813rem] font-medium text-text">
        Name
      </label>
      <input
        type="text"
        value={name}
        onChange={(e) => setName(e.target.value)}
        onKeyDown={(e) => e.key === "Enter" && handleSave()}
        placeholder="e.g. Work team"
        className="mb-3 box-border w-full rounded border border-border bg-elevated px-2 py-1.5 text-[0.875rem] text-text"
        autoFocus
      />

      <label className="mb-1 block text-[0.813rem] font-medium text-text">
        Query
      </label>
      <input
        type="text"
        value={query}
        onChange={(e) => setQuery(e.target.value)}
        onKeyDown={(e) => e.key === "Enter" && handleSave()}
        placeholder="e.g. from:bob service:discord"
        className="mb-4 box-border w-full rounded border border-border bg-elevated px-2 py-1.5 text-[0.875rem] text-text"
      />

      <div className="flex justify-end gap-2">
        <Button onClick={onCancel} style={{ padding: "0.375rem 0.75rem" }}>
          Cancel
        </Button>
        <Button
          variant="primary"
          onClick={handleSave}
          disabled={!name.trim() || !query.trim()}
          style={{ padding: "0.375rem 1rem" }}
        >
          Save
        </Button>
      </div>
    </ModalShell>
  );
}
