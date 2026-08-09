import { useState, useEffect, useMemo } from "react";
import { apiClient } from "../lib/api";

interface Contact {
  id: string;
  name: string;
  handle_count: number;
  handles?: string[];
  last_message_at: string | null;
}

/** Strip advanced-search tokens; keep plain text + handle:"…" values for filtering. */
function filterNeedle(raw: string): { text: string; handle: string | null } {
  let q = raw.trim();
  if (!q) return { text: "", handle: null };

  let handle: string | null = null;
  const quoted = q.match(/\bhandle:"([^"]+)"/i);
  const bare = q.match(/\bhandle:(\S+)/i);
  if (quoted) {
    handle = quoted[1];
    q = q.replace(quoted[0], " ");
  } else if (bare) {
    handle = bare[1].replace(/^"|"$/g, "");
    q = q.replace(bare[0], " ");
  }

  q = q
    .replace(/\bsearch:contacts\b/gi, " ")
    .replace(/\b(first-contact|last-contact|message-count|group-count):\S+/gi, " ")
    .replace(/\s+/g, " ")
    .trim();

  return { text: q, handle };
}

function contactMatches(c: Contact, filter: string): boolean {
  const { text, handle } = filterNeedle(filter);
  if (!text && !handle) return true;

  const name = c.name.toLowerCase();
  const handles = (c.handles ?? []).map((h) => h.toLowerCase());

  if (handle) {
    const h = handle.toLowerCase();
    if (!handles.some((x) => x.includes(h))) return false;
  }

  if (text) {
    const needle = text.toLowerCase();
    if (name.includes(needle)) return true;
    if (handles.some((x) => x.includes(needle))) return true;
    return false;
  }

  return true;
}

export default function ContactList({
  filter = "",
  onSelect,
}: {
  filter?: string;
  onSelect: (contact: Contact) => void;
}) {
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
            handles: c.handles ?? [],
          })),
        ),
      )
      .catch((e) => {
        setContacts([]);
        setError(e instanceof Error ? e.message : String(e));
      })
      .finally(() => setLoading(false));
  }, []);

  const visible = useMemo(
    () => contacts.filter((c) => contactMatches(c, filter)),
    [contacts, filter],
  );

  if (loading) return <div style={{ padding: "1rem", fontSize: "0.813rem", color: "var(--muted)" }}>Loading…</div>;
  if (error) {
    return (
      <div style={{ padding: "1rem", fontSize: "0.813rem", color: "var(--danger)" }}>
        Could not load contacts: {error}
      </div>
    );
  }
  if (contacts.length === 0) {
    return <div style={{ padding: "1rem", fontSize: "0.813rem", color: "var(--muted)" }}>No contacts</div>;
  }
  if (visible.length === 0) {
    return (
      <div style={{ padding: "1rem", fontSize: "0.813rem", color: "var(--muted)" }}>
        No contacts match this filter
      </div>
    );
  }

  return (
    <div style={{ overflow: "auto" }}>
      {visible.map((c) => (
        <button
          key={c.id}
          onClick={() => onSelect(c)}
          style={{
            display: "flex", justifyContent: "space-between", width: "100%",
            textAlign: "left", border: "none", background: "transparent",
            padding: "0.5rem 0.75rem", cursor: "pointer",
            borderBottom: "1px solid var(--border)",
          }}
        >
          <div>
            <div style={{ fontSize: "0.875rem", fontWeight: 500 }}>{c.name}</div>
            <div style={{ fontSize: "0.75rem", color: "var(--muted)" }}>
              {c.handle_count} handle{c.handle_count !== 1 ? "s" : ""}
            </div>
          </div>
          {c.last_message_at && (
            <div style={{ fontSize: "0.75rem", color: "var(--muted)", flexShrink: 0 }}>
              {new Date(c.last_message_at).toLocaleDateString()}
            </div>
          )}
        </button>
      ))}
    </div>
  );
}
