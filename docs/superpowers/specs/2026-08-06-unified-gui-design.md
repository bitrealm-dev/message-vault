# Unified GUI Design

**Date**: 2026-08-06
**Status**: draft

## Context

message-vault-io has a Tauri v2 desktop app (Vite+React) with extraction, format conversion, and vault push/pull screens. message-vault-rs has a Next.js web app for browsing vault data. The user wants one shared GUI codebase that works in both contexts:

- **Tauri desktop app**: Full experience — browsing vault data plus desktop-only actions (extract, format, push, pull)
- **Vault server web deployment**: Browser-based browsing of vault data served as static files by axum

The current message-vault-rs browsing interface has layout and data-model issues that need to be addressed rather than blindly ported.

## Architecture decisions

### Framework: Vite+React

The existing Tauri Vite+React app in message-vault-io becomes the canonical GUI. The vault browsing UI from message-vault-rs is ported into this app. The Next.js web app in message-vault-rs is retired.

**Rationale:**
- Tauri dev experience already works with hot reload on `cargo tauri dev`
- Server-side rendering provides no benefit — all data lives behind the axum API and requires authentication
- API routes in Next.js are redundant — the browser calls axum directly via `fetch()`
- One build output (`dist/`) serves both contexts: bundled into Tauri binary, served as static files by axum in the Docker image

### Two build targets, one codebase

```
Vite build → dist/ (HTML/JS/CSS)
              │
              ├── Tauri: bundled into binary via frontendDist config
              │          Desktop-only screens visible when isTauri() === true
              │
              └── Docker: served as static files by axum
                         Desktop-only screens hidden
```

Desktop-only screens (Extract, Format, Push, Pull) are gated behind a runtime `isTauri()` check. The vault browsing screens (conversations, search, profile, settings) are available in both contexts.

### Architecture diagram

```
Browser → axum API → SQLite (vault data)
Browser ← axum serves dist/ as static files

Tauri → invoke() → Rust commands (extract, format, push, pull, ffmpeg, filesystem)
Tauri → fetch()  → axum API (browse, search, contacts, settings)
```

## Data model philosophy

The old model was contact-centric — a phone number was the primary handle, contacts were the navigation unit, and messages lived under contacts. This breaks down with multi-service data (Discord, Instagram, SMS, iMessage).

The new model is conversation-centric:

- A **conversation** is the primary unit — a set of participants messaging each other at a particular time on a particular service
- Conversations are sorted by most recent message timestamp (newest first)
- Direct messages (1 other person) and group messages (2+ people) appear in the same flat list
- A **profile** is reached by clicking a participant's name from within a conversation — it shows their handles, conversation history, and metadata
- The profile is created at signup, not hidden in settings

### Handles

A person can have multiple handles across services:

| Handle type | Example |
|-------------|---------|
| Phone | `+1 555-1234` |
| Email | `bob@example.com` |
| Discord | `bob#1234` |
| Instagram | `@bob.ig` |
| Telegram | `@bob_tg` |
| Signal | `+1 555-1234` |

Handles are set on the user's own profile during onboarding and can be edited later. The system uses handles to match imported messages to the vault owner and to other participants.

## Screen flow

### Login

```
┌──────────────────────────────────┐
│          Message Vault           │
│                                  │
│   Server URL: [______________]   │
│                                  │
│   [Hanko passkey login]          │
│   or                             │
│   Username: [______________]     │
│   Password: [______________]     │
│                                  │
│   [Create account]               │
│                                  │
│   ─────────── or ───────────     │
│                                  │
│   [Extract messages]             │
│   Parse a backup to JSONL        │
│   without connecting to a vault  │
│                                  │
│   [Format conversion]            │
│   Convert between output formats │
└──────────────────────────────────┘
```

The user enters the vault server URL. The client calls the server's auth-mode endpoint to determine whether it uses Hanko passkeys or local username/password. The login form renders the appropriate fields. Auth mode is a server-side setting (configured in the Docker container) — the client never chooses.

