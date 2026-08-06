# Tier 4 — Polish & Loose Ends

**Goal:** Wire remaining components that exist but aren't connected, clean up any issues, and verify the full build.

**Tasks:**

### Task 1: Wire ContactReviewTable into ImportScreen
- **File:** `web/src/screens/ImportScreen.tsx`
- ContactReviewTable exists but isn't shown. Add a step: when `contactsPath` is set, show a "Review contacts" button that calls `invokeContactsInfo` and renders the table.
- Build and commit.

### Task 2: Verify and fix any remaining issues
- Run `cd web && npm run build` and fix any errors
- Check git status for any uncommitted changes that should be committed
- Push everything
