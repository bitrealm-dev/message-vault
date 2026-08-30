import { useState } from "react";
import Button from "./Button";
import ModalShell from "./ModalShell";

interface SavedSearchFormProps {
  onSave: (name: string, query: string) => void;
  onCancel: () => void;
  initial?: { name: string; query: string };
}

export default function SavedSearchForm({ onSave, onCancel, initial }: SavedSearchFormProps) {
  const [name, setName] = useState(initial?.name || "");
  const [query, setQuery] = useState(initial?.query || "");

  const handleSave = () => {
    if (!name.trim() || !query.trim()) return;
    onSave(name.trim(), query.trim());
  };

  return (
    <ModalShell
      open
      onOpenChange={(o) => {
        if (!o) onCancel();
      }}
      label={initial ? "Edit saved search" : "New saved search"}
      maxWidth="25rem"
    >
      <h3 className="mb-4 text-[1rem] text-text">
        {initial ? "Edit saved search" : "New saved search"}
      </h3>

      <label className="mb-3 block">
        <span className="mb-1 block text-[0.813rem] font-medium text-text">Name</span>
        <input
          type="text"
          value={name}
          onChange={(e) => setName(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && handleSave()}
          placeholder="e.g. Work team"
          className="box-border w-full rounded border border-border bg-elevated px-2 py-1.5 text-[0.875rem] text-text"
        />
      </label>

      <label className="mb-4 block">
        <span className="mb-1 block text-[0.813rem] font-medium text-text">Query</span>
        <input
          type="text"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && handleSave()}
          placeholder="e.g. service:whatsapp is:group"
          className="box-border w-full rounded border border-border bg-elevated px-2 py-1.5 text-[0.875rem] text-text"
        />
      </label>

      <div className="flex justify-end gap-2">
        <Button onClick={onCancel} size="sm">
          Cancel
        </Button>
        <Button
          variant="primary"
          onClick={handleSave}
          disabled={!name.trim() || !query.trim()}
          size="sm"
          className="!px-4"
        >
          Save
        </Button>
      </div>
    </ModalShell>
  );
}