After entering the server URL, the login form adapts to the server's mode:
- **Hanko**: Show the Hanko passkey login flow
- **Local**: Show username and password fields

The SPA stores the auth token and navigates to the conversation list.

Below the auth section, two offline tools are available without login:
- **Extract messages**: Parse a backup to JSONL without connecting to a vault
- **Format conversion**: Convert between output formats (JSONL/EML/MBOX/CSV/XML)

These do not require authentication — they only access the local filesystem. In the web deployment, they are hidden (no local filesystem access).

### Onboarding (new user)

New user flow: Create account → Create profile. The profile is created at signup, not hidden in Settings later:

- Display name
- Handles (phone, email, Discord, Instagram, etc.) — used to match imported messages to the owner
- Profile photo

### Home — Conversation list

After login, the user sees a flat conversation list sorted by most recent message timestamp:

```
┌──────────────────────┬──────────────────────────────────────┐
│  🔍 Global search    │                                      │
│                      │  Select a conversation to view       │
│  SAVED GROUPS   [+]  │  messages, or search to find         │
│  ├ Work team   (12)  │  someone.                            │
│  ├ Family       (8)  │                                      │
│  ├ 2023 Arch.  (47)  │                                      │
│  └ Videos      (15)  │                                      │
│                      │                                      │
│  ▸ Conversations  142 │                                      │
│  ▸ Contacts        87 │                                      │
│  ▸ Trash            3 │                                      │
│                      │                                      │
│  ────────────────    │                                      │
│                      │                                      │
│  ┌────────────────┐  │                                      │
│  │  📥 Import     │  │                                      │
│  └────────────────┘  │                                      │
│  ┌────────────────┐  │                                      │
│  │  📤 Export     │  │                                      │
│  └────────────────┘  │                                      │
│                      │                                      │
│  ────────────────    │                                      │
│                      │                                      │
│  👤 Profile          │                                      │
│  ⚙ Settings         │                                      │
└──────────────────────┴──────────────────────────────────────┘
```

The right panel shows a placeholder until a conversation is selected. Import and Export are left-panel buttons (desktop only, require auth). Extract and Format are available from the login screen without authentication.

### Conversation rows — display logic

**Direct messages (1 other participant):**
```
Bob Smith                              Aug 6
  +1 555-1234 · SMS
```
If the name is resolved from contacts, show it. Otherwise show the handle. Service indicator underneath.

**Small groups (2-7 participants):**
```
Bob Smith, Carol J., Ted A.            Jul 28
  3 participants · 142 messages · SMS
```
Show all names, then count + message count + service.

**Large groups (8+ participants):**
```
20 participants · 214 messages · Sep 2020 – Jan 2022 · SMS
```
Don't show participant names — the date range distinguishes it. Users can rename locally.

**Local rename:** Any conversation can be given a custom label that overrides the auto-generated display. Original data is not modified.

### Message view

When a conversation is selected:

```
┌──────────────────────┬──────────────────────────────────────┐
│  🔍 Global search    │  Bob Smith                           │
│                      │  +1 555-1234 · SMS · 142 messages    │
│  SAVED GROUPS        │  Jan 2019 – Aug 2026  [Photos]      │
│  ...                 ├──────────────────────────────────────┤
│                      │                                      │
│  ▸ Conversations      │  [Find: _________ ↑↓]                │
│  ▸ Contacts          │                                      │
│  ▸ Trash             │                                      │
│                      │  ┌──────────────────────────────┐    │
│  ────────────────    │  │ Message bubble               │    │
│                      │  │ Aug 6, 2020 2:34 PM          │    │
│  ┌────────────────┐  │  └──────────────────────────────┘    │
│  │  📥 Import     │  │                                      │
│  └────────────────┘  │  ┌──────────────────────────────┐    │
│  ┌────────────────┐  │  │ Reply bubble                 │    │
│  │  📤 Export     │  │  │ Aug 6, 2020 2:35 PM          │    │
│  └────────────────┘  │  └──────────────────────────────┘    │
│                      │                                      │
│  ────────────────    │                                      │
│                      │                                      │
│  👤 Profile          │                                      │
│  ⚙ Settings         │                                      │
└──────────────────────┴──────────────────────────────────────┘
```

