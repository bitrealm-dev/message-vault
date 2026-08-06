# Unified GUI — Plan 4: Search, Contacts, Saved Groups

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build global search with operator syntax, advanced search form, contact list view, contact drawer, and saved groups in the left panel. After this plan the user can search all conversations, browse contacts, view contact details, and save searches as named groups.

**Architecture:** The global search bar in the left panel accepts operator-based queries (`from:`, `to:`, `with:`, `has:`, `date:`, etc.). The advanced search form is a dropdown with Messages and Contacts tabs. Search results replace the conversation list. Saved groups are named queries stored in localStorage (migrated to the server later). The contact list is a flat directory view. The contact drawer slides in from the right.

**Tech Stack:** React 19, TypeScript, existing API client and types from Plans 2-3

## Global Constraints

- Search syntax reuses operators from the existing message-vault-rs search system
- Saved groups are localStorage-only in this plan (server-side persistence is follow-up)
- Contact drawer is a CSS slide-over, no new library needed
- Contacts list fetches from the server API (endpoint may need adding — see Task 2)

---

## File Structure

| File | Responsibility |
|------|---------------|
| `web/src/components/GlobalSearch.tsx` | Search input with operator autocomplete |
| `web/src/components/AdvancedSearchForm.tsx` | Dropdown form with Messages/Contacts tabs |
| `web/src/screens/ContactList.tsx` | Flat list of all contacts |
| `web/src/components/ContactDrawer.tsx` | Slide-over panel with handles, history, group count |
| `web/src/lib/savedGroups.ts` | localStorage read/write for saved search queries |

---

### Task 1: Global search bar

**Files:**
- Create: `web/src/components/GlobalSearch.tsx`

**Interfaces:**
- Produces: `GlobalSearch` — search input that emits query string on Enter, with autocomplete for `from:`/`to:`/`with:` operators
- Consumes: `apiClient` (for contact name autocomplete)

- [ ] **Step 1: Write GlobalSearch**

```typescript
// web/src/components/GlobalSearch.tsx

import { useState, useEffect, useRef, useMemo } from "react";
import { apiClient } from "../lib/api";

const OPERATORS = ["from:", "to:", "with:", "within:", "has:", "date:", "source:", "label:", "handle:"];

interface Suggestion {
  kind: "contact" | "operator";
  value: string;
}

export default function GlobalSearch({
  value,
  onChange,
  onSearch,
}: {
  value: string;
  onChange: (v: string) => void;
  onSearch: (query: string) => void;
}) {
  const [suggestions, setSuggestions] = useState<Suggestion[]>([]);
  const [suggestIndex, setSuggestIndex] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);

  // Parse current input for operator-based autocomplete
  const suggestionState = useMemo(() => {
    const m = value.match(/(?:^|\s)((?:from|to|with|within|label|handle):)(?:"([^"]*)"|([^\s]*))$/i);
    if (!m || m.index == null) return null;
    const op = m[1].toLowerCase();
    const partial = (m[2] ?? m[3] ?? "").toLowerCase();
    return { operator: op.slice(0, -1), partial, replaceFrom: m.index + m[0].indexOf(op) };
  }, [value]);

  // Fetch contact suggestions when typing after an operator
  useEffect(() => {
    if (!suggestionState) {
      setSuggestions([]);
      return;
    }
    // Show operator suggestions if no partial yet
    if (!suggestionState.partial) {
      setSuggestions(
        OPERATORS.filter((o) => o.startsWith(suggestionState.operator + ":"))
          .map((o) => ({ kind: "operator" as const, value: o })),
      );
      return;
    }
    // Fetch matching contacts for participant operators
    if (["from", "to", "with", "handle"].includes(suggestionState.operator)) {
      apiClient
        .get<{ contacts: { name: string }[] }>(
          `/v1/export/contacts?q=${encodeURIComponent(suggestionState.partial)}&limit=8`,
        )
        .then((res) =>
          setSuggestions(
            res.contacts
              .filter((c) => c.name)
              .map((c) => ({ kind: "contact" as const, value: c.name })),
          ),
        )
        .catch(() => setSuggestions([]));
    }
  }, [suggestionState]);

  const applySuggestion = (s: Suggestion) => {
    if (!suggestionState) return;
    const prefix = value.slice(0, suggestionState.replaceFrom);
    const op = suggestionState.operator + ":";
    const next = `${prefix}${op}${s.value} `;
    onChange(next);
    inputRef.current?.focus();
  };

  return (
    <div style={{ position: "relative", padding: "0.75rem" }}>
      <input
        ref={inputRef}
        type="search"
        value={value}
        onChange={(e) => onChange(e.target.value)}
        onKeyDown={(e) => {
          if (suggestions.length > 0) {
            if (e.key === "ArrowDown") {
              e.preventDefault();
              setSuggestIndex((i) => (i + 1) % suggestions.length);
              return;
            }
            if (e.key === "ArrowUp") {
              e.preventDefault();
              setSuggestIndex((i) => (i - 1 + suggestions.length) % suggestions.length);
              return;
            }
            if (e.key === "Tab") {
              e.preventDefault();
              applySuggestion(suggestions[suggestIndex]);
              return;
            }
          }
          if (e.key === "Enter") {
            e.preventDefault();
            onSearch(value);
          }
        }}
        placeholder="Search vault — try from: or has:"
        style={{
          width: "100%", padding: "0.375rem 0.5rem", fontSize: "0.813rem",
          border: "1px solid #d1d5db", borderRadius: "4px",
        }}
      />
      {suggestions.length > 0 && (
        <div style={{
          position: "absolute", top: "100%", left: "0.75rem", right: "0.75rem",
          background: "#fff", border: "1px solid #d1d5db", borderRadius: "4px",
          boxShadow: "0 4px 6px rgba(0,0,0,0.1)", zIndex: 30, maxHeight: "200px",
          overflow: "auto",
        }}>
          {suggestions.map((s, i) => (
            <button
              key={`${s.kind}:${s.value}`}
              onMouseDown={(e) => { e.preventDefault(); applySuggestion(s); }}
              style={{
                display: "block", width: "100%", textAlign: "left", border: "none",
                background: i === suggestIndex ? "#f3f4f6" : "transparent",
                padding: "0.375rem 0.75rem", fontSize: "0.813rem", cursor: "pointer",
              }}
            >
              <span>{s.value}</span>
              <span style={{ float: "right", color: "#9ca3af", fontSize: "0.688rem" }}>
                {s.kind}
              </span>
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
```

