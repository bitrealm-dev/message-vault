# Tier 3 — Spec Features With No Existing Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the nine spec features from the unified GUI design that have no corresponding implementation plan: service-specific message rendering, attachment viewer, inline name editing, handle matching, sources panel, onboarding, search results view, date jump links, and local rename.

**Architecture:** These features build on the existing conversation-centric data model and API. Most are pure frontend work in `message-vault-io/web/src/`. A few (handle matching, sources panel) require new or augmented API endpoints in `message-vault-rs`. The plan is ordered by dependency — each task builds on earlier ones where applicable.

**Tech Stack:** React 19, TypeScript, Vite (frontend), Rust/axum (backend API additions)

## Global Constraints

- All frontend changes go in `message-vault-io/web/src/`
- All backend changes go in `message-vault-rs/src/`
- The `Conversation` and `Message` types in `web/src/lib/types.ts` are the canonical shapes — API responses must match
- Service-specific rendering is progressive enhancement — the base MessageBubble fallback (plain text) is already in place
- Handle matching and sources panel require read-only API queries (no new write endpoints)
- Onboarding must work in both Tauri desktop and web deployment
- The `label` field on `Conversation` (for local rename) already exists in the type — just needs UI

---

## File Structure

| File | Responsibility |
|------|---------------|
| `web/src/components/MessageBubble.tsx` | Delegate to service renderer or fallback |
| `web/src/components/messages/SmsBubble.tsx` | SMS/MMS bubble (base renderer) |
| `web/src/components/messages/ImessageBubble.tsx` | iMessage reactions, tapbacks, effects |
| `web/src/components/messages/WhatsAppBubble.tsx` | WhatsApp reply chains, deleted indicators |
| `web/src/components/messages/DiscordBubble.tsx` | Discord embeds, role colors |
| `web/src/components/AttachmentThumbnail.tsx` | Inline image/video thumbnail |
| `web/src/components/AttachmentLightbox.tsx` | Full-screen image viewer with nav |
| `web/src/components/VideoPlayer.tsx` | Inline video player with controls |
| `web/src/components/ContactDrawer.tsx` | Add inline name editing |
| `web/src/screens/OnboardingScreen.tsx` | Post-registration profile setup |
| `web/src/components/SourcesPanel.tsx` | Backup provenance slide-out |
| `web/src/screens/SearchResults.tsx` | Grouped search results view |
| `web/src/components/ConversationRow.tsx` | Add inline rename on click |
| `web/src/components/MessageView.tsx` | Add date jump links to header |
| `web/src/lib/types.ts` | Add service-specific message fields |
| `web/src/App.tsx` | Wire onboarding after registration |
| `message-vault-rs/src/server.rs` | Add handle-matching + sources endpoints |

---

### Task 1: Extend Message type with service-specific fields

**Files:**
- Modify: `message-vault-io/web/src/lib/types.ts`

**Goal:** Add optional service-specific fields to the `Message` type so renderers can access reactions, embeds, reply chains, etc. These fields are populated by the existing vault API when the data is available.

Add to the `Message` interface:

```typescript
export interface Message {
  // ... existing fields ...
  
  // Service-specific data (optional — populated when available)
  reactions?: Reaction[];        // iMessage tapbacks, Discord reactions
  reply_to_message?: MessageRef; // WhatsApp reply chains
  embeds?: Embed[];              // Discord embeds
  edit_history?: EditEntry[];    // iMessage edit history
  deleted_indicator?: boolean;   // WhatsApp "this message was deleted"
  effect?: string;               // iMessage screen effect
  role_color?: string;           // Discord role color
  is_story_reply?: boolean;      // Instagram story reply
  forwarded?: boolean;           // Instagram forwarding indicator
}

export interface Reaction {
  emoji: string;
  count: number;
  users: string[];  // display names
}

export interface MessageRef {
  id: string;
  sender_name: string;
  body_preview: string;
}

export interface Embed {
  type: "image" | "video" | "link" | "rich";
  url?: string;
  title?: string;
  description?: string;
  thumbnail_url?: string;
}

export interface EditEntry {
  body: string;
  edited_at: string;
}
```

These are all optional — the existing plain-text rendering continues to work when they're absent.

- [ ] **Step 1: Add the types**
- [ ] **Step 2: Build and verify** (`cd web && npm run build`)
- [ ] **Step 3: Commit**

---

### Task 2: Service-specific message renderers

**Files:**
- Modify: `web/src/components/MessageBubble.tsx`
- Create: `web/src/components/messages/SmsBubble.tsx`
- Create: `web/src/components/messages/ImessageBubble.tsx`
- Create: `web/src/components/messages/DiscordBubble.tsx`

**Goal:** `MessageBubble` becomes a dispatcher — it reads `message.service` and delegates to the appropriate renderer. Unknown services fall back to `SmsBubble` (plain text). Start with 3 services; WhatsApp and Instagram follow in Task 8.

#### SmsBubble.tsx (base/fallback renderer)

```typescript
import type { Message } from "../../lib/types";

export default function SmsBubble({ message }: { message: Message }) {
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
        {message.service && (
          <span style={{ fontSize: "0.688rem", color: "#d1d5db", textTransform: "uppercase" }}>
            {message.service}
          </span>
        )}
      </div>
      <div style={{ fontSize: "0.875rem", color: "#1f2937", lineHeight: 1.5, whiteSpace: "pre-wrap" }}>
        {message.body}
      </div>
    </div>
  );
}
```

