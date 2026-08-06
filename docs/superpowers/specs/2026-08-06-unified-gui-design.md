# Unified GUI Design

**Date**: 2026-08-06
**Status**: draft (in progress)

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
└──────────────────────────────────┘
```

Existing vault users enter their server URL and authenticate (Hanko passkey or username/password). The SPA stores the auth token and navigates to the conversation list.

### Onboarding (new user)

New user flow: Create account → Create profile. The profile is created at signup, not hidden in Settings later:

- Display name
- Handles (phone, email, Discord, Instagram, etc.) — used to match imported messages to the owner
- Profile photo

### Home — Conversation list

After login, the user sees a flat conversation list sorted by most recent message timestamp:

```
┌─────────────────────────────────────────────────────┐
│  [Global search]                    [Profile] [⚙]   │
├──────────────┬──────────────────────────────────────┤
│ Saved Groups │                                      │
│ Work team    │   Select a conversation to view      │
│ Family       │   messages, or search to find        │
│ 2023 Archive │   someone.                           │
│ Videos       │                                      │
│              │                                      │
│ All          │                                      │
│ Trash        │                                      │
├──────────────┤                                      │
│ Conversations│                                      │
│              │                                      │
│ Bob Smith    │                                      │
│ yesterday     │                                      │
│              │                                      │
│ Bob, Carol   │                                      │
│ + 17 others  │                                      │
│ Sep 2020 –    │                                      │
│ Jan 2022     │                                      │
│ 214 msgs     │                                      │
│              │                                      │
├──────────────┴──────────────────────────────────────┤
│  [Extract] [Format] [Import] [Export]  — desktop only│
└─────────────────────────────────────────────────────┘
```

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
┌─────────────────────────────────────────────────────┐
│  [Global search]                    [Profile] [⚙]   │
├──────────────┬──────────────────────────────────────┤
│ Conversations│  ┌──────────────────────────────────┐ │
│              │  │ Bob Smith                        │ │
│  Bob Smith   │  │ +1 555-1234 · SMS  142 messages │ │
│              │  │ Jan 2019 – Aug 2026 [Photos]    │ │
│  ...         │  ├──────────────────────────────────┤ │
│              │  │                                  │ │
│              │  │  [Find bar: _________ ↑↓]        │ │
│              │  │                                  │ │
│              │  │  ┌──────────────────────────┐   │ │
│              │  │  │ Message bubble           │   │ │
│              │  │  │ Aug 6, 2020 2:34 PM      │   │ │
│              │  │  └──────────────────────────┘   │ │
│              │  │                                  │ │
│              │  │  ┌──────────────────────────┐   │ │
│              │  │  │ Reply bubble             │   │ │
│              │  │  │ Aug 6, 2020 2:35 PM      │   │ │
│              │  │  └──────────────────────────┘   │ │
│              │  │                                  │ │
│              │  └──────────────────────────────────┘ │
├──────────────┴──────────────────────────────────────┤
│  [Extract] [Format] [Import] [Export]  — desktop only│
└─────────────────────────────────────────────────────┘
```

- **Header**: Participant name/handle, participant chips (clickable — opens profile drawer), message count, date range, Photos & Files shortcut
- **Find bar**: Appears when search is active. Highlights matches in visible messages with next/prev arrows
- **Messages**: Full-width, service-specific rendering (iMessage reactions vs. Discord embeds vs. SMS bubbles)
- **No permanent right panel** — metadata lives in the header, participant details in a drawer
- **Jump links**: Date-based quick-jump anchors at the top (from the old GUI) — but consider virtualized rendering to avoid one giant DOM

### Participant profile drawer

Clicking a participant name or chip opens a slide-over drawer:

- Display name (editable)
- Handles across services
- Conversations with this person (direct + groups they're in)
- Last message timestamp, total messages
- Sources breakdown (which backups contributed to this person's data)

### Search

**Global search** (left panel header): Searches all conversations. Results list shows conversation name + matching snippet. Click a result → opens that conversation, jumps to the message, and the find bar auto-populates with the search term.

**Find bar** (message view): Highlights all matches in the visible messages with next/prev arrows. If arrived from global search, pre-filled with that term. User can type a new term to search within the current conversation.

Flow: global search "vacation" → click result → find bar shows "vacation", click next twice → type "hotel" → now searching for something else in the same conversation.

### Saved groups (dynamic labels)

The left panel has a "Saved Groups" section above the conversation list. Each saved group is a named search query:

| Group name | Query |
|------------|-------|
| Work team | `from:bob or carol service:discord` |
| Family | `participants:bob,carol,ted,alice` |
| 2023 Archive | `date:2023` |
| Videos | `has:attachment type:video` |

Clicking a saved group filters the conversation list to matching conversations. No manual labeling of individual messages — the group is the saved query. Users can create, rename, reorder, and delete saved groups.

If the user wants to tag specific messages, they add a keyword like `#family` anywhere in a message body and save a search for `#family`.

### Desktop-only actions

```
┌─────────────────────────────────────────────────────┐
│  [Extract] [Format] [Import] [Push] [Pull]          │
│  Hidden in web deployment (isTauri() === false)     │
└─────────────────────────────────────────────────────┘
```

These screens are gated behind `isTauri()`:

- **Extract**: Parse phone backups, convert to JSONL
- **Format**: Convert between output formats
- **Import**: Push extracted messages to the vault server (requires auth)
- **Push**: Upload data to vault (requires auth)
- **Pull**: Download data from vault (requires auth)

Import and push/pull operations require authentication — the auth token is obtained at login and passed to the vault API.

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

## What this design does NOT cover (yet)

- Trash and recovery workflow
- Settings (server config, theme, storage management)
- Excluded conversations / spam filtering
- Contact editing and merging UI
- Import progress and status dashboard
- The existing extract/format/push/pull screens from the current Tauri app (these carry forward as-is until iterated on)

## Open questions

- How to display conversation backlinks in the participant profile drawer (list of groups + direct)
- Exact search query syntax for saved groups
- Virtualized message list implementation (to replace the "load everything and fast-scroll" approach)
- How the existing Tauri extract/format/push/pull screens integrate into the bottom bar