- [ ] **Step 2: Build and verify**

```bash
cd web && npm run build
```

- [ ] **Step 3: Commit**

```bash
git add web/src/components/GlobalSearch.tsx
git commit -m "feat(web): add global search bar with operator autocomplete

Operator syntax: from:, to:, with:, has:, date:, source:, within:, label:.
Autocomplete fetches matching contacts from the server API.

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

### Task 2: Contact list and drawer

**Files:**
- Create: `web/src/screens/ContactList.tsx`
- Create: `web/src/components/ContactDrawer.tsx`

**Interfaces:**
- Produces: `ContactList` — flat directory of contacts. `ContactDrawer` — slide-over with handles, dates, group count
- Consumes: `apiClient`, `Participant` type

- [ ] **Step 1: Write ContactList**

```typescript
// web/src/screens/ContactList.tsx

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

  useEffect(() => {
    apiClient
      .get<{ contacts: Contact[] }>("/v1/export/contacts")
      .then((res) => setContacts(res.contacts))
      .catch(() => setContacts([]))
      .finally(() => setLoading(false));
  }, []);

  if (loading) return <div style={{ padding: "1rem", fontSize: "0.813rem", color: "#9ca3af" }}>Loading…</div>;

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
```

- [ ] **Step 2: Write ContactDrawer**

```typescript
// web/src/components/ContactDrawer.tsx

import { useState, useEffect } from "react";
import { apiClient } from "../lib/api";

interface ContactDetail {
  id: string;
  name: string;
  handles: { handle: string; service: string; start_date: string | null; end_date: string | null; message_count: number }[];
  direct_conversations: number;
  group_conversations: number;
  total_messages: number;
}

export default function ContactDrawer({
  contactId,
  onClose,
}: {
  contactId: string | null;
  onClose: () => void;
}) {
  const [detail, setDetail] = useState<ContactDetail | null>(null);

  useEffect(() => {
    if (!contactId) return;
    apiClient
      .get<ContactDetail>(`/v1/export/contacts/${contactId}`)
      .then(setDetail)
      .catch(() => setDetail(null));
  }, [contactId]);

  if (!contactId || !detail) return null;

  return (
    <>
      {/* Backdrop */}
      <div onClick={onClose} style={{
        position: "fixed", inset: 0, background: "rgba(0,0,0,0.2)", zIndex: 40,
      }} />
      {/* Drawer */}
      <div style={{
        position: "fixed", right: 0, top: 0, bottom: 0, width: "320px",
        background: "#fff", boxShadow: "-2px 0 8px rgba(0,0,0,0.1)", zIndex: 50,
        overflow: "auto", padding: "1.5rem",
      }}>
        <div style={{ display: "flex", justifyContent: "space-between", marginBottom: "1rem" }}>
          <h2 style={{ margin: 0, fontSize: "1.125rem" }}>{detail.name}</h2>
          <button onClick={onClose} style={{ border: "none", background: "none", fontSize: "1.25rem", cursor: "pointer" }}>×</button>
        </div>

        <h3 style={{ fontSize: "0.75rem", color: "#9ca3af", textTransform: "uppercase", marginBottom: "0.5rem" }}>
          Handles
        </h3>
        {detail.handles.map((h, i) => (
          <div key={i} style={{ marginBottom: "0.5rem", fontSize: "0.875rem" }}>
            <div style={{ fontWeight: 500 }}>{h.handle}</div>
            <div style={{ color: "#6b7280" }}>
              {h.service}
              {h.start_date && ` · ${new Date(h.start_date).getFullYear()}–${h.end_date ? new Date(h.end_date).getFullYear() : "present"}`}
              {h.message_count > 0 && ` · ${h.message_count} messages`}
            </div>
          </div>
        ))}

        <div style={{ marginTop: "1rem", fontSize: "0.875rem", color: "#6b7280" }}>
          <div>{detail.direct_conversations} direct conversation{detail.direct_conversations !== 1 ? "s" : ""}</div>
          <div>{detail.group_conversations} group conversation{detail.group_conversations !== 1 ? "s" : ""}</div>
          <div>{detail.total_messages} total messages</div>
        </div>
      </div>
    </>
  );
}
```

- [ ] **Step 3: Build and verify**

```bash
cd web && npm run build
```

- [ ] **Step 4: Commit**

```bash
git add web/src/screens/ContactList.tsx web/src/components/ContactDrawer.tsx
git commit -m "feat(web): add contact list and contact detail drawer