#### ImessageBubble.tsx

```typescript
import type { Message } from "../../lib/types";

export default function ImessageBubble({ message }: { message: Message }) {
  const time = new Date(message.sent_at).toLocaleString([], {
    month: "short", day: "numeric", hour: "numeric", minute: "2-digit",
  });

  return (
    <div style={{ padding: "0.5rem 1.5rem", borderBottom: "1px solid #f3f4f6" }}>
      <div style={{ display: "flex", gap: "0.5rem", marginBottom: "0.25rem" }}>
        <span style={{ fontSize: "0.75rem", fontWeight: 600, color: "#007aff" }}>
          {message.sender.name || message.sender.handle}
        </span>
        <span style={{ fontSize: "0.75rem", color: "#9ca3af" }}>{time}</span>
        {message.effect && (
          <span style={{ fontSize: "0.688rem", color: "#8b5cf6", fontStyle: "italic" }}>
            {message.effect}
          </span>
        )}
      </div>
      
      {/* Edit history indicator */}
      {message.edit_history && message.edit_history.length > 0 && (
        <div style={{ fontSize: "0.688rem", color: "#9ca3af", fontStyle: "italic", marginBottom: "0.25rem" }}>
          Edited
        </div>
      )}
      
      <div style={{ fontSize: "0.875rem", color: "#1f2937", lineHeight: 1.5, whiteSpace: "pre-wrap" }}>
        {message.body}
      </div>

      {/* Tapback reactions */}
      {message.reactions && message.reactions.length > 0 && (
        <div style={{ display: "flex", gap: "0.375rem", marginTop: "0.25rem" }}>
          {message.reactions.map((r, i) => (
            <span key={i} style={{
              fontSize: "0.75rem", background: "#f3f4f6",
              padding: "0.125rem 0.375rem", borderRadius: "4px",
            }}>
              {r.emoji} {r.count}
            </span>
          ))}
        </div>
      )}
    </div>
  );
}
```

#### DiscordBubble.tsx

```typescript
import type { Message } from "../../lib/types";

export default function DiscordBubble({ message }: { message: Message }) {
  const time = new Date(message.sent_at).toLocaleString([], {
    month: "short", day: "numeric", hour: "numeric", minute: "2-digit",
  });

  return (
    <div style={{ padding: "0.5rem 1.5rem", borderBottom: "1px solid #f3f4f6" }}>
      <div style={{ display: "flex", gap: "0.5rem", alignItems: "center", marginBottom: "0.25rem" }}>
        <span style={{
          fontSize: "0.75rem", fontWeight: 600,
          color: message.role_color || "#5865f2",
        }}>
          {message.sender.name || message.sender.handle}
        </span>
        <span style={{ fontSize: "0.688rem", color: "#9ca3af" }}>{time}</span>
      </div>
      
      <div style={{ fontSize: "0.875rem", color: "#1f2937", lineHeight: 1.5, whiteSpace: "pre-wrap" }}>
        {message.body}
      </div>

      {/* Embeds */}
      {message.embeds && message.embeds.length > 0 && message.embeds.map((embed, i) => (
        <div key={i} style={{
          marginTop: "0.5rem", borderLeft: "4px solid #5865f2",
          background: "#f3f4f6", padding: "0.5rem 0.75rem", borderRadius: "0 4px 4px 0",
        }}>
          {embed.title && (
            <div style={{ fontSize: "0.813rem", fontWeight: 600, marginBottom: "0.125rem" }}>
              {embed.url ? <a href={embed.url} style={{ color: "#2563eb" }}>{embed.title}</a> : embed.title}
            </div>
          )}
          {embed.description && (
            <div style={{ fontSize: "0.813rem", color: "#4b5563" }}>{embed.description}</div>
          )}
        </div>
      ))}

      {/* Reactions */}
      {message.reactions && message.reactions.length > 0 && (
        <div style={{ display: "flex", gap: "0.375rem", marginTop: "0.25rem" }}>
          {message.reactions.map((r, i) => (
            <span key={i} style={{
              fontSize: "0.75rem", background: "#e5e7eb",
              padding: "0.125rem 0.375rem", borderRadius: "4px",
            }}>
              {r.emoji} {r.count}
            </span>
          ))}
        </div>
      )}
    </div>
  );
}
```

#### Updated MessageBubble.tsx (dispatcher)

Replace the current render with:

```typescript
import type { Message } from "../lib/types";
import SmsBubble from "./messages/SmsBubble";
import ImessageBubble from "./messages/ImessageBubble";
import DiscordBubble from "./messages/DiscordBubble";

export default function MessageBubble({ message }: { message: Message }) {
  switch (message.service?.toLowerCase()) {
    case "imessage":
    case "ios":
      return <ImessageBubble message={message} />;
    case "discord":
      return <DiscordBubble message={message} />;
    case "whatsapp":
      // Fall through to base for now — WhatsApp renderer in Task 8
    default:
      return <SmsBubble message={message} />;
  }
}
```

- [ ] **Step 1: Create the `web/src/components/messages/` directory**
- [ ] **Step 2: Write SmsBubble, ImessageBubble, DiscordBubble**
- [ ] **Step 3: Rewrite MessageBubble as dispatcher**
- [ ] **Step 4: Build and verify** (`cd web && npm run build`)
- [ ] **Step 5: Commit**