- **Header**: Participant name/handle, participant chips (clickable — opens profile drawer), message count, date range, Photos & Files shortcut
- **Find bar**: Appears when search is active. Highlights matches in visible messages with next/prev arrows
- **Messages**: Full-width, service-specific rendering (iMessage reactions vs. Discord embeds vs. SMS bubbles)
- **No permanent right panel** — metadata lives in the header, participant details in a drawer
- **Pagination**: Messages are loaded in pages from the API (`offset`/`limit`). The header shows "Messages 1–50 of 1,423" with prev/next controls. Date jump links resolve to the correct page offset. No giant DOM — only the visible page is rendered.

### Participant contact view

Clicking a participant name or chip opens a slide-over drawer. This is a contact view, not a conversation browser — the user is "who is this person?" not "what conversations are they in?"

- **Display name**: Click to inline edit. Renaming here updates the name everywhere this person appears
- **Handles by service with date ranges**: Shows which handles the person uses, which service each belongs to, and the date range that handle was active. This gives the user context about how they communicate with this person over time:

```
+1 555-1234 · SMS       2019–2026 · 1,203 messages
bob#1234 · Discord      2021–2023 · 450 messages
@bob.ig · Instagram     2023–present
```

- **Message counts per handle**: Shown for direct messages only. Group message stats are not attributed to individual participants — the count would be misleading
- **Group membership**: "3 group conversations" — just a count, not per-handle stats
- **Sources** (optional expand): Which backups contributed data for this person, and which handles came from which source

Adding a new handle triggers matching: "We found 3 conversations matching bob#1234 on Discord"

### My Profile (the vault owner)

Separate from the participant contact view. Accessed from the "Profile" link in the bottom of the left panel:

- Display name
- My handles (phone, email, Discord, Instagram, etc.) — used to match imported messages to the owner
- Account settings: change password, manage sessions
- Storage usage (how many messages, attachments, conversations in the vault)
- Delete account

The profile is created at onboarding, not hidden in Settings. Editing handles here is the same UI as the contact view.

### Search

The existing message-vault-rs search system is the foundation — port it to the unified GUI and iterate, don't rebuild.

**Search bar** (left panel header): Operator-based query input with autocomplete for contacts and labels. Supports `from:`, `to:`, `with:`, `within:`, `label:`, `handle:`, `has:`, `date:`, `source:` operators.

**Advanced search form** (dropdown from search bar): Two tabs:
- **Messages** (default): from, to, with person (expandable for first/last name, phone), has/doesn't have words, subject, date range, message type (all/direct/group), source, attachment filter (type, filename, size), results grouping, sort, context
- **Contacts**: handle (expandable for first/last name, phone), first/last message date range, group/direct message counts

**Search results**: Replace the main view area. Grouped by conversation by default (one row per conversation, showing matching message count + snippet). Click a result → opens that conversation with the find bar pre-populated.

**Find bar** (message view): Highlights all matches in the visible page. Next/prev arrows navigate between matches. If arrived from search results, pre-filled with the search term. User can type a new term to search within the current conversation.

Flow: search "vacation" → results show 3 conversations → click one → find bar shows "vacation", click next twice → type "hotel" → now searching within this conversation.

### Contacts list

Toggleable from the left panel under "Contacts." A flat list of all people in the vault — names, handle count, last message date. Sortable by name or recency. Click a row → contact view drawer. This is for discovery ("who's in my vault?") not for navigating to messages.

### Saved groups (dynamic labels)

The left panel has a "Saved Groups" section above the conversation list. Each saved group is a named search query:

| Group name | Query |
|------------|-------|
| Work team | `from:bob or carol service:discord` |
| Family | `participants:bob,carol,ted,alice` |
| 2023 Archive | `date:2023` |
| Videos | `has:attachment type:video` |

