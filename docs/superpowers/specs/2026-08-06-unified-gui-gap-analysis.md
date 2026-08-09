# Unified GUI — Gap Analysis: Spec vs. Code

**Date:** 2026-08-06
**Compared against:** `docs/superpowers/specs/2026-08-06-unified-gui-design.md`
**Excluded:** Login / registration

---

## Summary

The six implementation plans have been partially executed. All files exist, but several are stubs or not wired. Roughly **22 items remain across 3 tiers**, estimated at **10–16 days of work** (excluding login/registration).

---

## Plan Completion Status

### Plan 1 — Server API: ✅ 100%

All three new endpoints exist in `message-vault-rs`:
- `GET /v1/auth/mode` — auth mode discovery
- `GET /v1/export/messages?offset=N&limit=N` — offset pagination
- `GET /v1/imports` — import history listing

Bonus endpoints also present: `GET /v1/auth/check`, `GET/POST /v1/account/profile`.

### Plan 2 — App Shell: ✅ 100%

API client, auth context (with localStorage persistence + token validation), login screen (with Hanko integration), register screen, left panel, app layout, `isTauri()` helper — all built, wired, and functional.

### Plan 3 — Conversations & Messages: ✅ ~95%

Types, ConversationList, ConversationRow (direct/small-group/large-group display logic), MessageView with offset pagination, MessageBubble, PaginationBar — all built and wired.

ConversationRow display logic matches the design (DM / small-group / large-group title + subtitle, including date range on large-group titles).

### Plan 4 — Search, Contacts, Saved Groups: ⚠️ ~50%

| Item | Status |
|------|--------|
| GlobalSearch.tsx (operator autocomplete) | ❌ Missing |
| AdvancedSearchForm.tsx | ❌ Missing |
| ContactList.tsx | ✅ Done |
| ContactDrawer.tsx (read-only) | ✅ Done |
| savedGroups.ts (localStorage module) | ✅ Done |
| SavedGroupForm.tsx | ❌ Missing |
| Saved groups wired into LeftPanel | ❌ Static "No saved groups yet" placeholder |

### Plan 5 — Desktop Features: ⚠️ ~55%

| Item | Status |
|------|--------|
| ImportScreen.tsx | ✅ Exists — but uses `setTimeout` placeholders |
| ExportScreen.tsx | ✅ Exists — but uses `setTimeout` placeholders |
| StepProgress.tsx | ✅ Done |
| Extract.tsx adapted with `onBack` prop | ❌ Still uses old `({ onError })` signature |
| Format.tsx adapted with `onBack` prop | ❌ Still uses old `({ onError })` signature |
| LoginScreen offline buttons wired | ❌ Buttons have no `onClick` handlers |

### Plan 6 — Trash, Settings, Profile, Integration: ⚠️ ~55%

| Item | Status |
|------|--------|
| TrashScreen.tsx | ✅ Done (but no trash API endpoints in server) |
| SettingsScreen.tsx | ✅ Done |
| ProfileScreen.tsx | ✅ Done |
| All screens wired in AppLayout | ✅ Done |
| Docker: axum serves Vite build | ❌ `tower-http` missing `fs` feature, no `ServeDir` |
| Docker: Next.js removed | ❌ `message-vault-rs/web/` still fully present |
| Docker: entrypoint scripts | ❌ Still start Next.js process on port 3000 |

---

## Spec Features Not Covered by Any Plan or Code

These are called for in the spec but have no corresponding implementation at all.

| # | Feature | Spec Section |
|---|---------|-------------|
| 1 | **Participant chips in message header** — clickable, opens contact drawer | Message view |
| 2 | **Find bar match highlighting** with next/prev arrows | Message view |
| 3 | **Service-specific message rendering** (iMessage reactions, Discord embeds, WhatsApp reply chains, Instagram stories) | Service-specific rendering |
| 4 | **Attachment thumbnails** inline + lightbox viewer + video player | Attachments |
| 5 | **Inline name editing** in contact drawer ("Click to inline edit") | Participant contact view |
| 6 | **Handle matching on add** ("We found 3 conversations matching bob#1234 on Discord") | Participant contact view |
| 7 | **Sources panel** — backup provenance from conversation header | Sources and dedup |
| 8 | **Onboarding profile creation** — display name, handles, photo at signup | Onboarding |
| 9 | **Import contacts conflict review** — side-by-side comparison table | Import flow |
| 10 | **Import history screen** — chronological list of past imports (API exists, no UI) | Import history |
| 11 | **Export popover** — three scope options from left panel button | Export |
| 12 | **Conversation checkboxes** — for ad-hoc export selection | Export |
| 13 | **Search results view** — grouped by conversation with snippets | Search |
| 14 | **Date jump links** in message view | Message view |
| 15 | **Local rename UI** for conversations (`label` field exists in type, no UI to edit it) | Conversation rows |
| 16 | **Delete account** in Settings | Settings |
| 17 | **Change password** in Settings | Settings |
| 18 | **Storage usage stats** in Settings | Settings |
| 19 | **Global search** with operator autocomplete (`from:`, `to:`, `has:`, etc.) | Search |
| 20 | **Advanced search form** (Messages/Contacts tabs dropdown) | Search |
| 21 | **Saved groups UI** — create, rename, reorder, delete in left panel | Saved groups |
| 22 | **Docker integration** — serve Vite `dist/`, remove Next.js entirely | Architecture |

---

## Work Tiers

### Tier 1 — Finish the Plans (what was supposed to be done): ~2–3 days

Already written as an implementation plan: `docs/superpowers/plans/2026-08-06-tier-1-finish-plans.md`

1. Wire saved groups into LeftPanel + build SavedGroupForm
2. Wire login offline tool buttons (Extract/Format with back navigation)
3. Wire real Tauri extract + API push into ImportScreen
4. Wire real API pull into ExportScreen
5. Docker integration (ServeDir, update Dockerfiles, remove Next.js)

### Tier 2 — Missing Planned Components: ~3–5 days

6. GlobalSearch component with operator autocomplete
7. AdvancedSearchForm (Messages/Contacts tabs)
8. Import history screen
9. Find bar match highlighting + next/prev arrows
10. Participant chips in message header (clickable → opens drawer)
11. Export popover with three scope options
12. Conversation checkboxes for ad-hoc export selection
13. Import contacts conflict review UI (side-by-side comparison table)
14. Delete account / change password / storage stats sections in Settings

### Tier 3 — Spec Features with No Plan: ~5–8 days

15. Service-specific message rendering (5+ services: SMS, iMessage, WhatsApp, Discord, Instagram)
16. Attachment thumbnails + lightbox viewer + video player
17. Inline name editing in contact drawer
18. Handle matching on-add flow ("We found 3 conversations matching…")
19. Sources/backup provenance panel
20. Onboarding profile creation flow (after registration)
21. Search results grouped-by-conversation view with snippets
22. Date jump links in message view
23. Local rename UI for conversations

---

## Related Plans

- `2026-08-06-tier-1-finish-plans.md` — immediate next work (2–3 days)
- `2026-08-06-unified-gui-plan-1-server-api.md` through `plan-6-trash-settings-profile.md` — the original 6 plans
- `2026-08-06-unified-gui-design.md` — the source spec
