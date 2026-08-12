# Web SPA Sequenced Cleanup Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the Vite SPA under `web/` easier to change by deleting dead surface area, tightening TypeScript boundaries, extracting shared shells/hooks, and splitting the largest screens — without changing product behavior users already rely on.

**Architecture:** Work proceeds in six ordered passes. Each pass is independently mergeable: green `cd web && npm run lint && npm test && npm run build`, no `web-next/` edits, and reachable UI behavior stays the same unless a pass explicitly fixes a broken path (Pass 2 message deep-link). Prefer extract-then-delete over rewrite-in-place.

**Tech Stack:** React 19, TypeScript, Vite 6, Vitest, existing `web/src` components (`Button`, `ModalShell`, `FormRow`, `useTauriJob`, `usePagedList`, `InfiniteOffsetList`).

**Source:** Code review of `web/` (2026-08-12) focused on consolidation, dead code, long functions, and TypeScript practice.

## Global Constraints

- Scope is `web/` and docs under `docs/superpowers/` only — **never modify `web-next/`**.
- Branch each pass from current `main`; ignore unrelated local WIP.
- After every task: `cd web && npm run lint && npm test && npm run build` must exit 0.
- Do not enable React Compiler ESLint rules in this plan.
- Do not add React Testing Library / jsdom unless a task explicitly requires a component test; prefer Vitest unit tests for pure helpers.
- Empty `/` and `/contacts` route elements in `App.tsx` are intentional (UI lives in `AppLayout`) — do not “fix” them by deleting routes.
- Keep `Extract.tsx` / `Format.tsx` reachable from Login offline tools.

## Pass map (ship as separate PRs)

| Pass | Name | Outcome |
|------|------|---------|
| 0 | Prep | Branch + verification baseline |
| 1 | Dead surface | Unused exports/props/scripts gone |
| 2 | Type boundaries | Safer casts; message deep-link loads by id |
| 3 | Shared shells | `TauriJobFormShell`, Select helper, async/resource hooks |
| 4 | Import job unification | Import uses same job lifecycle as Extract/Format/Export |
| 5 | Split giants | Import, AdvancedSearch, Storage, MessageView, Handles, Tokens |
| 6 | Domain consolidation | Profile hook, handle-service, list chrome, dates, bubbles |

Stop after any pass and merge. Later passes assume earlier ones landed.

---

## File map (new modules this plan introduces)

| Path | Responsibility | Introduced in |
|------|----------------|---------------|
| `web/src/lib/selectKey.ts` | Parse React Aria `Key` into allowed string unions | Pass 2 |
| `web/src/lib/messagesLocationState.ts` | Typed router state for message navigation | Pass 2 |
| `web/src/lib/authGuards.ts` | Narrow auth mode + persisted auth JSON | Pass 2 |
| `web/src/components/TauriJobFormShell.tsx` | Shared Extract/Format/Export layout + job chrome | Pass 3 |
| `web/src/lib/useAsyncAction.ts` | busy/error/run for auth (and similar) submits | Pass 3 |
| `web/src/lib/useResource.ts` | One-shot fetch with abort + loading/error/reload | Pass 3 |
| `web/src/lib/useAccountProfile.ts` | Shared `GET /v1/account/profile` | Pass 6 |
| `web/src/lib/handleService.ts` | `HandleService` union + option list | Pass 6 |
| `web/src/lib/formatDate.ts` | Shared day/month/unix formatters | Pass 6 |
| `web/src/components/FormField.tsx` | Inline or stacked label+control | Pass 6 |
| `web/src/screens/import/*` | Form / job hook / progress views | Pass 5 |
| `web/src/components/advancedSearch/*` | Widgets + query builder split from mega-form | Pass 5 |

---

### Task 0: Baseline on a clean branch

**Files:** none (verification only)

- [ ] **Step 1: Branch from main**