---

### Task 3: Attachment thumbnails, lightbox, and video player

**Files:**
- Create: `web/src/components/AttachmentThumbnail.tsx`
- Create: `web/src/components/AttachmentLightbox.tsx`
- Create: `web/src/components/VideoPlayer.tsx`
- Modify: `web/src/components/MessageBubble.tsx` (render thumbnails inline)

**Goal:** Render attachment thumbnails inline in the message stream. Click opens a full-screen lightbox with prev/next nav for images. Video attachments get an inline player.

The vault API serves attachments at `GET /v1/assets/{sha256}` with the auth token. Thumbnails are served by the same endpoint — the browser handles scaling via CSS/HTML attributes.

#### AttachmentThumbnail.tsx

```typescript
import { getBaseUrl } from "../lib/api";
import { useAuth } from "../lib/auth";
import type { Attachment } from "../lib/types";

export default function AttachmentThumbnail({
  attachment,
  onClick,
}: {
  attachment: Attachment;
  onClick: () => void;
}) {
  const { token } = useAuth();
  const url = `${getBaseUrl()}/v1/assets/${attachment.sha256}`;

  const isVideo = attachment.mime_type?.startsWith("video/");
  const isImage = attachment.mime_type?.startsWith("image/");

  if (!isImage && !isVideo) {
    return (
      <div style={{
        display: "flex", alignItems: "center", gap: "0.5rem",
        padding: "0.5rem", background: "#f9fafb", borderRadius: "4px",
        marginTop: "0.375rem", fontSize: "0.813rem",
      }}>
        <span>📎</span>
        <span style={{ color: "#374151" }}>{attachment.filename}</span>
        <span style={{ color: "#9ca3af" }}>
          {(attachment.size_bytes / 1024).toFixed(0)} KB
        </span>
      </div>
    );
  }

  return (
    <div
      onClick={onClick}
      style={{
        marginTop: "0.375rem", cursor: "pointer",
        maxWidth: "300px", borderRadius: "6px", overflow: "hidden",
        border: "1px solid #e5e7eb",
      }}
    >
      {isImage && (
        <img
          src={url}
          alt={attachment.filename}
          loading="lazy"
          style={{ width: "100%", height: "auto", display: "block" }}
        />
      )}
      {isVideo && (
        <div style={{ position: "relative" }}>
          <img
            src={url}
            alt={attachment.filename}
            loading="lazy"
            style={{ width: "100%", height: "auto", display: "block", opacity: 0.7 }}
          />
          <div style={{
            position: "absolute", inset: 0, display: "flex",
            alignItems: "center", justifyContent: "center",
          }}>
            <span style={{ fontSize: "2rem" }}>▶️</span>
          </div>
        </div>
      )}
    </div>
  );
}
```

#### AttachmentLightbox.tsx

```typescript
import type { Attachment } from "../lib/types";

export default function AttachmentLightbox({
  attachments,
  currentIndex,
  onClose,
  onPrev,
  onNext,
}: {
  attachments: Attachment[];
  currentIndex: number;
  onClose: () => void;
  onPrev: () => void;
  onNext: () => void;
}) {
  const attachment = attachments[currentIndex];
  if (!attachment) return null;

  return (
    <div style={{
      position: "fixed", inset: 0, background: "rgba(0,0,0,0.9)",
      display: "flex", alignItems: "center", justifyContent: "center",
      zIndex: 200,
    }} onClick={onClose}>
      {/* Prev */}
      {attachments.length > 1 && (
        <button onClick={(e) => { e.stopPropagation(); onPrev(); }}
          style={{
            position: "absolute", left: "1rem", top: "50%", transform: "translateY(-50%)",
            background: "rgba(255,255,255,0.2)", border: "none", color: "#fff",
            fontSize: "2rem", width: "48px", height: "48px", borderRadius: "50%",
            cursor: "pointer", display: "flex", alignItems: "center", justifyContent: "center",
          }}>
          ‹
        </button>
      )}

      <img
        src={`/v1/assets/${attachment.sha256}`}
        alt={attachment.filename}
        style={{ maxWidth: "90vw", maxHeight: "90vh", objectFit: "contain" }}
        onClick={(e) => e.stopPropagation()}
      />

      {/* Next */}
      {attachments.length > 1 && (
        <button onClick={(e) => { e.stopPropagation(); onNext(); }}
          style={{
            position: "absolute", right: "1rem", top: "50%", transform: "translateY(-50%)",
            background: "rgba(255,255,255,0.2)", border: "none", color: "#fff",
            fontSize: "2rem", width: "48px", height: "48px", borderRadius: "50%",
            cursor: "pointer", display: "flex", alignItems: "center", justifyContent: "center",
          }}>
          ›
        </button>
      )}

      {/* Close + counter */}
      <div style={{ position: "absolute", top: "1rem", right: "1rem", display: "flex", gap: "1rem", alignItems: "center" }}>
        <span style={{ color: "#fff", fontSize: "0.875rem" }}>
          {currentIndex + 1} / {attachments.length}
        </span>
        <button onClick={onClose}
          style={{ background: "rgba(255,255,255,0.2)", border: "none", color: "#fff",
            fontSize: "1.5rem", width: "40px", height: "40px", borderRadius: "50%", cursor: "pointer" }}>
          ×
        </button>
      </div>
    </div>
  );
}
```

