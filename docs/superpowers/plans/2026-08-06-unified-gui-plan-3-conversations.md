# Unified GUI — Plan 3: Conversations and Messages

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the conversation list (left panel) and paginated message view (main area). After this plan the user can browse conversations, open one, and page through messages with a find bar.

**Architecture:** Conversation list fetches from `/v1/export/conversations` (or equivalent endpoint — adapt to the actual API). Message view fetches pages via offset/limit from `/v1/export/messages`. A `Conversation` type defines the shared shape used by both components. Pagination controls show "Messages 1–50 of 1,423" with prev/next buttons.

**Tech Stack:** React 19, TypeScript, existing API client from Plan 2

## Global Constraints

- All data fetching uses `apiClient` from `web/src/lib/api.ts`
- Conversation and message types defined in a shared types file
- Pagination matches the Fastmail-style offset/limit pattern from Plan 1
- Conversation display logic: direct = name + handle + service, small group = all names, large group = count + date range

---

## File Structure

| File | Responsibility |
|------|---------------|
| `web/src/lib/types.ts` | `Conversation`, `Message`, `Participant` types |
| `web/src/screens/ConversationList.tsx` | Flat list of conversations, sort by newest |
| `web/src/screens/MessageView.tsx` | Header + paginated messages + find bar |
| `web/src/components/ConversationRow.tsx` | Single conversation row in the list |
| `web/src/components/MessageBubble.tsx` | Single message bubble with timestamp |
| `web/src/components/PaginationBar.tsx` | "Messages 1–50 of 1,423" with prev/next |
| `web/src/App.tsx` | Wire ConversationList into left panel, MessageView into main area |

---

### Task 1: Shared types

**Files:**
- Modify: `web/src/lib/types.ts` — replace existing types with conversation/message types

**Interfaces:**
- Produces: `Conversation`, `Message`, `Participant` types used by all subsequent tasks

- [ ] **Step 1: Write types**

```typescript
// web/src/lib/types.ts

export interface Participant {
  name: string | null;
  handle: string;
  service: string;
}

export interface Conversation {
  id: string;
  participants: Participant[];
  message_count: number;
  last_message_at: string;
  date_range_start: string | null;
  date_range_end: string | null;
  service: string;
  is_group: boolean;
  label: string | null; // user-assigned local rename
}

export interface Message {
  id: string;
  conversation_id: string;
  sender: Participant;
  body: string;
  sent_at: string;
  service: string;
  attachments: Attachment[];
  reply_to: string | null;
  is_deleted: boolean;
}

export interface Attachment {
  sha256: string;
  filename: string;
  mime_type: string;
  size_bytes: number;
}

export interface PaginatedMessages {
  messages: Message[];
  total: number;
  offset: number;
  limit: number;
}

// Keep existing types used by old screens
export interface ExtractConfig {
  source: string;
  path: string;
  output_dir: string;
}

export interface ExtractErrorEvent {
  detail: string;
  user_message?: string;
}
```

- [ ] **Step 2: Build and verify**

```bash
cd web && npm run build
```

Expected: existing screens may break temporarily — that's fine. Fix any type errors in old screens by importing from the correct location.

- [ ] **Step 3: Commit**

```bash
git add web/src/lib/types.ts
git commit -m "feat(web): add Conversation, Message, Participant types

Shared types for the unified GUI. Replaces old extract-specific types.

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

### Task 2: Conversation list

**Files:**
- Create: `web/src/screens/ConversationList.tsx`
- Create: `web/src/components/ConversationRow.tsx`

**Interfaces:**
- Produces: `ConversationList` component — fetches conversations, renders rows, emits onSelect
- Consumes: `apiClient`, `Conversation` type

- [ ] **Step 1: Write ConversationRow**

```typescript
// web/src/components/ConversationRow.tsx

import type { Conversation } from "../lib/types";

function formatDate(iso: string): string {
  const d = new Date(iso);
  const now = new Date();
  const diffDays = Math.floor((now.getTime() - d.getTime()) / 86400000);

  if (diffDays === 0) {
    return d.toLocaleTimeString([], { hour: "numeric", minute: "2-digit" });
  }
  if (diffDays === 1) return "yesterday";
  if (diffDays < 7) return `${diffDays}d ago`;
  if (d.getFullYear() === now.getFullYear()) {
    return d.toLocaleDateString([], { month: "short", day: "numeric" });
  }
  return d.toLocaleDateString([], { month: "short", day: "numeric", year: "numeric" });
}

