# Auth Flow Redesign

**Date:** 2026-08-07
**Status:** draft
**Scope:** Login, registration, and onboarding screens + supporting API changes.

## Problem

The current auth flow has three problems:

1. **Redundant profile collection.** `RegisterScreen` collects `preferred_name` and `phone` at account creation, then `OnboardingScreen` asks for display name and handles *again*. Users enter the same data twice.

2. **Forked UX.** Local-auth users see a registration form; Hanko users are auto-provisioned and skip it entirely. The two paths share no common structure.

3. **Dead-end empty state.** After completing onboarding, a new account with no imported messages sees "No conversations" and "Select a conversation to view messages" — no guidance on what to do next.

## Design

### API changes

**`AuthTokenResponse`** gains `new_account`:

```json
{
  "token": "mv-user-...",
  "account_id": "uuid",
  "username": "alice",
  "new_account": true
}
```

Returned by `POST /v1/auth/register`, `/v1/auth/login`, and `/v1/auth/hanko/session`.

| Endpoint | `new_account` |
|---|---|
| `POST /v1/auth/register` | always `true` |
| `POST /v1/auth/login` | always `false` |
| `POST /v1/auth/hanko/session` | `true` when the account was auto-provisioned; `false` when an existing account was found by `hanko_user_id` |

The `hanko_session_handler` already branches on new vs existing internally — it only needs to surface the flag.

**`RegisterRequest`** drops `preferred_name` and `phone`:

```json
{
  "username": "alice",
  "password": "optional-string"
}
```

Profile fields belong in the profile endpoint, not the auth endpoint. The server-side `register_handler` removes the `preferred_name` insert and `upsert_account_phone` call — those are handled by onboarding.

### Frontend: screen flow

```
LoginScreen
  ├─ Hanko mode: <hanko-auth> → new_account
  │    ├─ true  → OnboardingScreen
  │    └─ false → AppLayout
  └─ Local mode: username + password login → new_account: false → AppLayout
       └─ "Create account" → RegisterScreen (username + password only)
            └─ new_account: true → OnboardingScreen
```

### Frontend: per-screen changes

**LoginScreen** — no structural changes. Already handles mode detection and renders `<hanko-auth>` or local login. "Create account" button continues to link to RegisterScreen in local mode.

**RegisterScreen** — reduced to username, password, confirm password, and the "No password" checkbox. Display name and phone fields removed. "← Back to login" button kept.

**OnboardingScreen** — fields unchanged (display name, handles). Two additions:

- **"Sign out"** button at the bottom. Calls `logout()`, clears all state, returns to LoginScreen. Lets the user bail without completing onboarding.
- After successful submit, calls `finishOnboarding()` instead of re-calling `login()`.

### Frontend: auth state (`auth.tsx`)

**`AuthState`** — drops the persisted `needsOnboarding` flag:

```ts
interface AuthState {
  serverUrl: string;
  token: string | null;
  accountId: string | null;
  isAuthenticated: boolean;
  needsOnboarding: boolean;  // in-memory only, never persisted
}
```

`needsOnboarding` becomes an in-memory flag set from the server's `new_account` response. It is never written to localStorage.

**`login(serverUrl, token, accountId, newAccount)`** — sets `needsOnboarding: newAccount`. Does NOT persist when `newAccount` is true. Persists immediately when false.

**`finishOnboarding()`** — new method. Persists `{serverUrl, token, accountId}` to localStorage and sets `needsOnboarding: false`. Called by `OnboardingScreen` after profile save succeeds.

**`logout()`** — unchanged. Clears persisted state and resets everything. Works from any screen including onboarding.

**Mount restore** — simplified. On app load, if a persisted token exists it is validated against `GET /v1/auth/check`. On success → fully authenticated (persisted tokens only exist for completed accounts). On failure → clear and show login. No profile heuristic, no `needsOnboarding` flag to restore.

### Empty state after onboarding

`ConversationList` currently shows "No conversations" when the list is empty. For a new account with no imports this is a dead end.

Change the empty state to:

> **No messages yet**
> Import your first messages to get started.
> [Import messages] — button that navigates to `activeView="import"`

The parent component (`AppLayout`) already manages navigation via `onNavigate`/`activeView`. The `ConversationList` component receives an `onNavigate` prop for this.

### Edge cases

| Scenario | Behavior |
|---|---|
| Hanko session exchange fails | Error shown in LoginScreen, nothing persisted |
| Register fails (username taken, etc.) | Error shown in RegisterScreen, user stays on form |
| Onboarding save fails | Error shown, user stays on onboarding, nothing persisted |
| Mid-onboarding close/refresh | No persisted token → user sees LoginScreen on restart. Re-authenticate → server returns `new_account: true` → back to onboarding |
| Onboarding "Sign out" | `logout()` → cleared, back to LoginScreen |
| Login with empty password | Button disabled only on `loading \|\| !username` — empty password is valid (demo account, no-password accounts) |

## Files changed

| File | Change |
|---|---|
| `crates/vault/server/src/auth.rs` | Add `new_account` to `AuthTokenResponse`; remove `preferred_name`/`phone` from `RegisterRequest` and `register_handler`; surface `new_account` in `hanko_session_handler` and `login_handler` |
| `web/src/lib/auth.tsx` | Add `newAccount` param to `login()`; add `finishOnboarding()`; drop profile heuristic; don't persist mid-onboarding |
| `web/src/screens/LoginScreen.tsx` | Pass `new_account` from auth responses to `login()` |
| `web/src/screens/RegisterScreen.tsx` | Remove display name and phone fields; pass `new_account` to `login()` |
| `web/src/screens/OnboardingScreen.tsx` | Add "Sign out" button; call `finishOnboarding()` instead of `login()` |
| `web/src/screens/ConversationList.tsx` | Add empty-state CTA with "Import messages" navigation |
| `web/src/components/AppLayout.tsx` | Pass `onNavigate` to `ConversationList` |

## Backward compatibility

- `RegisterRequest` drops `preferred_name` and `phone` — the server ignores unknown fields by default (serde `#[serde(default)]` already used), so old clients sending these fields will still work; the values will be silently dropped.
- `AuthTokenResponse` gains `new_account` — old clients ignore unknown fields. They'll continue using the profile heuristic, which still works. No breakage.
- No database schema changes.