#### VideoPlayer.tsx

A lightweight inline video player using the HTML5 `<video>` element:

```typescript
import { getBaseUrl } from "../lib/api";
import { useAuth } from "../lib/auth";
import type { Attachment } from "../lib/types";

export default function VideoPlayer({ attachment }: { attachment: Attachment }) {
  const { token } = useAuth();
  const url = `${getBaseUrl()}/v1/assets/${attachment.sha256}`;

  return (
    <div style={{ marginTop: "0.375rem", maxWidth: "400px" }}>
      <video
        controls
        preload="metadata"
        style={{ width: "100%", borderRadius: "6px" }}
      >
        <source src={url} type={attachment.mime_type} />
      </video>
    </div>
  );
}
```

#### Wire into MessageBubble

In each service renderer (starting with SmsBubble, which is the base), add attachment rendering below the body:

Inside the render, after the message body div:
```typescript
{message.attachments && message.attachments.length > 0 && (
  <div>
    {message.attachments.map((att) => (
      att.mime_type?.startsWith("video/") ? (
        <VideoPlayer key={att.sha256} attachment={att} />
      ) : (
        <AttachmentThumbnail
          key={att.sha256}
          attachment={att}
          onClick={() => onAttachmentClick(att)}
        />
      )
    ))}
  </div>
)}
```

The `onAttachmentClick` callback and lightbox state should be managed by `MessageView` and passed down as a prop to `MessageBubble`, which passes it to each service renderer.

- [ ] **Step 1: Create AttachmentThumbnail, AttachmentLightbox, VideoPlayer**
- [ ] **Step 2: Add lightbox state to MessageView, pass onAttachmentClick to MessageBubble**
- [ ] **Step 3: Wire thumbnails into SmsBubble (base renderer)**
- [ ] **Step 4: Build and verify**
- [ ] **Step 5: Commit**

---

### Task 4: Inline name editing in contact drawer

**Files:**
- Modify: `web/src/components/ContactDrawer.tsx`

**Goal:** The contact name in the drawer becomes click-to-edit. Clicking the name replaces it with an input field. Enter or blur saves via `POST /v1/export/contacts/{id}`. The updated name propagates everywhere — the conversation list re-fetches to pick up the change.

Add state to ContactDrawer:

```typescript
const [editingName, setEditingName] = useState(false);
const [nameValue, setNameValue] = useState(detail.name);

// Sync when detail changes (new contact selected)
useEffect(() => { setNameValue(detail.name); setEditingName(false); }, [detail.name]);
```

Replace the static `<h2>{detail.name}</h2>` with:

```typescript
{editingName ? (
  <input
    type="text"
    value={nameValue}
    onChange={(e) => setNameValue(e.target.value)}
    onKeyDown={async (e) => {
      if (e.key === "Enter") {
        await apiClient.post(`/v1/export/contacts/${contactId}`, { name: nameValue });
        setEditingName(false);
        // Re-fetch detail
      }
    }}
    onBlur={async () => {
      if (nameValue !== detail.name) {
        await apiClient.post(`/v1/export/contacts/${contactId}`, { name: nameValue });
      }
      setEditingName(false);
    }}
    autoFocus
    style={{ fontSize: "1.125rem", fontWeight: 600, padding: "0.25rem", width: "100%" }}
  />
) : (
  <h2
    onClick={() => setEditingName(true)}
    style={{ margin: 0, fontSize: "1.125rem", cursor: "pointer" }}
    title="Click to edit"
  >
    {detail.name} ✎
  </h2>
)}
```

- [ ] **Step 1: Add inline editing to ContactDrawer**
- [ ] **Step 2: Build and verify**
- [ ] **Step 3: Commit**

---

### Task 5: Local rename UI for conversations

**Files:**
- Modify: `web/src/components/ConversationRow.tsx`

**Goal:** Clicking a conversation name in the list enters inline-edit mode. The rename is stored in the `label` field on the Conversation object and persisted to localStorage (or the server in a future iteration).

Add state and click handler:

```typescript
const [editing, setEditing] = useState(false);
const [labelValue, setLabelValue] = useState(conversation.label || "");

const handleSaveLabel = () => {
  // Store locally — the API endpoint for persisting labels is follow-up (Tier 4)
  conversation.label = labelValue.trim() || null;
  setEditing(false);
};
```

Replace the display name span with:

```typescript
{editing ? (
  <input
    type="text"
    value={labelValue}
    onChange={(e) => setLabelValue(e.target.value)}
    onKeyDown={(e) => {
      if (e.key === "Enter") handleSaveLabel();
      if (e.key === "Escape") setEditing(false);
    }}
    onBlur={handleSaveLabel}
    onClick={(e) => e.stopPropagation()}
    autoFocus
    style={{ fontSize: "0.875rem", fontWeight: 500, width: "100%", padding: "0.125rem 0.25rem" }}
  />
) : (
  <span
    onClick={(e) => { e.stopPropagation(); setEditing(true); setLabelValue(conversation.label || displayName(conversation)); }}
    title="Click to rename"
    style={{ cursor: "pointer" }}
  >
    {displayName(conversation)}
    {conversation.label && <span style={{ fontSize: "0.688rem", color: "#9ca3af", marginLeft: "0.25rem" }}>(renamed)</span>}
  </span>
)}
```