```bash
git fetch origin main
git checkout main
git pull --ff-only origin main
git checkout -b refactor/web-cleanup-pass-1-dead-surface
```

- [ ] **Step 2: Baseline checks**

```bash
cd web && npm ci && npm run lint && npm test && npm run build
```

Expected: all exit 0.

---

# Pass 1 — Dead surface

### Task 1.1: Remove dead ConversationList auto-select API

**Files:**
- Modify: `web/src/screens/ConversationList.tsx`
- Modify: any type-only exports of `ConversationAutoSelect` in the same file

**Interfaces:**
- Consumes: current list props used by `AppLayout` / `MessageRoute` (no `autoSelect` today)
- Produces: `ConversationList` props without `autoSelect` / `onAutoSelectDone`

- [ ] **Step 1: Confirm no callers**

```bash
rg -n 'autoSelect|ConversationAutoSelect|onAutoSelectDone' web/src
```

Expected: only definitions inside `ConversationList.tsx`.

- [ ] **Step 2: Delete the prop, type, ref, and effect** that implement auto-select (~25 lines). Keep list fetch, filter, and render behavior unchanged.

- [ ] **Step 3: Verify and commit**

```bash
cd web && npm run lint && npm test && npm run build
git add web/src/screens/ConversationList.tsx
git commit -m "$(cat <<'EOF'
refactor(web): remove unused ConversationList auto-select API

EOF
)"
```

---

### Task 1.2: Un-export module-private helpers and fix theme boot duplication

**Files:**
- Modify: `web/src/lib/theme.ts`, `web/index.html` (only if consolidating the boot script)
- Modify: `web/src/lib/system-settings.ts`, `web/src/lib/portaledOverlay.ts`, `web/src/lib/missingAttachmentLabel.ts`, `web/src/lib/contactDetailCache.ts`, `web/src/lib/contactRecentSearches.ts`, `web/src/lib/usePagedList.ts`, `web/src/lib/tauri.ts` as needed
- Modify: `web/src/lib/types.ts` only if removing *unused named exports* of interfaces that nothing imports by name (keep the interfaces themselves for nested typing)

**Interfaces:**
- Consumes: existing internal call sites inside each module
- Produces: fewer `export` keywords; public API limited to symbols other files import

- [ ] **Step 1: Inventory unused exports**

```bash
cd web && npx --yes knip --include exports 2>/dev/null || true
rg -n 'THEME_BOOT_SCRIPT|THEME_STORAGE_KEY|isPortaledOverlayTarget|attachmentDisplayName|setCachedContactDetail|PAGE_SIZE_FILL' web/src
```

- [ ] **Step 2: Theme boot script**

Choose one source of truth:

**Preferred:** Keep the inline script in `web/index.html` (must run before paint). Remove the unused `THEME_BOOT_SCRIPT` export from `theme.ts` (and `THEME_STORAGE_KEY` alias if knip flags it). Add a one-line comment in `theme.ts` pointing at `index.html` as the boot script location.

**Alternative (only if editing HTML is desirable):** Import a shared string into a tiny build step — do **not** do this unless already supported; prefer the Preferred path.

- [ ] **Step 3: Downgrade other unused exports to non-exported `function`/`const`/`type`** inside their defining modules. Do not delete logic that is still called internally.

Candidates (verify before changing):

- `theme.ts`: `isThemeMode`, `isResolvedTheme`, `parseStoredSeeds`, `prefersDarkScheme`, `activeSeeds` (if only used internally)
- `system-settings.ts`: storage key constants and helpers only used by `resolveImportStagingDir`
- `portaledOverlay.ts`: `isPortaledOverlayTarget` if only `shouldIgnoreOutsideDismiss` is imported elsewhere
- `missingAttachmentLabel.ts`: `attachmentDisplayName`
- `contactDetailCache.ts`: `setCachedContactDetail`
- `contactRecentSearches.ts`: `saveContactRecentSearches`, `CONTACT_RECENT_SEARCHES_MAX` if unused outside
- `usePagedList.ts`: `PAGE_SIZE_FILL`, unused `PagedFetchResult` export
- `tauri.ts`: `PushFinishedReport` if only used inside the module