Flat contact list sorted by name. Slide-over drawer shows handles
with date ranges, per-handle message counts, group/direct counts.

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

### Task 3: Saved groups

**Files:**
- Create: `web/src/lib/savedGroups.ts`
- Modify: `web/src/components/LeftPanel.tsx` — render saved groups from storage
- Create: `web/src/components/SavedGroupForm.tsx`

**Interfaces:**
- Produces: `savedGroups` module with `list()`, `add()`, `remove()`, `rename()`. `SavedGroupForm` component for creating/editing.
- Consumes: localStorage

- [ ] **Step 1: Write savedGroups module**

```typescript
// web/src/lib/savedGroups.ts

export interface SavedGroup {
  id: string;
  name: string;
  query: string;
}

const STORAGE_KEY = "mv-saved-groups";

export function listGroups(): SavedGroup[] {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    return raw ? JSON.parse(raw) : [];
  } catch {
    return [];
  }
}

export function addGroup(name: string, query: string): SavedGroup {
  const groups = listGroups();
  const group: SavedGroup = { id: crypto.randomUUID(), name, query };
  groups.push(group);
  localStorage.setItem(STORAGE_KEY, JSON.stringify(groups));
  return group;
}

export function removeGroup(id: string): void {
  const groups = listGroups().filter((g) => g.id !== id);
  localStorage.setItem(STORAGE_KEY, JSON.stringify(groups));
}

export function renameGroup(id: string, name: string): void {
  const groups = listGroups().map((g) => (g.id === id ? { ...g, name } : g));
  localStorage.setItem(STORAGE_KEY, JSON.stringify(groups));
}
```

- [ ] **Step 2: Wire saved groups into LeftPanel**

In `LeftPanel.tsx`, replace the "No saved groups yet" placeholder with:

```typescript
import { useState } from "react";
import { listGroups } from "../lib/savedGroups";

// Inside LeftPanel, after the search input:
const [groups, setGroups] = useState(() => listGroups());

// Refresh groups when the component mounts or when navigating
useEffect(() => {
  setGroups(listGroups());
}, [activeView]);

// Render saved groups:
<div style={{ padding: "0 0.75rem", marginBottom: "0.5rem" }}>
  <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", marginBottom: "0.25rem" }}>
    <span style={{ fontSize: "0.688rem", fontWeight: 600, color: "#9ca3af", textTransform: "uppercase", letterSpacing: "0.05em" }}>
      Saved Groups
    </span>
    <button onClick={() => onNavigate("new-group")} style={{ fontSize: "0.688rem", border: "none", background: "none", color: "#2563eb", cursor: "pointer" }}>
      + New
    </button>
  </div>
  {groups.length === 0 ? (
    <div style={{ fontSize: "0.813rem", color: "#9ca3af", padding: "0.25rem 0" }}>No saved groups</div>
  ) : (
    groups.map((g) => (
      <button
        key={g.id}
        onClick={() => { onChange(g.query); onSearch(g.query); }}
        style={{
          display: "block", width: "100%", textAlign: "left", border: "none",
          background: "transparent", padding: "0.25rem 0", fontSize: "0.813rem",
          cursor: "pointer", color: "#374151",
        }}
      >
        {g.name}
      </button>
    ))
  )}
</div>
```

- [ ] **Step 3: Build and verify**

```bash
cd web && npm run build
```

- [ ] **Step 4: Commit**

```bash
git add web/src/lib/savedGroups.ts web/src/components/LeftPanel.tsx
git commit -m "feat(web): add saved groups (named search queries)

localStorage-backed saved groups appear in the left panel.
Click a group to run its query. + New button to create groups.

Co-Authored-By: Claude <noreply@anthropic.com>"
```