- [ ] **Step 1: Add inline rename to ConversationRow**
- [ ] **Step 2: Build and verify**
- [ ] **Step 3: Commit**

---

### Task 6: Onboarding profile creation

**Files:**
- Create: `web/src/screens/OnboardingScreen.tsx`
- Modify: `web/src/App.tsx` — route to onboarding after registration
- Modify: `web/src/lib/auth.tsx` — add `needsOnboarding` flag

**Goal:** After creating an account, the user sees an onboarding screen to set their display name and handles. This is the profile creation step the spec calls for — the profile is created at signup, not hidden in Settings.

#### OnboardingScreen.tsx

```typescript
import { useState } from "react";
import { useAuth } from "../lib/auth";
import { apiClient } from "../lib/api";

interface HandleInput {
  handle: string;
  service: string;
}

const SERVICES = ["phone", "email", "discord", "instagram", "telegram", "signal"];

export default function OnboardingScreen() {
  const { login, token, serverUrl, accountId } = useAuth();
  const [displayName, setDisplayName] = useState("");
  const [handles, setHandles] = useState<HandleInput[]>([{ handle: "", service: "phone" }]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");

  const addHandle = () => {
    setHandles([...handles, { handle: "", service: "phone" }]);
  };

  const updateHandle = (index: number, field: keyof HandleInput, value: string) => {
    const next = [...handles];
    next[index] = { ...next[index], [field]: value };
    setHandles(next);
  };

  const removeHandle = (index: number) => {
    if (handles.length === 1) return;
    setHandles(handles.filter((_, i) => i !== index));
  };

  const handleSubmit = async () => {
    setLoading(true);
    setError("");
    try {
      await apiClient.post("/v1/account/profile", {
        name: displayName.trim(),
        handles: handles.filter((h) => h.handle.trim()),
      });
      // Mark onboarding complete — user is now fully authenticated
      login(serverUrl, token!, accountId!);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  };

  const canSubmit = displayName.trim() && handles.some((h) => h.handle.trim());

  return (
    <div style={{
      display: "flex", alignItems: "center", justifyContent: "center",
      minHeight: "100vh", background: "#f3f4f6", fontFamily: "system-ui",
    }}>
      <div style={{
        background: "#fff", padding: "2rem", borderRadius: "8px",
        width: "100%", maxWidth: "480px", boxShadow: "0 1px 3px rgba(0,0,0,0.1)",
      }}>
        <h1 style={{ margin: "0 0 0.5rem", fontSize: "1.5rem", textAlign: "center" }}>
          Welcome to Message Vault
        </h1>
        <p style={{ textAlign: "center", color: "#6b7280", fontSize: "0.875rem", marginBottom: "1.5rem" }}>
          Set up your profile so we can match imported messages to you.
        </p>

        <label style={labelStyle}>Display Name</label>
        <input type="text" value={displayName} onChange={(e) => setDisplayName(e.target.value)}
          placeholder="Your name" style={inputStyle} autoFocus />

        <label style={{ ...labelStyle, marginTop: "1rem" }}>My Handles</label>
        <p style={{ fontSize: "0.75rem", color: "#9ca3af", marginBottom: "0.5rem" }}>
          Add handles you use across services. These are used to match your messages.
        </p>

        {handles.map((h, i) => (
          <div key={i} style={{ display: "flex", gap: "0.5rem", marginBottom: "0.5rem" }}>
            <select value={h.service} onChange={(e) => updateHandle(i, "service", e.target.value)}
              style={{ padding: "0.375rem 0.5rem", fontSize: "0.875rem", border: "1px solid #d1d5db", borderRadius: "4px", width: "120px" }}>
              {SERVICES.map((s) => <option key={s} value={s}>{s}</option>)}
            </select>
            <input type="text" value={h.handle} onChange={(e) => updateHandle(i, "handle", e.target.value)}
              placeholder={h.service === "phone" ? "+1 555-1234" : h.service === "discord" ? "user#1234" : "@handle"}
              style={{ flex: 1, padding: "0.375rem 0.5rem", fontSize: "0.875rem", border: "1px solid #d1d5db", borderRadius: "4px" }} />
            <button onClick={() => removeHandle(i)} disabled={handles.length === 1}
              style={{ border: "none", background: "none", color: "#9ca3af", cursor: "pointer", fontSize: "1.25rem" }}>
              ×
            </button>
          </div>
        ))}
        <button onClick={addHandle}
          style={{ fontSize: "0.813rem", border: "none", background: "none", color: "#2563eb", cursor: "pointer", padding: 0 }}>
          + Add another handle
        </button>

        {error && (
          <div style={{ marginTop: "1rem", padding: "0.5rem 0.75rem", background: "#fef2f2", border: "1px solid #fecaca", borderRadius: "4px", color: "#991b1b", fontSize: "0.813rem" }}>
            {error}
          </div>
        )}

        <button onClick={handleSubmit} disabled={!canSubmit || loading}
          style={{ width: "100%", marginTop: "1.5rem", padding: "0.75rem", fontSize: "1rem", fontWeight: 600 }}>
          {loading ? "Saving…" : "Continue to Vault"}
        </button>
      </div>
    </div>
  );
}

const labelStyle: React.CSSProperties = {
  fontSize: "0.875rem", fontWeight: 500, display: "block", marginBottom: "0.25rem",
};

const inputStyle: React.CSSProperties = {
  width: "100%", padding: "0.5rem", fontSize: "0.875rem",
  border: "1px solid #d1d5db", borderRadius: "4px", boxSizing: "border-box",
};
```

