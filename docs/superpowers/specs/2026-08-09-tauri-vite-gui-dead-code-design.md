# Tauri / Vite GUI dead-code removal

**Date:** 2026-08-09  
**Status:** approved  
**Scope:** `web/` (Vite React SPA). No changes to `web-next/`, and no new Tauri commands.

## Problem

The unified GUI still contains screens and navigation branches from the older desktop-tab layout. Several screens are never imported by the live shell. Others are imported but cannot open from any control the user can click. That makes the tree harder to read and invites mistaken edits to dead paths.

This pass removes dead UI and wiring only. Behavior of every screen that is reachable today stays the same.

## Approach

Option **B** from the simplification brainstorm:

1. Delete orphan screen files that nothing imports.
2. Delete screens that are only reachable through dead `AppLayout` state.
3. Remove the Export popover that offers three scopes but always opens the same screen.
4. Leave Extract / Format, shared job-hook extraction, and large-file splits for later plans.

## Files to delete

| File | Reason |
|------|--------|
| `web/src/screens/Home.tsx` | Old home hub for Extract / Format / Push / Pull tabs. Nothing imports it. |
| `web/src/screens/Push.tsx` | Standalone vault push form. Import already runs push via Tauri. |
| `web/src/screens/Pull.tsx` | Standalone vault pull form. Export already runs pull via Tauri. |
| `web/src/screens/Contacts.tsx` | Old Tauri `contacts_info` / VCF inspector. Live contacts UI is `ContactList.tsx`. |
| `web/src/screens/Settings.tsx` | Old `export.ini` defaults editor. Live settings UI is `SettingsScreen.tsx`. |
| `web/src/screens/SearchResults.tsx` | Message-search results panel. `AppLayout` never sets `searchActive` to true because that path called an unsupported API and blanked the list. |
| `web/src/screens/ImportHistoryScreen.tsx` | Full-page import history. No sidebar entry. Settings → Storage already lists imports from `/v1/imports`. |

### Explicitly keep

- `Extract.tsx` and `Format.tsx` — LoginScreen still opens these for offline desktop work when Tauri is available.
- `SettingsScreen.tsx`, `ContactList.tsx`, `ImportScreen.tsx`, `ExportScreen.tsx`, `ProfileScreen.tsx` (for `ProfileSettingsPanel`).

## `AppLayout` changes

File: `web/src/components/AppLayout.tsx`

Remove:

- Import of `SearchResults` and `ImportHistoryScreen`.
- State: `searchActive`, `findTerm`, `exportScope`.
- Handler: `handleSelectResult` (only SearchResults used it).
- List-column branch that rendered `SearchResults` when `searchActive` was true.
- `switch` case `"import-history"`.
- Passing `initialFindTerm={findTerm}` into `MessageView`.

Keep conversation search as it works today: typing or submitting a query filters `ConversationList` (and contact search still switches to the contacts list). Do not restore a separate message SearchResults panel in this pass.

After removal, `MessageView` may still declare an optional `initialFindTerm` prop. If nothing passes it, remove that prop and the effect that seeds the in-thread find box from it. The in-thread find box itself stays; users can still type a find term inside a conversation.

## Export navigation

File: `web/src/components/LeftPanel.tsx`

Today the Export control opens a popover with three buttons:

- Export entire vault
- Export current view
- Export selected

All three call `onNavigate("export")`. `AppLayout` always passed `scope="all"` and `selectedCount={0}`, so the other two labels did nothing different.

Change:

- Replace the popover with a single **Export** button that navigates to `"export"`, matching the Import button pattern.
- Call `ExportScreen` without fake scope choices. Simplify `ExportScreen` so it always exports the entire vault: delete the `scope` and `selectedCount` props and the unused scope-label branches.

## Out of scope

- Extracting a shared job / progress hook for Extract, Format, Push, Pull, or Import.
- Splitting large files (`ImportScreen`, `ProfileScreen`, `SettingsScreen`, `ThemeSettings`, `ContactDrawer`).
- Introducing React Router.
- Re-implementing message search against a supported API.
- Changing `src-tauri` command registration (push/pull commands remain for Import/Export).
- Any work under `web-next/`.

## Success criteria

1. `cd web && npm run build` succeeds.
2. Typecheck / lint for touched files is clean if the package scripts provide them.
3. Manual smoke (desktop / Tauri when available):
   - Login offline: Extract and Format still open and return via Back.
   - Authenticated: Conversations, Contacts, Trash, Import, Export, Settings still open.
   - Conversation search still filters the list (no blank SearchResults panel).
   - Settings → Storage still shows import history.
4. `rg` under `web/src` finds no imports of the deleted screen files.
5. No user-visible behavior change on paths listed in (3).

## Follow-up plans (not this work)

- Shared Tauri job runner for Extract / Format (and optionally Import progress).
- Split oversized screens for readability.
- Real message-search results UI once the vault API supports the shape SearchResults expected.