- [ ] **Step 4: Verify imports still resolve**

```bash
cd web && npm run lint && npm test && npm run build
```

- [ ] **Step 5: Commit**

```bash
git add web/src/lib web/index.html
git commit -m "$(cat <<'EOF'
refactor(web): stop exporting module-private helpers

EOF
)"
```

---

### Task 1.3: Clarify trash list selection no-op

**Files:**
- Modify: `web/src/components/AppLayout.tsx` (trash `ConversationList` wiring)

**Interfaces:**
- Consumes: existing trash mode in `AppLayout`
- Produces: either non-interactive list chrome or documented intentional no-op

- [ ] **Step 1: Read trash branch** in `AppLayout` where `ConversationList` gets `onSelect={() => {}}`.

- [ ] **Step 2: Prefer the smaller fix** — if rows are not meant to open threads while trash is active, pass a prop like `selectionDisabled` (if easy) **or** leave the no-op but add a short comment that trash selection is handled only by `TrashScreen` in the outlet. Do **not** invent new trash navigation in this pass.

- [ ] **Step 3: Commit if code/comment changed**

```bash
git commit -am "$(cat <<'EOF'
refactor(web): document trash list selection no-op

EOF
)"
```

---

### Task 1.4: Merge Pass 1

- [ ] **Step 1: Final verify**

```bash
cd web && npm run lint && npm test && npm run build
```

- [ ] **Step 2: Merge to main** (local or PR — follow user instruction). Tag the PR title with `refactor(web): cleanup pass 1 dead surface`.

---

# Pass 2 — Type boundaries + message deep-link

Start branch: `refactor/web-cleanup-pass-2-types`.

### Task 2.1: Shared Select key parser

**Files:**
- Create: `web/src/lib/selectKey.ts`
- Create: `web/src/lib/selectKey.test.ts`
- Modify: call sites that do `as Source` / `as AuthMode`-style casts from Select keys:
  - `web/src/screens/ImportScreen.tsx`
  - `web/src/components/AdvancedSearchForm.tsx`
  - `web/src/screens/settings/ProfileSettingsPanel.tsx`
  - `web/src/screens/SettingsScreen.tsx`
  - `web/src/components/ThemeSettings.tsx`

**Interfaces:**
- Produces:

```ts
export function parseSelectKey<T extends string>(
  key: React.Key | null,
  allowed: readonly T[],
): T | null;
```

- [ ] **Step 1: Write failing tests** for valid key, unknown key, null.

- [ ] **Step 2: Implement `parseSelectKey`** — `String(key)`, then `allowed.includes` via type predicate.

- [ ] **Step 3: Replace `as` casts at Select `onSelectionChange` sites** with `parseSelectKey` + early return / ignore when null.

- [ ] **Step 4: Verify and commit**

```bash
cd web && npm test && npm run lint && npm run build
git add web/src/lib/selectKey.ts web/src/lib/selectKey.test.ts web/src/screens web/src/components
git commit -m "$(cat <<'EOF'
refactor(web): parse Select keys into allowed unions

EOF
)"
```

---

### Task 2.2: Messages location state helper

**Files:**
- Create: `web/src/lib/messagesLocationState.ts`
- Create: `web/src/lib/messagesLocationState.test.ts`
- Modify: `web/src/components/AppLayout.tsx`, `web/src/components/MessageRoute.tsx`

**Interfaces:**
- Produces:

```ts
export type MessagesLocationState = {
  conversation?: Conversation; // import type from lib/types
  openContactId?: string;
};

export function asMessagesLocationState(
  state: unknown,
): MessagesLocationState | null;
```