#### Wire into App.tsx

In `App.tsx`, add an onboarding check. After registration, the auth context sets a `needsOnboarding` flag (default `true` for new accounts, `false` for existing ones). The `AppContent` component checks this:

```typescript
function AppContent() {
  const { isAuthenticated, needsOnboarding } = useAuth();

  if (isAuthenticated && needsOnboarding) {
    return <OnboardingScreen />;
  }

  if (isAuthenticated) {
    return <AppLayout />;
  }
  // ... login/register ...
}
```

The `/v1/account/profile` endpoint already exists (from Plan 1) — the `GET` returns whether a profile exists. Auth context can check this on login.

- [ ] **Step 1: Create OnboardingScreen**
- [ ] **Step 2: Add `needsOnboarding` to auth context** (set true after register, check profile on login)
- [ ] **Step 3: Wire into App.tsx**
- [ ] **Step 4: Build and verify**
- [ ] **Step 5: Commit**

---

### Task 7: Search results grouped-by-conversation view

**Files:**
- Create: `web/src/screens/SearchResults.tsx`
- Modify: `web/src/components/AppLayout.tsx` — show SearchResults when search is active

**Goal:** When the user runs a global search (Enter in the search bar), the left panel conversation list is replaced with grouped search results. Each result row shows the conversation name, match count, and a snippet. Clicking a result opens that conversation with the find bar pre-populated.

#### SearchResults.tsx

```typescript
import { useState, useEffect } from "react";
import { apiClient } from "../lib/api";
import type { Conversation } from "../lib/types";

interface SearchResult {
  conversation: Conversation;
  match_count: number;
  snippet: string;
}

export default function SearchResults({
  query,
  onSelectResult,
}: {
  query: string;
  onSelectResult: (conversation: Conversation, term: string) => void;
}) {
  const [results, setResults] = useState<SearchResult[]>([]);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    if (!query.trim()) return;
    setLoading(true);
    apiClient
      .get<{ results: SearchResult[] }>(
        `/v1/export/messages?q=${encodeURIComponent(query)}&group_by=conversation&limit=50`,
      )
      .then((res) => setResults(res.results))
      .catch(() => setResults([]))
      .finally(() => setLoading(false));
  }, [query]);

  if (loading) return <div style={{ padding: "1rem", fontSize: "0.813rem", color: "#9ca3af" }}>Searching…</div>;
  if (results.length === 0) return <div style={{ padding: "1rem", fontSize: "0.813rem", color: "#9ca3af" }}>No results for "{query}"</div>;

  return (
    <div style={{ overflow: "auto", flex: 1 }}>
      <div style={{ padding: "0.5rem 0.75rem", fontSize: "0.75rem", color: "#6b7280", borderBottom: "1px solid #e5e7eb" }}>
        {results.length} conversation{results.length !== 1 ? "s" : ""} matching "{query}"
      </div>
      {results.map((r) => (
        <button
          key={r.conversation.id}
          onClick={() => onSelectResult(r.conversation, query)}
          style={{
            display: "block", width: "100%", textAlign: "left", border: "none",
            background: "transparent", padding: "0.5rem 0.75rem", cursor: "pointer",
            borderBottom: "1px solid #f3f4f6",
          }}
        >
          <div style={{ fontSize: "0.875rem", fontWeight: 500, color: "#1f2937" }}>
            {r.conversation.label || r.conversation.participants.map((p) => p.name || p.handle).join(", ")}
          </div>
          <div style={{ fontSize: "0.75rem", color: "#6b7280" }}>
            {r.match_count} match{r.match_count !== 1 ? "es" : ""}
          </div>
          <div style={{ fontSize: "0.75rem", color: "#9ca3af", marginTop: "0.125rem", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
            {r.snippet}
          </div>
        </button>
      ))}
    </div>
  );
}
```

#### Wire into AppLayout

In `AppLayout.tsx`, when a search is active (`searchQuery` is non-empty and the user pressed Enter), show `SearchResults` in the left panel instead of `ConversationList`. When a result is clicked, set the active conversation and pre-fill the find bar.

- [ ] **Step 1: Create SearchResults**
- [ ] **Step 2: Wire into AppLayout** — add search state, show SearchResults when active
- [ ] **Step 3: Wire result click** — open conversation, pass search term to MessageView's find bar
- [ ] **Step 4: Build and verify**
- [ ] **Step 5: Commit**

---

### Task 8: Handle matching on-add + WhatsApp/Instagram renderers

**Files:**
- Modify: `web/src/components/ContactDrawer.tsx` — add "Add handle" form with match results
- Create: `web/src/components/messages/WhatsAppBubble.tsx`
- Create: `web/src/components/messages/InstagramBubble.tsx`
- Modify: `web/src/components/MessageBubble.tsx` — add WhatsApp and Instagram cases
- Create: `message-vault-rs/src/server.rs` — add `POST /v1/contacts/{id}/handles/match` endpoint (or extend existing)

**Goal:** When adding a new handle to a contact, the system checks for matching conversations and shows a prompt: "We found 3 conversations matching bob#1234 on Discord." The matching is a lightweight query — search messages/conversations for the new handle value. If the backend endpoint doesn't exist yet, fall back to a client-side search via the export API.