function displayName(conv: Conversation): string {
  if (conv.label) return conv.label;

  if (!conv.is_group) {
    const p = conv.participants[0];
    return p?.name || p?.handle || "(unknown)";
  }

  // Small group: show names
  if (conv.participants.length <= 7) {
    return conv.participants.map((p) => p.name || p.handle).join(", ");
  }

  // Large group: count + date range
  const parts = [`${conv.participants.length} participants`];
  if (conv.date_range_start && conv.date_range_end) {
    parts.push(formatDateRange(conv.date_range_start, conv.date_range_end));
  }
  return parts.join(" · ");
}

function formatDateRange(start: string, end: string): string {
  const s = new Date(start);
  const e = new Date(end);
  const fmt = (d: Date) =>
    d.toLocaleDateString([], { month: "short", year: "numeric" });
  return `${fmt(s)} – ${fmt(e)}`;
}

function subtitle(conv: Conversation): string {
  const parts: string[] = [];
  if (conv.is_group) {
    parts.push(`${conv.message_count} msgs`);
  } else {
    const p = conv.participants[0];
    if (p) parts.push(`${p.handle} · ${p.service}`);
  }
  return parts.join(" · ");
}

export default function ConversationRow({
  conversation,
  isSelected,
  onClick,
}: {
  conversation: Conversation;
  isSelected: boolean;
  onClick: () => void;
}) {
  return (
    <button
      onClick={onClick}
      style={{
        display: "block", width: "100%", textAlign: "left", border: "none",
        background: isSelected ? "#e5e7eb" : "transparent",
        padding: "0.5rem 0.75rem", cursor: "pointer",
        borderBottom: "1px solid #f3f4f6",
      }}
    >
      <div style={{
        display: "flex", justifyContent: "space-between", alignItems: "baseline",
        marginBottom: "2px",
      }}>
        <span style={{
          fontSize: "0.875rem", fontWeight: 500, color: "#1f2937",
          overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap",
          flex: 1, marginRight: "0.5rem",
        }}>
          {displayName(conversation)}
        </span>
        <span style={{ fontSize: "0.75rem", color: "#9ca3af", flexShrink: 0 }}>
          {formatDate(conversation.last_message_at)}
        </span>
      </div>
      <div style={{ fontSize: "0.75rem", color: "#6b7280" }}>
        {subtitle(conversation)}
      </div>
    </button>
  );
}
```

- [ ] **Step 2: Write ConversationList**

```typescript
// web/src/screens/ConversationList.tsx

import { useState, useEffect } from "react";
import { apiClient } from "../lib/api";
import type { Conversation } from "../lib/types";
import ConversationRow from "../components/ConversationRow";

export default function ConversationList({
  selectedId,
  onSelect,
  query,
}: {
  selectedId: string | null;
  onSelect: (conversation: Conversation) => void;
  query: string;
}) {
  const [conversations, setConversations] = useState<Conversation[]>([]);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    setLoading(true);
    apiClient
      .get<{ conversations: Conversation[] }>(
        `/v1/export/conversations?q=${encodeURIComponent(query)}`,
      )
      .then((res) => setConversations(res.conversations))
      .catch(() => setConversations([]))
      .finally(() => setLoading(false));
  }, [query]);

  if (loading) {
    return <div style={{ padding: "1rem", fontSize: "0.813rem", color: "#9ca3af" }}>Loading…</div>;
  }

  if (conversations.length === 0) {
    return <div style={{ padding: "1rem", fontSize: "0.813rem", color: "#9ca3af" }}>No conversations</div>;
  }

  return (
    <div style={{ overflow: "auto", flex: 1 }}>
      {conversations.map((c) => (
        <ConversationRow
          key={c.id}
          conversation={c}
          isSelected={c.id === selectedId}
          onClick={() => onSelect(c)}
        />
      ))}
    </div>
  );
}
```

- [ ] **Step 3: Build and verify**

```bash
cd web && npm run build
```

Expected: compiles cleanly.

- [ ] **Step 4: Commit**

```bash
git add web/src/screens/ConversationList.tsx web/src/components/ConversationRow.tsx
git commit -m "feat(web): add conversation list with display logic

Flat list sorted by most recent message. Display logic: direct = name +
handle, small group = all names, large group = count + date range.
Supports local rename via label field.

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

### Task 3: Paginated message view

**Files:**
- Create: `web/src/screens/MessageView.tsx`
- Create: `web/src/components/MessageBubble.tsx`
- Create: `web/src/components/PaginationBar.tsx`

**Interfaces:**
- Produces: `MessageView` — header with participant chips + paginated messages + find bar
- Consumes: `apiClient`, `Conversation`, `Message`, `PaginatedMessages` types

- [ ] **Step 1: Write MessageBubble**

