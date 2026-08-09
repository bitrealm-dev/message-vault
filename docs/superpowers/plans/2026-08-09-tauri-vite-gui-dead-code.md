# Tauri / Vite GUI Dead-Code Removal Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Delete orphan and unreachable Vite GUI screens/wiring so the tree matches what users can open, without changing reachable behavior.

**Architecture:** Remove seven unused screen modules; slim `AppLayout` and `LeftPanel`; make `ExportScreen` always export the entire vault; drop unused `initialFindTerm` on `MessageView`.

**Tech Stack:** React 19 + TypeScript + Vite (`web/`)

## Global Constraints

- Scope is `web/` only — do not change `web-next/` or Tauri Rust command registration.
- Keep `Extract.tsx` and `Format.tsx` (Login offline tools).
- Preserve conversation-list filtering search behavior.
- Behavior of reachable screens must stay the same.

**Spec:** `docs/superpowers/specs/2026-08-09-tauri-vite-gui-dead-code-design.md`

## Files

| Action | Path |
|--------|------|
| Delete | `web/src/screens/Home.tsx`, `Push.tsx`, `Pull.tsx`, `Contacts.tsx`, `Settings.tsx`, `SearchResults.tsx`, `ImportHistoryScreen.tsx` |
| Modify | `web/src/components/AppLayout.tsx`, `LeftPanel.tsx`, `web/src/screens/ExportScreen.tsx`, `MessageView.tsx` |

---

### Task 1: Delete orphan and unreachable screens

**Files:**
- Delete: the seven paths listed above

- [ ] **Step 1: Delete the files**

```bash
rm web/src/screens/Home.tsx \
   web/src/screens/Push.tsx \
   web/src/screens/Pull.tsx \
   web/src/screens/Contacts.tsx \
   web/src/screens/Settings.tsx \
   web/src/screens/SearchResults.tsx \
   web/src/screens/ImportHistoryScreen.tsx
```

- [ ] **Step 2: Confirm no remaining imports**

```bash
rg -n 'Home|Push|Pull|Contacts|SearchResults|ImportHistoryScreen|screens/Settings' web/src --glob '*.tsx' --glob '*.ts'
```

Expected: only live names (`SettingsScreen`, `ContactList`, `invokePush` in Import/Export, etc.) — no imports of deleted modules.

---

### Task 2: Slim AppLayout

**Files:**
- Modify: `web/src/components/AppLayout.tsx`

- [ ] **Step 1: Remove dead imports and state**

Remove imports of `ImportHistoryScreen` and `SearchResults`.

Remove state: `searchActive`, `findTerm`, `exportScope`.

Remove `handleSelectResult`.

In `handleNavigate`, `handleSearch`, `handleSearchChange`, and `handleBrowseContactConversations`, remove all `setSearchActive(...)` calls.

- [ ] **Step 2: Simplify listContent**

Replace the `searchActive && ... ? <SearchResults ...> : <ConversationList ...>` ternary with only `ConversationList`.

- [ ] **Step 3: Simplify mainContent**

Remove `case "import-history"`.

Change export case to `<ExportScreen />` (no props).

Render `MessageView` without `initialFindTerm`.

---

### Task 3: Single Export button in LeftPanel

**Files:**
- Modify: `web/src/components/LeftPanel.tsx`

- [ ] **Step 1: Replace Export popover with one button**

Remove `showExportPopover` state and the popover markup.

Use:

```tsx
<button style={linkStyle("export")} onClick={() => onNavigate("export")}>
  Export
</button>
```

same pattern as Import.

---

### Task 4: Simplify ExportScreen and MessageView

**Files:**
- Modify: `web/src/screens/ExportScreen.tsx`
- Modify: `web/src/screens/MessageView.tsx`

- [ ] **Step 1: ExportScreen — entire vault only**

Delete `ExportScope` type, `scope` / `selectedCount` props, and `scopeLabel` branching.

Use fixed copy: `Exporting entire vault`.

For Tauri pull, always pass `query: ""`.

Component signature:

```tsx
export default function ExportScreen() {
```

- [ ] **Step 2: MessageView — drop initialFindTerm**

Remove `initialFindTerm` from props and the `useEffect` that seeds `findTerm` from it. Keep the in-thread find box.

---

### Task 5: Verify and commit

- [ ] **Step 1: Build**

```bash
cd web && npm run build
```

Expected: `tsc` and Vite build succeed.

- [ ] **Step 2: Grep deleted modules**

```bash
rg -n 'from ["''].*/(Home|Push|Pull|Contacts|Settings|SearchResults|ImportHistoryScreen)["'']' web/src
```

Expected: no matches.

- [ ] **Step 3: Commit**

```bash
git add web/src docs/superpowers/plans/2026-08-09-tauri-vite-gui-dead-code.md
git commit -m "$(cat <<'EOF'
refactor(web): remove unreachable Tauri/Vite GUI screens

Delete orphan tab screens and dead AppLayout/LeftPanel wiring so the
tree matches what users can open. Export always means entire vault.
EOF
)"
```