- [ ] **Step 1: Implement narrow helper** (object check; optional fields; do not deeply validate `Conversation` beyond “object with id” if that is what navigation always sets).

- [ ] **Step 2: Replace `location.state as …` in AppLayout and MessageRoute.**

- [ ] **Step 3: Verify and commit**

```bash
git commit -am "$(cat <<'EOF'
refactor(web): share messages location state narrowing

EOF
)"
```

---

### Task 2.3: Auth persistence and mode guards

**Files:**
- Create: `web/src/lib/authGuards.ts`
- Create: `web/src/lib/authGuards.test.ts`
- Modify: `web/src/lib/auth.tsx`, `web/src/screens/LoginScreen.tsx`

**Interfaces:**
- Produces: `isAuthMode(value: unknown): value is AuthMode`, `parsePersistedAuth(raw: string): Partial<AuthState> | null`

- [ ] **Step 1: Unit tests** for valid/invalid mode and corrupt JSON.

- [ ] **Step 2: Use guards in `auth.tsx` load path and Login mode handling.** Fall back to logged-out / unknown mode instead of trusting casts.

- [ ] **Step 3: Verify and commit**

```bash
git commit -am "$(cat <<'EOF'
fix(web): validate auth mode and persisted auth JSON

EOF
)"
```

---

### Task 2.4: Load conversation by URL id when state is missing

**Files:**
- Modify: `web/src/components/MessageRoute.tsx`
- Possibly Modify: `web/src/lib/api.ts` / existing conversation fetch helpers if one already exists
- Optional test: pure helper for “resolve conversation from state or fetch”

**Interfaces:**
- Consumes: route param `conversationId`, optional location state
- Produces: `MessageView` with a real conversation, or an error/empty state that is not a permanent lie

- [ ] **Step 1: When `state.conversation` is missing**, fetch conversation (or messages bootstrap) by `conversationId` from the API the app already uses for lists/details. Show a small loading state; on failure show an error string and a way back to `/`.

- [ ] **Step 2: Manual check** — open a conversation, copy URL, refresh. Thread should load.

- [ ] **Step 3: Verify and commit**

```bash
git commit -am "$(cat <<'EOF'
fix(web): load message view by conversation id on refresh

EOF
)"
```

---

### Task 2.5: Merge Pass 2

Same verify + merge pattern as Pass 1. Title: `refactor(web): cleanup pass 2 type boundaries`.

---

# Pass 3 — Shared shells and hooks

Branch: `refactor/web-cleanup-pass-3-shells`.

### Task 3.1: `useAsyncAction`

**Files:**
- Create: `web/src/lib/useAsyncAction.ts`
- Create: `web/src/lib/useAsyncAction.test.ts` (test the state machine with a fake async fn if practical; otherwise skip and rely on screen smoke)
- Modify: `web/src/screens/LoginScreen.tsx`, `RegisterScreen.tsx`, `OnboardingScreen.tsx`

**Interfaces:**
- Produces:

```ts
export function useAsyncAction(): {
  busy: boolean;
  error: string;
  run: (fn: () => Promise<void>) => Promise<void>;
  clearError: () => void;
};
```

- [ ] **Step 1: Implement hook** — set busy, clear/set error from `String(e)`, always clear busy in `finally`.

- [ ] **Step 2: Replace duplicated try/catch loading flags** on Login/Register/Onboarding. Use `PasswordField` on Login if Register already does.

- [ ] **Step 3: Verify and commit**

```bash
git commit -am "$(cat <<'EOF'
refactor(web): share useAsyncAction on auth screens

EOF
)"
```

---

### Task 3.2: `useResource`

**Files:**
- Create: `web/src/lib/useResource.ts`
- Modify first adopters (pick 2–3 only in this task): `web/src/screens/TrashScreen.tsx`, `web/src/components/SourcesPanel.tsx`, and/or `web/src/screens/settings/ApiTokensSection.tsx` list load

**Interfaces:**
- Produces:

```ts
export function useResource<T>(
  key: string | null,
  fetcher: (signal: AbortSignal) => Promise<T>,
): { data: T | null; loading: boolean; error: string; reload: () => void };
```

- [ ] **Step 1: Implement with AbortController** on key change / unmount.

- [ ] **Step 2: Adopt on Trash + Sources** so errors are visible (Trash currently swallows errors).

- [ ] **Step 3: Verify and commit**

```bash
git commit -am "$(cat <<'EOF'
refactor(web): add useResource for one-shot API loads

EOF
)"
```

---

### Task 3.3: `TauriJobFormShell`

**Files:**
- Create: `web/src/components/TauriJobFormShell.tsx`
- Modify: `web/src/screens/Extract.tsx`, `Format.tsx`, `ExportScreen.tsx`

**Interfaces:**
- Consumes: existing `useTauriJob`, `Button`, `ProgressBar`, `isTauri` patterns
- Produces:

```tsx
// Conceptual API — adjust names to match house style when implementing
type TauriJobFormShellProps = {
  title: string;
  children: React.ReactNode; // fields
  startLabel: string;
  onStart: () => void;
  onCancel: () => void;
  running: boolean;
  progress: /* same shape ProgressBar already expects */;
  error?: string | null;
  success?: React.ReactNode;
  requireTauri?: boolean;
};
```

- [ ] **Step 1: Extract shared layout** from Extract (simplest) into `TauriJobFormShell`.

- [ ] **Step 2: Rewire Format and Export** to the shell. Export keeps its finished toast/error content via `error`/`success` slots.

- [ ] **Step 3: Verify and commit**

```bash
git commit -am "$(cat <<'EOF'
refactor(web): extract TauriJobFormShell for Extract/Format/Export

EOF
)"
```

---

### Task 3.4: Merge Pass 3

Title: `refactor(web): cleanup pass 3 shared shells`.

---

# Pass 4 — Import job unification

Branch: `refactor/web-cleanup-pass-4-import-job`.

### Task 4.1: Extend job hook for sequenced extract→push

**Files:**
- Modify: `web/src/lib/useTauriJob.ts` (or Create `web/src/lib/useSequencedTauriJobs.ts` if extending would break callers)
- Modify: `web/src/screens/ImportScreen.tsx` (replace manual `awaitTauriJob` subscription path)

**Interfaces:**
- Consumes: existing Tauri event subscription used by `useTauriJob`
- Produces: ability to run phase 1 then phase 2 with progress callbacks and single cancel flag

- [ ] **Step 1: Read `useTauriJob` and Import’s `startImport`** (~204 lines). List the behaviors Import needs that the hook lacks (multi-phase, custom progress steps, staging path).

- [ ] **Step 2: Implement the smallest extension** that lets Import drop its duplicate subscription lifecycle. Prefer one hook module over two competing event listeners.

- [ ] **Step 3: Keep Import UI phases** (form / progress / done) in the screen for now; only unify job lifecycle in this pass.

- [ ] **Step 4: Manual smoke** — desktop Tauri import still completes; cancel still works.

- [ ] **Step 5: Verify and commit**

```bash
git commit -am "$(cat <<'EOF'
refactor(web): run Import through shared Tauri job lifecycle

EOF
)"
```

---

### Task 4.2: Merge Pass 4

Title: `refactor(web): cleanup pass 4 import job hook`.

---

# Pass 5 — Split giant files

Branch: `refactor/web-cleanup-pass-5-splits` (or one branch per file if reviews get large).

Do **behavior-preserving moves only**. No feature work.

### Task 5.1: Split `ImportScreen.tsx` (~667 lines)

**Files:**
- Create: `web/src/screens/import/ImportFormFields.tsx`
- Create: `web/src/screens/import/ImportProgressView.tsx`
- Create: `web/src/screens/import/useImportJob.ts` (if Pass 4 did not already)
- Modify: `web/src/screens/ImportScreen.tsx` → thin phase switcher