```typescript
// web/src/components/MessageBubble.tsx

import type { Message } from "../lib/types";

export default function MessageBubble({ message }: { message: Message }) {
  const time = new Date(message.sent_at).toLocaleString([], {
    month: "short", day: "numeric", year: "numeric",
    hour: "numeric", minute: "2-digit",
  });

  return (
    <div style={{ padding: "0.5rem 1.5rem", borderBottom: "1px solid #f3f4f6" }}>
      <div style={{ display: "flex", gap: "0.5rem", marginBottom: "0.25rem" }}>
        <span style={{ fontSize: "0.75rem", fontWeight: 600, color: "#374151" }}>
          {message.sender.name || message.sender.handle}
        </span>
        <span style={{ fontSize: "0.75rem", color: "#9ca3af" }}>{time}</span>
      </div>
      <div style={{ fontSize: "0.875rem", color: "#1f2937", lineHeight: 1.5, whiteSpace: "pre-wrap" }}>
        {message.body}
      </div>
    </div>
  );
}
```

- [ ] **Step 2: Write PaginationBar**

```typescript
// web/src/components/PaginationBar.tsx

export default function PaginationBar({
  offset,
  limit,
  total,
  onPrev,
  onNext,
}: {
  offset: number;
  limit: number;
  total: number;
  onPrev: () => void;
  onNext: () => void;
}) {
  const start = total === 0 ? 0 : offset + 1;
  const end = Math.min(offset + limit, total);

  return (
    <div style={{
      display: "flex", alignItems: "center", justifyContent: "center",
      gap: "1rem", padding: "0.5rem", borderTop: "1px solid #e5e7eb",
      fontSize: "0.813rem", color: "#6b7280",
    }}>
      <button onClick={onPrev} disabled={offset === 0}
        style={{ padding: "0.25rem 0.75rem", fontSize: "0.813rem" }}>
        Previous
      </button>
      <span>
        Messages {start}–{end} of {total}
      </span>
      <button onClick={onNext} disabled={offset + limit >= total}
        style={{ padding: "0.25rem 0.75rem", fontSize: "0.813rem" }}>
        Next
      </button>
    </div>
  );
}
```

- [ ] **Step 3: Write MessageView**

```typescript
// web/src/screens/MessageView.tsx

import { useState, useEffect, useCallback } from "react";
import { apiClient } from "../lib/api";
import type { Conversation, Message } from "../lib/types";
import MessageBubble from "../components/MessageBubble";
import PaginationBar from "../components/PaginationBar";

const PAGE_SIZE = 50;

export default function MessageView({ conversation }: { conversation: Conversation }) {
  const [messages, setMessages] = useState<Message[]>([]);
  const [total, setTotal] = useState(0);
  const [offset, setOffset] = useState(0);
  const [findTerm, setFindTerm] = useState("");
  const [loading, setLoading] = useState(false);

  const fetchPage = useCallback(
    async (newOffset: number, searchTerm?: string) => {
      setLoading(true);
      try {
        const q = searchTerm
          ? `conversation:${conversation.id} ${searchTerm}`
          : `conversation:${conversation.id}`;
        const res = await apiClient.get<{ messages: Message[]; total: number }>(
          `/v1/export/messages?q=${encodeURIComponent(q)}&offset=${newOffset}&limit=${PAGE_SIZE}`,
        );
        setMessages(res.messages);
        setTotal(res.total);
        setOffset(newOffset);
      } catch {
        setMessages([]);
        setTotal(0);
      } finally {
        setLoading(false);
      }
    },
    [conversation.id],
  );

  useEffect(() => {
    fetchPage(0);
  }, [fetchPage]);

  const handleSearch = () => {
    fetchPage(0, findTerm);
  };

  return (
    <div style={{ display: "flex", flexDirection: "column", height: "100%" }}>
      {/* Header */}
      <div style={{
        padding: "0.75rem 1.5rem", borderBottom: "1px solid #e5e7eb",
        background: "#fafafa",
      }}>
        <div style={{ fontSize: "1rem", fontWeight: 600, marginBottom: "0.25rem" }}>
          {conversation.label ||
            (conversation.is_group
              ? `${conversation.participants.length} participants`
              : conversation.participants[0]?.name || conversation.participants[0]?.handle)}
        </div>
        <div style={{ display: "flex", gap: "1rem", fontSize: "0.75rem", color: "#6b7280" }}>
          <span>{conversation.participants.map((p) => p.name || p.handle).join(", ")}</span>
          {conversation.date_range_start && conversation.date_range_end && (
            <span>
              {new Date(conversation.date_range_start).toLocaleDateString([], { month: "short", year: "numeric" })} –{" "}
              {new Date(conversation.date_range_end).toLocaleDateString([], { month: "short", year: "numeric" })}
            </span>
          )}
          <span>{conversation.message_count} messages</span>
        </div>
      </div>

      {/* Find bar */}
      <div style={{
        padding: "0.375rem 1.5rem", borderBottom: "1px solid #e5e7eb",
        display: "flex", gap: "0.5rem", alignItems: "center",
      }}>
        <input
          type="text"
          value={findTerm}
          onChange={(e) => setFindTerm(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && handleSearch()}
          placeholder="Find in conversation…"
          style={{
            flex: 1, padding: "0.25rem 0.5rem", fontSize: "0.813rem",
            border: "1px solid #d1d5db", borderRadius: "4px",
          }}
        />
        <button onClick={handleSearch} style={{ padding: "0.25rem 0.75rem", fontSize: "0.813rem" }}>
          Find
        </button>
      </div>

      {/* Messages */}
      <div style={{ flex: 1, overflow: "auto" }}>
        {loading ? (
          <div style={{ padding: "1rem", fontSize: "0.813rem", color: "#9ca3af" }}>
            Loading…
          </div>
        ) : (
          messages.map((m) => <MessageBubble key={m.id} message={m} />)
        )}
      </div>

      {/* Pagination */}
      <PaginationBar
        offset={offset}
        limit={PAGE_SIZE}
        total={total}
        onPrev={() => fetchPage(Math.max(0, offset - PAGE_SIZE))}
        onNext={() => fetchPage(offset + PAGE_SIZE)}
      />
    </div>
  );
}
```