Also complete the service-specific renderers with WhatsApp and Instagram.

#### Handle matching in ContactDrawer

Add an "Add handle" section below the existing handles list:

```typescript
const [newHandle, setNewHandle] = useState("");
const [newService, setNewService] = useState("discord");
const [matchResults, setMatchResults] = useState<Conversation[] | null>(null);

const checkMatches = async () => {
  if (!newHandle.trim()) return;
  try {
    const res = await apiClient.get<{ conversations: Conversation[] }>(
      `/v1/export/conversations?q=handle:${encodeURIComponent(newHandle)}`,
    );
    setMatchResults(res.conversations);
  } catch {
    setMatchResults([]);
  }
};

const addHandle = async () => {
  if (!newHandle.trim()) return;
  await apiClient.post(`/v1/export/contacts/${contactId}`, {
    add_handle: { handle: newHandle.trim(), service: newService },
  });
  setNewHandle("");
  setMatchResults(null);
  // Re-fetch detail
};
```

After `checkMatches()`:
```typescript
{matchResults !== null && matchResults.length > 0 && (
  <div style={{ marginTop: "0.5rem", padding: "0.5rem", background: "#eff6ff", borderRadius: "4px", fontSize: "0.813rem" }}>
    We found {matchResults.length} conversation{matchResults.length !== 1 ? "s" : ""} matching {newHandle} on {newService}.
  </div>
)}
```

#### WhatsAppBubble.tsx

```typescript
import type { Message } from "../../lib/types";

export default function WhatsAppBubble({ message }: { message: Message }) {
  const time = new Date(message.sent_at).toLocaleString([], {
    month: "short", day: "numeric", hour: "numeric", minute: "2-digit",
  });

  return (
    <div style={{ padding: "0.5rem 1.5rem", borderBottom: "1px solid #f3f4f6" }}>
      <div style={{ display: "flex", gap: "0.5rem", marginBottom: "0.25rem" }}>
        <span style={{ fontSize: "0.75rem", fontWeight: 600, color: "#075e54" }}>
          {message.sender.name || message.sender.handle}
        </span>
        <span style={{ fontSize: "0.75rem", color: "#9ca3af" }}>{time}</span>
      </div>

      {/* Reply chain */}
      {message.reply_to_message && (
        <div style={{
          fontSize: "0.75rem", color: "#6b7280", background: "#f3f4f6",
          padding: "0.25rem 0.5rem", borderRadius: "4px", marginBottom: "0.25rem",
          borderLeft: "3px solid #25d366",
        }}>
          <span style={{ fontWeight: 600 }}>{message.reply_to_message.sender_name}</span>:{" "}
          {message.reply_to_message.body_preview}
        </div>
      )}

      {/* Deleted indicator */}
      {message.deleted_indicator ? (
        <div style={{ fontSize: "0.875rem", color: "#9ca3af", fontStyle: "italic" }}>
          This message was deleted
        </div>
      ) : (
        <div style={{ fontSize: "0.875rem", color: "#1f2937", lineHeight: 1.5, whiteSpace: "pre-wrap" }}>
          {message.body}
        </div>
      )}
    </div>
  );
}
```

#### InstagramBubble.tsx

```typescript
import type { Message } from "../../lib/types";

export default function InstagramBubble({ message }: { message: Message }) {
  const time = new Date(message.sent_at).toLocaleString([], {
    month: "short", day: "numeric", hour: "numeric", minute: "2-digit",
  });

  return (
    <div style={{ padding: "0.5rem 1.5rem", borderBottom: "1px solid #f3f4f6" }}>
      <div style={{ display: "flex", gap: "0.5rem", marginBottom: "0.25rem" }}>
        <span style={{ fontSize: "0.75rem", fontWeight: 600, color: "#e4405f" }}>
          {message.sender.name || message.sender.handle}
        </span>
        <span style={{ fontSize: "0.75rem", color: "#9ca3af" }}>{time}</span>
        {message.is_story_reply && (
          <span style={{ fontSize: "0.688rem", color: "#e4405f" }}>Story reply</span>
        )}
        {message.forwarded && (
          <span style={{ fontSize: "0.688rem", color: "#9ca3af" }}>Forwarded</span>
        )}
      </div>
      <div style={{ fontSize: "0.875rem", color: "#1f2937", lineHeight: 1.5, whiteSpace: "pre-wrap" }}>
        {message.body}
      </div>
    </div>
  );
}
```

Update MessageBubble dispatcher with the new cases.

- [ ] **Step 1: Add handle matching UI to ContactDrawer**
- [ ] **Step 2: Create WhatsAppBubble and InstagramBubble**
- [ ] **Step 3: Update MessageBubble dispatcher**
- [ ] **Step 4: Build and verify**
- [ ] **Step 5: Commit**

---

### Task 9: Date jump links + sources panel

**Files:**
- Modify: `web/src/screens/MessageView.tsx` — add date jump links
- Create: `web/src/components/SourcesPanel.tsx`

**Goal:** Date jump links in the conversation header let the user click a year/month and jump to that page offset in the message list. The sources panel (accessible from a "Sources" button in the header) shows backup provenance — which backups contributed data for this conversation.

#### Date jump links in MessageView

