import { useState } from "react";

interface SavedGroupFormProps {
  onSave: (name: string, query: string) => void;
  onCancel: () => void;
  initial?: { name: string; query: string };
}

export default function SavedGroupForm({ onSave, onCancel, initial }: SavedGroupFormProps) {
  const [name, setName] = useState(initial?.name || "");
  const [query, setQuery] = useState(initial?.query || "");

  const handleSave = () => {
    if (!name.trim() || !query.trim()) return;
    onSave(name.trim(), query.trim());
  };

  return (
    <div style={{
      position: "fixed", inset: 0, display: "flex", alignItems: "center",
      justifyContent: "center", zIndex: 100,
    }}>
      <div onClick={onCancel} style={{
        position: "absolute", inset: 0, background: "rgba(0,0,0,0.3)",
      }} />
      <div style={{
        position: "relative", background: "#fff", borderRadius: "8px",
        padding: "1.5rem", width: "100%", maxWidth: "400px",
        boxShadow: "0 4px 12px rgba(0,0,0,0.15)",
      }}>
        <h3 style={{ margin: "0 0 1rem", fontSize: "1rem" }}>
          {initial ? "Edit saved group" : "New saved group"}
        </h3>

        <label style={{ fontSize: "0.813rem", fontWeight: 500, display: "block", marginBottom: "0.25rem" }}>
          Name
        </label>
        <input
          type="text"
          value={name}
          onChange={(e) => setName(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && handleSave()}
          placeholder="e.g. Work team"
          style={{
            width: "100%", padding: "0.375rem 0.5rem", fontSize: "0.875rem",
            border: "1px solid #d1d5db", borderRadius: "4px", marginBottom: "0.75rem",
            boxSizing: "border-box",
          }}
          autoFocus
        />

        <label style={{ fontSize: "0.813rem", fontWeight: 500, display: "block", marginBottom: "0.25rem" }}>
          Query
        </label>
        <input
          type="text"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && handleSave()}
          placeholder="e.g. from:bob service:discord"
          style={{
            width: "100%", padding: "0.375rem 0.5rem", fontSize: "0.875rem",
            border: "1px solid #d1d5db", borderRadius: "4px", marginBottom: "1rem",
            boxSizing: "border-box",
          }}
        />

        <div style={{ display: "flex", gap: "0.5rem", justifyContent: "flex-end" }}>
          <button onClick={onCancel}
            style={{ padding: "0.375rem 0.75rem", fontSize: "0.875rem", border: "1px solid #d1d5db", background: "#fff", borderRadius: "4px", cursor: "pointer" }}>
            Cancel
          </button>
          <button onClick={handleSave}
            disabled={!name.trim() || !query.trim()}
            style={{ padding: "0.375rem 1rem", fontSize: "0.875rem", fontWeight: 600, cursor: "pointer" }}>
            Save
          </button>
        </div>
      </div>
    </div>
  );
}