- [ ] **Step 1: Move form fields** without changing props/state names more than required.

- [ ] **Step 2: Move progress/done views.**

- [ ] **Step 3: Verify and commit**

```bash
git commit -am "$(cat <<'EOF'
refactor(web): split ImportScreen into form, job, and progress

EOF
)"
```

---

### Task 5.2: Split `AdvancedSearchForm.tsx` (~653 lines)

**Files:**
- Create: `web/src/components/advancedSearch/buildAdvancedQuery.ts`
- Create: `web/src/components/advancedSearch/ServiceMultiSelect.tsx` (move out)
- Create: `web/src/components/advancedSearch/DateBoundField.tsx` (and CountField if present)
- Create: `web/src/components/advancedSearch/MessagesSearchFields.tsx`
- Create: `web/src/components/advancedSearch/ContactsSearchFields.tsx`
- Modify: `web/src/components/AdvancedSearchForm.tsx` → state + submit only

- [ ] **Step 1: Extract pure query builder** + unit tests for token assembly (happy path + empty fields).

- [ ] **Step 2: Move widgets and field groups.**

- [ ] **Step 3: Verify and commit**

```bash
git commit -am "$(cat <<'EOF'
refactor(web): split AdvancedSearchForm into builder and field groups

EOF
)"
```

---

### Task 5.3: Split `StorageSection.tsx` (~476 lines)

**Files:**
- Create: `web/src/screens/settings/storage/StorageUsageCard.tsx`
- Create: `web/src/screens/settings/storage/ImportHistoryTable.tsx`
- Create: `web/src/screens/settings/storage/ImportDetailPanel.tsx`
- Create: `web/src/screens/settings/storage/TopAttachmentsTable.tsx`
- Create: `web/src/screens/settings/storage/storageUtils.ts`
- Create: `web/src/screens/settings/storage/useStorageData.ts` (optional if `useResource` covers it)
- Modify: `web/src/screens/settings/StorageSection.tsx`

- [ ] **Step 1: Extract utils + tables** to flatten the deep JSX (colSpan detail row).

- [ ] **Step 2: Verify and commit**

```bash
git commit -am "$(cat <<'EOF'
refactor(web): split StorageSection into usage, history, and attachments

EOF
)"
```

---

### Task 5.4: Split `MessageView.tsx` (~453 lines)

**Files:**
- Create: `web/src/screens/message/useConversationMessages.ts`
- Create: `web/src/screens/message/ConversationHeader.tsx`
- Create: `web/src/screens/message/YearChipBar.tsx`
- Create: `web/src/screens/message/MessageFindBar.tsx`
- Create: `web/src/screens/message/MessageThread.tsx`
- Modify: `web/src/screens/MessageView.tsx`

- [ ] **Step 1: Extract data hook first** (page/year fetch), then chrome components.

- [ ] **Step 2: Verify and commit**

```bash
git commit -am "$(cat <<'EOF'
refactor(web): split MessageView into data hook and chrome pieces

EOF
)"
```

---

### Task 5.5: Split `ContactDrawerHandles.tsx` (~472 lines) and `ApiTokensSection.tsx` (~390 lines)

**Files:**
- Create: handle table/row/mutations under `web/src/components/contactDrawer/`
- Create: tokens hook/table/form under `web/src/screens/settings/`

- [ ] **Step 1: Split handles table** without changing mutation API to the parent drawer.

- [ ] **Step 2: Split API tokens** list/create/dialogs.

- [ ] **Step 3: Verify and commit** (one or two commits)

```bash
git commit -am "$(cat <<'EOF'
refactor(web): split contact handle table and API tokens section

EOF
)"
```

---

### Task 5.6: Merge Pass 5

Title: `refactor(web): cleanup pass 5 split large screens`.

---

# Pass 6 — Domain consolidation