Clicking a saved group filters the conversation list to matching conversations. No manual labeling of messages — the group is a saved query. Users can create, rename, reorder, and delete saved groups. Messages are imported data and cannot be edited to add tags.

### Desktop-only actions

Four desktop actions, divided by whether they need vault authentication:

**Require auth** (left panel buttons, visible when logged in):
- **Import**: Combined extract + push. Left panel button, replaces main view with import form.
- **Export**: Pull from vault to local files. Left panel button, popover for scope selection.

**No auth required** (login screen, below the auth form):
- **Extract**: Parse backup to JSONL without connecting to a vault. For offline use.
- **Format**: Convert between output formats. Also accessible from the authenticated view via a Tools menu.

All four are gated behind `isTauri()` and hidden in the web deployment. The web deployment shows only the auth form — no offline tools, no import/export.

## Service-specific message rendering

Different services have different message formats. The message view renders service-specific UI:

- **SMS/MMS**: Standard bubbles, delivery status
- **iMessage**: Tapbacks, reactions, effects, edit history
- **WhatsApp**: Reply chains, media captions, deleted message indicators
- **Discord**: Embeds, threads, reactions, role colors
- **Instagram**: Story replies, media-forwarding indicators

The conversation schema already tracks `service` — the renderer uses this to pick the appropriate component.

## Import flow (desktop only)

Import combines extraction and push into a single operation. The user picks a source, provides a backup path, optionally provides a contacts file, and hits Import. Extraction, dedup, and push run as a pipeline — progress is shown inline.

The import button lives in the left panel sidebar. Clicking it switches the main view area to the import form. No wizard navigation — it's one scrollable form.

### Import steps

**1. Source type:** Dropdown to pick the backup source (iMessage, WhatsApp, SMS Backup & Restore, GO SMS Pro, iMazing, SMS Backup+, OpenExtract).

**2. Backup path:** File/directory picker for the backup location. Source-specific options appear below (e.g., WhatsApp platform, Apple platform, backup password).

**3. Contacts (optional):** File picker for a VCF or vCard CSV file. Parsed and compared against existing vault contacts.

**4. Conflict review:** If a contacts file is provided, show side-by-side comparison:

| Contact file name | Vault name | Handle | Action |
|-------------------|------------|--------|--------|
| Bob Smith | Bobby Smith | +1 555-1234 | [Use file] [Use vault] [Edit] |
| Mom | — | (none) | [Add handle: ___] |

- If the vault has a matching name for a handle, auto-suggest linking
- Contacts without handles appear as unmatched — user can add a handle or skip
- The goal is to get names and handles right at import time, not clean up later

**5. Progress:** A linear step indicator showing current phase:

```
Parsing backup… 1,423 messages found         ✓
Converting attachments… 12 of 45             ⏳
Uploading to vault… 89%                      ○
```

Primary view shows the high-level steps. A "Show details" toggle reveals the raw extraction/push log underneath.

**6. Done:** Summary — messages imported, conversations created, duplicates skipped, attachments uploaded. Option to import another backup or return to the conversation list.

### Contacts handling

- **Full merge with conflict resolution**: Vault contacts and the provided contacts file are merged. The import review step lets the user resolve conflicts before data enters the vault.
- **Auto-suggest**: If the vault has a contact named "Mom" with a phone number, suggest linking an unmatched name to that handle.
- **Unmatched names**: Shown with a prompt to add a handle. Can be skipped — the name is stored but won't link to conversations.

## Sources and deduplication

When a conversation is reconstructed from multiple backups:

- The primary view shows the clean combined timeline
- A "Sources" panel (accessible from the conversation header) shows backup provenance:
  - Backup A: 10,000 messages (80%)
  - Backup B: 3,000 messages, of which 2,500 are duplicates already in Backup A
  - Net contribution: Backup A 10,000 + Backup B 500 unique = 10,500 total
- Individual messages can optionally show a source indicator for debugging

