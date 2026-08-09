import { useState, useEffect } from "react";
import { apiClient } from "../lib/api";

interface Contact {
  id: string;
  name: string;
  handle_count: number;
  last_message_at: string | null;
}

export default function ContactList({ onSelect }: { onSelect: (contact: Contact) => void }) {
  const [contacts, setContacts] = useState<Contact[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");

  useEffect(() => {
    setLoading(true);
    setError("");
    apiClient
      .get<{ contacts: Contact[] }>("/v1/export/contacts")
      .then((res) =>
        setContacts(
          (res.contacts || []).map((c) => ({
            ...c,
            id: String(c.id),
          })),
        ),
      )
      .catch((e) => {
        setContacts([]);
        setError(e instanceof Error ? e.message : String(e));
      })
      .finally(() => setLoading(false));
  }, []);

  if (loading) return <div style={{ padding: "1rem", fontSize: "0.813rem", color: "#9ca3af" }}>Loading…</div>;
  if (error) {
    return (
      <div style={{ padding: "1rem", fontSize: "0.813rem", color: "#dc2626" }}>
        Could not load contacts: {error}
      </div>
    );
  }
  if (contacts.length === 0) {
    return <div style={{ padding: "1rem", fontSize: "0.813rem", color: "#9ca3af" }}>No contacts</div>;
  }

  return (
    <div style={{ overflow: "auto" }}>
      {contacts.map((c) => (
        <button
          key={c.id}
          onClick={() => onSelect(c)}
          style={{
            display: "flex", justifyContent: "space-between", width: "100%",
            textAlign: "left", border: "none", background: "transparent",
            padding: "0.5rem 0.75rem", cursor: "pointer",
            borderBottom: "1px solid #f3f4f6",
          }}
        >
          <div>
            <div style={{ fontSize: "0.875rem", fontWeight: 500 }}>{c.name}</div>
            <div style={{ fontSize: "0.75rem", color: "#6b7280" }}>
              {c.handle_count} handle{c.handle_count !== 1 ? "s" : ""}
            </div>
          </div>
          {c.last_message_at && (
            <div style={{ fontSize: "0.75rem", color: "#9ca3af", flexShrink: 0 }}>
              {new Date(c.last_message_at).toLocaleDateString()}
            </div>
          )}
        </button>
      ))}
    </div>
  );
}