- [ ] **Step 4: Build and verify**

```bash
cd web && npm run build
```

Expected: compiles cleanly.

- [ ] **Step 5: Commit**

```bash
git add web/src/screens/MessageView.tsx web/src/components/MessageBubble.tsx web/src/components/PaginationBar.tsx
git commit -m "feat(web): add paginated message view with find bar

Conversation header with participants, date range, message count.
Server-side pagination with 'Messages 1-50 of 1,423' controls.
Find bar searches within the current conversation.

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

### Task 4: Wire into App

**Files:**
- Modify: `web/src/App.tsx` — wire ConversationList into left panel, MessageView into main area

**Interfaces:**
- Consumes: `ConversationList`, `MessageView`, `AppLayout`

- [ ] **Step 1: Update AppLayout to pass conversation state**

```typescript
// web/src/components/AppLayout.tsx

import { useState, type ReactNode } from "react";
import LeftPanel from "./LeftPanel";
import ConversationList from "../screens/ConversationList";
import type { Conversation } from "../lib/types";

export default function AppLayout({ children }: { children: ReactNode }) {
  const [activeView, setActiveView] = useState("conversations");
  const [selectedConversation, setSelectedConversation] = useState<Conversation | null>(null);

  // Left panel shows the conversation list when in conversations view
  const leftContent =
    activeView === "conversations" || activeView === "contacts" || activeView === "trash" ? (
      <div style={{ display: "flex", flexDirection: "column", flex: 1, overflow: "hidden" }}>
        <ConversationList
          selectedId={selectedConversation?.id || null}
          onSelect={(c) => {
            setSelectedConversation(c);
            setActiveView("conversations");
          }}
          query={activeView === "trash" ? "is:trash" : ""}
        />
      </div>
    ) : null;

  return (
    <div style={{ display: "flex", height: "100vh", fontFamily: "system-ui" }}>
      <LeftPanel
        activeView={activeView}
        onNavigate={setActiveView}
        conversationList={leftContent}
      />
      <main style={{ flex: 1, overflow: "auto", background: "#fff" }}>
        {selectedConversation && activeView === "conversations" ? (
          <MessageView conversation={selectedConversation} />
        ) : (
          children
        )}
      </main>
    </div>
  );
}
```

- [ ] **Step 2: Update LeftPanel to accept conversationList prop**

Add `conversationList?: ReactNode` to LeftPanel's props and render it between the search bar and the nav links:

```typescript
// In LeftPanel, after the saved groups section, add:
{conversationList && (
  <div style={{ flex: 1, overflow: "hidden", display: "flex", flexDirection: "column" }}>
    {conversationList}
  </div>
)}
```

- [ ] **Step 3: Build and verify**

```bash
cd web && npm run build
```

Expected: compiles cleanly. The app now shows the conversation list in the left panel and messages in the main area when a conversation is selected.

- [ ] **Step 4: Commit**

```bash
git add web/src/App.tsx web/src/components/AppLayout.tsx web/src/components/LeftPanel.tsx
git commit -m "feat(web): wire conversation list and message view into layout

Left panel hosts conversation list between search and nav. Main area
shows MessageView when a conversation is selected.

Co-Authored-By: Claude <noreply@anthropic.com>"
```