## Attachments (images, video)

- Thumbnails rendered inline in the message stream
- Click to expand to full-size viewer with lightbox navigation
- Video: inline player with play/pause/seek controls
- Attachments are streamed from the vault server API (or read from local filesystem in Tauri for un-uploaded data)

## Export (desktop only)

Export pulls data from the vault to local files. Accessed from the left panel sidebar. Three entry points:

- **Export entire vault**: Downloads everything the user has access to
- **Export from query**: The user runs a global search or opens a saved group, then hits Export — exports only the matching conversations
- **Ad-hoc selection**: The user selects specific conversations from the list (checkboxes) and exports those

All three paths lead to the same export form:

**1. Save location:** Directory picker for where files land.

**2. Format:** Dropdown — JSONL, JSON, CSV. Future: HTML (static vault snapshot).

**3. Progress:** Linear step indicator with collapsible detail log, same pattern as import.

The export button opens a popover with three options:
- Export entire vault
- Export current view (available when a saved group or search is active)
- Export selected (available when conversations are checked)

The most likely choice is pre-selected. The popover prevents accidentally exporting everything.
Export is not expected to be a high-frequency action — two clicks is acceptable.

## Trash and deletion

Three levels of deletion:

| Unit | What happens | Trash display |
|------|-------------|---------------|
| Message | Message moves to trash. Conversation remains. | "Conversation with Bob — 1 message" |
| Conversation | All messages move to trash. Conversation metadata preserved. | "Conversation with Bob — 10,000 messages" |
| Contact | Profile and name mapping removed. Messages remain — they revert to showing raw handles. | "Contact Bob Smith" |

### Trash is conversation-grouped

The trash view always shows conversations as containers. What varies is the message count inside. This avoids deduplication problems — you never have individual messages and conversation deletes overlapping.

**Restore logic:**
- If the conversation still exists → merge restored messages back in
- If the conversation was fully deleted → recreate it from preserved metadata, then restore messages

No partial restores. No orphaned trash entries. The conversation is the container — restore always targets the right place.

**Empty trash** removes everything permanently.

**Bulk operations:** Deleting 10,000 messages in a conversation and undoing shows one row — "Conversation with Bob — 10,000 messages — deleted 5 minutes ago." Restore brings the whole conversation back.

## Settings

Accessed from the ⚙ icon in the left panel. Replaces the main view area.

**Vault connection:**
- Server URL
- Authentication (Hanko vs. local username/password)
- Connection test button

**Media:**
- ffmpeg path: Text field with Browse button. Defaults to system PATH. If ffmpeg is not found at startup, the app shows a notification with a link to documentation. Same model as VS Code's clangd path — system default unless overridden.
- Link to documentation for per-platform ffmpeg install instructions

**Appearance:**
- Theme (light / dark / system)
- Date/time format preferences

**Storage:**
- Conversation count, message count, attachment storage used
- Export all data link

**Account:**
- Change password
- Delete account (with confirmation)

## Import history

Accessible from the Import button dropdown or the Settings screen. A chronological list of past imports:

| Date | Source | Messages | Attachments | Size | Conversations | Duplicates |
|------|--------|----------|-------------|------|---------------|------------|
| Aug 6, 2026 2:34 PM | iMessage (iOS) | 14,203 | 342 | 1.2 GB | 87 | 312 |
| Aug 4, 2026 9:15 AM | WhatsApp (Android) | 3,400 | 56 | 89 MB | 22 | 0 |

Helps answer "did I already import that backup?" before re-importing. Click a row to see the per-conversation breakdown from that import session.

## What this design does NOT cover

- Excluded conversations / spam filtering (marking conversations to exclude from views)
- Contact editing and merging UI (combining two contact profiles)
- New server endpoints: `GET /api/auth/mode`, import history API, paginated message queries

## Open questions

None — all design decisions are resolved. Search syntax is defined by the existing message-vault-rs system. Query syntax for saved groups inherits from the search bar operators.