In the MessageView header, below the participant info, add a date jump bar. Compute available date ranges from the conversation's `date_range_start` and `date_range_end`, and provide clickable year links. When clicked, estimate the offset: `(targetYear - startYear) / totalYears * totalMessages`.

```typescript
// Inside MessageView, compute date jump targets:
const dateJumps = useMemo(() => {
  if (!conversation.date_range_start || !conversation.date_range_end) return [];
  const start = new Date(conversation.date_range_start);
  const end = new Date(conversation.date_range_end);
  const years: number[] = [];
  for (let y = start.getFullYear(); y <= end.getFullYear(); y++) {
    years.push(y);
  }
  return years.map((year) => ({
    year,
    estimatedOffset: Math.floor(
      ((year - start.getFullYear()) / Math.max(1, end.getFullYear() - start.getFullYear())) *
        conversation.message_count,
    ),
  }));
}, [conversation]);

// Render date jump links in the header:
{dateJumps.length > 0 && (
  <div style={{ display: "flex", gap: "0.5rem", marginTop: "0.25rem" }}>
    {dateJumps.map((jump) => (
      <button
        key={jump.year}
        onClick={() => fetchPage(jump.estimatedOffset)}
        style={{
          fontSize: "0.688rem", border: "1px solid #d1d5db", background: "#fff",
          padding: "0.125rem 0.375rem", borderRadius: "4px", cursor: "pointer",
          color: "#2563eb",
        }}
      >
        {jump.year}
      </button>
    ))}
  </div>
)}
```

#### SourcesPanel.tsx

A slide-over panel (same pattern as ContactDrawer) showing backup provenance:

```typescript
import { useState, useEffect } from "react";
import { apiClient } from "../lib/api";

interface SourceInfo {
  backup_name: string;
  message_count: number;
  unique_count: number;
  percentage: number;
}

export default function SourcesPanel({
  conversationId,
  onClose,
}: {
  conversationId: string | null;
  onClose: () => void;
}) {
  const [sources, setSources] = useState<SourceInfo[]>([]);

  useEffect(() => {
    if (!conversationId) return;
    apiClient
      .get<{ sources: SourceInfo[] }>(`/v1/export/conversations/${conversationId}/sources`)
      .then((res) => setSources(res.sources))
      .catch(() => setSources([]));
  }, [conversationId]);

  if (!conversationId) return null;

  const total = sources.reduce((sum, s) => sum + s.unique_count, 0);

  return (
    <>
      <div onClick={onClose} style={{
        position: "fixed", inset: 0, background: "rgba(0,0,0,0.2)", zIndex: 40,
      }} />
      <div style={{
        position: "fixed", right: 0, top: 0, bottom: 0, width: "320px",
        background: "#fff", boxShadow: "-2px 0 8px rgba(0,0,0,0.1)", zIndex: 50,
        overflow: "auto", padding: "1.5rem",
      }}>
        <div style={{ display: "flex", justifyContent: "space-between", marginBottom: "1rem" }}>
          <h2 style={{ margin: 0, fontSize: "1.125rem" }}>Sources</h2>
          <button onClick={onClose} style={{ border: "none", background: "none", fontSize: "1.25rem", cursor: "pointer" }}>×</button>
        </div>

        {sources.length === 0 ? (
          <div style={{ fontSize: "0.875rem", color: "#9ca3af" }}>No source data available.</div>
        ) : (
          <>
            {sources.map((s, i) => (
              <div key={i} style={{ marginBottom: "0.75rem", padding: "0.5rem", background: "#f9fafb", borderRadius: "4px" }}>
                <div style={{ fontSize: "0.875rem", fontWeight: 500 }}>{s.backup_name}</div>
                <div style={{ fontSize: "0.75rem", color: "#6b7280" }}>
                  {s.message_count.toLocaleString()} messages ({s.percentage}% of total)
                </div>
                <div style={{ fontSize: "0.75rem", color: "#9ca3af" }}>
                  {s.unique_count.toLocaleString()} unique
                </div>
              </div>
            ))}
            <div style={{ marginTop: "0.5rem", fontSize: "0.813rem", color: "#6b7280" }}>
              Net total: {total.toLocaleString()} unique messages
            </div>
          </>
        )}
      </div>
    </>
  );
}
```

Add a "Sources" button to the MessageView header that opens SourcesPanel.

- [ ] **Step 1: Add date jump links to MessageView**
- [ ] **Step 2: Create SourcesPanel**
- [ ] **Step 3: Wire Sources button into MessageView header**
- [ ] **Step 4: Build and verify**
- [ ] **Step 5: Commit**

---

## Verification

After all tasks:
1. `cd message-vault-io/web && npm run build` — compiles cleanly
2. `cd message-vault-rs && cargo build -p message-vault-rs` — compiles cleanly (if API endpoints were added)
3. Manual test: log in → see onboarding if new account → browse conversations → click a conversation to rename → open message view → see service-specific bubbles → click attachment → lightbox → search → see grouped results → open contact drawer → edit name inline → add handle → see match prompt

## Dependencies

- Tasks 1-2: no dependencies
- Task 3: depends on Task 2 (MessageBubble dispatcher in place)
- Task 4: no dependencies
- Task 5: no dependencies
- Task 6: no dependencies
- Task 7: depends on search query support (existing)
- Task 8: depends on Task 4 (ContactDrawer needs editing infrastructure)
- Task 9: depends on Task 2 (MessageView needs the dispatcher)