Branch: `refactor/web-cleanup-pass-6-domain`.

### Task 6.1: `useAccountProfile` + single profile type

**Files:**
- Create: `web/src/lib/useAccountProfile.ts`
- Modify: `web/src/screens/settings/ProfileSettingsPanel.tsx`, `AccountSettingsPanel.tsx`
- Modify: move `AccountProfile` type out of `profileStyles.ts` into `web/src/lib/account.ts` (or `types.ts`)

- [ ] **Step 1: One fetch hook, one type.** Panels only render.

- [ ] **Step 2: Verify and commit**

```bash
git commit -am "$(cat <<'EOF'
refactor(web): share useAccountProfile across settings panels

EOF
)"
```

---

### Task 6.2: Shared `HandleService` + options

**Files:**
- Create: `web/src/lib/handleService.ts`
- Modify: `OnboardingScreen.tsx`, `ProfileSettingsPanel.tsx`, `contactDrawer/AddIdentityDialog.tsx`, `contactDrawerTypes.ts`

- [ ] **Step 1: Export union + `HANDLE_SERVICE_OPTIONS`.** Replace local string arrays and casts; use `parseSelectKey`.

- [ ] **Step 2: Verify and commit**

```bash
git commit -am "$(cat <<'EOF'
refactor(web): share HandleService options across onboarding and contacts

EOF
)"
```

---

### Task 6.3: `FormField` (inline | stacked) and list chrome

**Files:**
- Create: `web/src/components/FormField.tsx`
- Modify: `FormRow.tsx` (thin wrap or replace), `screens/import/ImportFormUi.tsx` `StackedField`
- Modify: `ConversationList.tsx` toward `InfiniteOffsetList` **only if** behavior parity is achievable in one PR; otherwise extract shared “range label + near-end” chrome used by both lists

- [ ] **Step 1: Unify field layout component.**

- [ ] **Step 2: Reduce conversation/contact list duplication** without regressing scroll or selection.

- [ ] **Step 3: Verify and commit**

```bash
git commit -am "$(cat <<'EOF'
refactor(web): unify FormField layout and list chrome

EOF
)"
```

---

### Task 6.4: Dates, branded bubbles, SourcesPanel shell

**Files:**
- Create: `web/src/lib/formatDate.ts` (+ tests)
- Modify: date call sites in tokens/storage/conversation row/trash/message view
- Modify: `WhatsAppBubble.tsx`, `InstagramBubble.tsx`, `DiscordBubble.tsx` → shared wrapper
- Modify: `SourcesPanel.tsx` to use `ModalShell` (or a small drawer variant)

- [ ] **Step 1: `formatDate` helpers + migrate call sites.**

- [ ] **Step 2: Extract branded service bubble wrapper** (color/slots only).

- [ ] **Step 3: SourcesPanel uses shared modal/drawer chrome.**

- [ ] **Step 4: Verify and commit**

```bash
git commit -am "$(cat <<'EOF'
refactor(web): share date helpers, service bubbles, and sources modal

EOF
)"
```

---

### Task 6.5: Merge Pass 6

Title: `refactor(web): cleanup pass 6 domain consolidation`.

---

## Out of scope (do not sneak in)

- `web-next/` anything
- React Compiler ESLint rule enablement
- Full zod/valibot adoption for every `apiClient` call (Pass 2 guards are enough; schema library is a later project)
- Coverage gates / Playwright e2e
- Redesigning Import UX or search UX
- Deleting empty `/` and `/contacts` routes

## Definition of done (whole sequence)

- Passes 1–6 merged (or explicitly deferred by user).
- `cd web && npm run lint && npm test && npm run build` green on `main`.
- No new orphan screens; knip unused-export noise reduced for the modules touched.
- `/messages/:id` refresh works (Pass 2).
- Extract/Format/Export share one form shell; Import shares job lifecycle.
- Largest screens are split into focused modules under ~300 lines where practical.
