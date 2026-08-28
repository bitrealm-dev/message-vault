# Auth entry flow — design

Date: 2026-08-28
Status: approved, not implemented
Scope: `web/` only. No server changes.

## Problem

Getting into a vault takes three screens: choose a vault, sign in, set up a profile. The first
exists to answer a question that has a correct default nearly every time, and the third is the
busiest card in the app.

Specific complaints this design answers:

- Profile setup is too busy — three lines of prose above the first field, a section heading and a
  help line above the account rows, and a bare unstyled text link for "add another account".
- "Source Accounts" is a third name for a thing already called "My Handles" in Settings and
  "Identity" in the contact drawer.
- The back control on profile setup goes back two screens, not one, and does not look like a button.
- The card changes size on every screen and grows as accounts are added.
- The Server URL help line explains Vite's proxy and the vault's static hosting — our plumbing, not
  anything the reader can act on.
- The reachability dot sits at the far right of a label row, detached from the address it describes.
- Errors render the HTTP status and a raw JSON envelope at the user.

## Goals

1. Two screens: sign in, then profile setup on first run only.
2. One card size — 448 × 560 — on every screen and in every state. No resizing, no scrolling.
3. Name the vault being connected to, and let it be changed without leaving the card.
4. One vocabulary for connection state: connecting… / connected / disconnected.
5. Errors that read as English, in the place that matches what failed.

## Non-goals

- No server changes. Every status code and message stays as it is.
- No change to what `/v1/auth/mode` returns, to Hanko sign-in, or to the demo/guest pool.
- No new persistence. The saved server URL keeps its current shape and lifetime.

## Screens

```
launch → Sign in (LoginScreen) → Profile setup (OnboardingScreen, first run only) → vault
```

`LoginScreen` becomes the entry point. The vault-selection step is deleted, along with its
"Back to Vault Selection" control.

A consequence worth stating: with sign-in first, profile setup's back is plain `logout()`. An earlier
draft of this design added a transient `resumeSignIn` flag to `AuthProvider` so that logging out
would return to the sign-in tabs rather than the vault picker. That flag is no longer needed, and
`lib/auth.tsx` is untouched by this work.

## The sign-in card

### Resolving the vault

A helper returns the address to use, in priority order:

1. the saved server URL from a previous session, if any;
2. `DEFAULT_TAURI_VAULT_URL` (`http://127.0.0.1:8080`) on the desktop;
3. `""` in the browser, meaning this origin.

The vault line displays a host, never an empty string: for case 3 it shows `location.host`. The
existing legacy rewrite (`http://localhost:8080` → `http://127.0.0.1:8080`) stays as it is.

### States

The card resolves its vault on mount and moves between four states without navigating.

| State | Vault line | Form | Leaves when |
|---|---|---|---|
| connecting | `host · connecting…` | skeleton, action muted | `/v1/auth/mode` answers, or 8 s elapse |
| connected | `host · connected`, with Change | tabs or Hanko, action live | Change pressed, or sign-in submitted |
| editing | address field + Use, live status | dimmed, action muted | Use accepts an address, or Cancel |
| disconnected | `host · disconnected`, field + Retry | dimmed, action muted | a probe succeeds |

The skeleton in `connecting` holds the frame so the card does not flicker into shape.

`disconnected` keeps the sign-in form visible but dimmed, with the address field and Retry above it
and the help line "Start your vault, or enter another address." The fix is offered where the problem
is reported; nothing navigates.

### Vocabulary

Three words, used everywhere including `healthStatusLabel`:

- **connecting…** — a probe is in flight
- **connected** — the vault answered
- **disconnected** — it did not

The current labels "Server reachable" / "Server unreachable" / "Checking server" are replaced. No
other status synonyms appear anywhere in these screens.

### One probe, not two

A successful `/v1/auth/mode` is itself proof the vault is reachable, so it drives the status in the
collapsed states. `useVaultHealth` runs only while the address editor is open, where watching a typed
address go green before committing to it is the job it was written for.

### Timing

All existing behavior from `lib/vaultHealth.ts` except the last row.

| Behavior | Value | Source |
|---|---|---|
| One probe gives up | 8 s | `HEALTH_PROBE_TIMEOUT_MS` |
| Retry backoff | 1 s → 2 s → 4 s → 8 s → 16 s → 30 s | `healthBackoffMs`, capped at 30 s |
| Gives up entirely | never — settles into a 30 s poll | so a vault you start is noticed within 30 s |
| Re-check when connected | 30 s | `HEALTH_SUCCESS_RECHECK_MS` |
| Typing settles | 400 ms | `HEALTH_URL_DEBOUNCE_MS` |
| **Mode detection gives up** | **8 s (new)** | `/v1/auth/mode` has no timeout today |

The last row is required work, not a nicety: without a timeout, `connecting…` hangs on the browser's
own default when a host accepts the connection and never answers. It is implemented at the call site:
`LoginScreen` passes an `AbortSignal` built from `HEALTH_PROBE_TIMEOUT_MS` into the `/v1/auth/mode`
request, the way `logoutTimeoutSignal()` already does for logout. `api.ts` gains no timeout of its
own — a global one would change every request in the app.

## Errors

### The rule: split by who answered

- **The vault replied and refused** — 400, 401, 409, 429, 500 — the message belongs to the **form
  footer**.
- **Nothing replied at all** — a rejected `fetch` — the vault line goes **disconnected** and opens
  the address field. A transport failure is not a credentials problem and is not reported as one.

Today a dead vault puts `Failed to fetch` underneath the password field. That stops.

### The envelope

`api.ts` currently throws ``new Error(`${res.status}: ${text}`)`` with the raw response body, and the
server answers `{"ok":false,"error":"…"}`. A wrong password therefore renders as:

```
401: {"ok":false,"error":"invalid username or password"}
```

Fix in `api.ts`: parse the envelope, throw an error whose message is the server's `error` string, and
attach the status as a property for callers that need to branch on it. Fall back to the raw text when
the body is not the envelope, and to a generic sentence when the body is empty. The server's strings
are already serviceable English; the client capitalizes the first character for display and adds
nothing else.

This is one change in the API client and it corrects every form in the app, not only these two.

### Position

`AuthErrorFooter` moves above the primary action, inside the pinned footer, so a message grows upward
into the card's slack and nothing below it moves. It keeps its reserved height and
`aria-live="polite"`.

### Constraint: no username enumeration

No message may distinguish a wrong password from an account that does not exist. `login_handler`
runs `verify_password(dummy_password_hash(), …)` on the missing-account branch so both paths take
the same time and return the same 401 with the same string. This is deliberate. A friendlier
"we don't know that user" message would throw it away.

## Profile setup

1. **Top of card.** Three stacked lines become two, all left-aligned: the title, then
   "So we can match imported messages to you." The greeting and the explainer said the same thing
   twice, and the centered pair fought the left-aligned title above them.
2. **"Your Accounts"** replaces "Source Accounts" — the words someone says out loud, and the same
   word whether the row holds a phone number today or a Discord username later. The service picker
   in each row carries the type, so nothing needs renaming when a new service lands.
3. **The help line under it is deleted.** An empty field showing `+1 555-123-4567` already says what
   belongs there.
4. **Per-service examples.** Each service carries its own placeholder, and the field swaps when the
   picker changes. Today this is a ternary — email gets `you@example.com`, everything else gets a
   phone number — which is already wrong for WhatsApp's format and would be wrong for every
   username-shaped service. The examples live beside the service list in `handleService.ts` so
   adding a service means adding its example on the same line.

   | Service | Example |
   |---|---|
   | Text message | `+1 555-123-4567` |
   | Email | `you@example.com` |
   | WhatsApp | `+1 555-123-4567` |
   | Discord (when it lands) | `yourname` |

5. **Add control.** A secondary button labeled "+ Add account", right-aligned under the rows, in
   place of the bare accent-colored text link flush against the left margin.
6. **Remove control.** The `×` becomes a `Button variant="ghostDanger" size="icon"`, and it renders
   only when there is more than one row rather than sitting there permanently disabled.
7. **Row cap: three.** At three rows the Add button is replaced by "Add the rest in Settings after
   setup." An unbounded list and a fixed frame that never scrolls cannot both be true.
8. **Field chrome.** Display Name uses `authInput` (square, `bg-elevated`) while the account rows use
   `TextField` (rounded, `bg-bg`). Everything moves to `TextField`.
9. **Back.** `AuthBackButton` labeled "Back to Sign In", in the same bottom-left position the
   sign-in card used, calling `logout()`.

## The frame

`authCard` becomes a fixed 448 × 560 flex column: content top, actions pinned bottom.

Pinning the actions is what puts Sign in, Create account, and Continue to Vault on the same pixel
row on every screen — and it means switching between the Login and Create Account tabs no longer
moves the button either.

Costs, accepted: the tallest form (Create Account, ~457 px of content) leaves ~100 px of slack, the
shortest leaves more, and profile setup takes three accounts rather than an unbounded list. Settings
→ Profile remains the place to add a fourth and beyond.

## Files

| File | Change |
|---|---|
| `web/src/screens/LoginScreen.tsx` | vault line, four states, mount detection; vault-selection step deleted |
| `web/src/screens/OnboardingScreen.tsx` | all nine profile-setup changes |
| `web/src/lib/api.ts` | parse the error envelope; attach status |
| `web/src/lib/authGuards.ts` | vault resolution and display-host helpers |
| `web/src/lib/vaultHealth.ts` | status labels; 8 s budget for mode detection |
| `web/src/lib/handleService.ts` | per-service placeholder examples |
| `web/src/lib/uiStyles.ts` | fixed-frame `authCard` |
| `web/src/components/AuthErrorFooter.tsx` | position within the footer |

`lib/auth.tsx` is not touched.

## Testing

`web/src/screens/LoginScreen.test.tsx` — extend:

- auto-detects on mount and renders the tabs with no Connect press;
- a failed detection renders the address editor with the form dimmed;
- Change opens the editor, and Use re-detects against the typed address;
- Hanko mode still renders its element;
- the "Connect advances the card" case is removed with the button.

`web/src/screens/OnboardingScreen.test.tsx` — new:

- heading reads "Your Accounts" and no help line follows it;
- the placeholder changes with the service picker;
- Add appends a row; Remove drops one and is absent at a single row;
- Add is replaced by the Settings note at three rows;
- back calls `logout()`.

`web/src/lib/api.test.ts` — extend: an envelope body yields the server's sentence as the message with
the status attached; a non-envelope body falls back to the raw text; an empty body yields the generic
sentence.

Then `npm run lint && npm test`, and a Playwright pass over both cards in light and dark, since the
frame and both cards change visually.

## Risks and behavior changes

1. **The app contacts a vault at launch without being asked.** Dropping the Connect press means
   `/v1/auth/mode` fires on mount. In the browser that is the origin that served the page; on the
   desktop it is loopback. Neither reaches past what opening the app already implies, but it is a
   real change and reviewers should see it stated.
2. **A slow vault must time out.** Covered by the 8 s budget above. Without it this design makes the
   first paint worse than the one it replaces.
3. **The row cap is a product decision, not a technical limit.** Three covers a phone, an email, and
   one more; anyone with a longer list finishes in Settings.

## Out of scope

Findings from this work that belong in their own changes:

- **Dark-theme primary button contrast.** In Ocean Depths dark, `bg-accent` with `text-sent-text` is
  white on `#a8dadc`, roughly 1.6:1. Every primary button in the app is affected, not just these.
- **Rate limiting is keyed by username, not IP.** `login:{username}` / `register:{username}`, 20 hits
  per 60 s sliding window, in-process, counting successes. Hammering one username is capped; spraying
  one attempt each across many usernames is not limited at all. On a self-hosted box exposed to the
  internet that is the half that matters. No human reaches the cap interactively, so the 429 message
  is script-only in practice.
- **`HANKO_API_URL is not configured` is an `Internal`,** so the operator's actual problem reaches
  the user as `internal server error` and the useful text goes only to stderr.
- **`password.len()` is bytes, not characters.** "At least 8 characters" admits four emoji.
- **Settings and contacts vocabulary.** Settings → Profile says "My Handles" and the contact drawer
  says "Identity" for what setup now calls accounts. Worth one pass over the words a user sees.

## Appendix A — every error these screens can produce

### `POST /v1/auth/login`

| Status | Text | When |
|---|---|---|
| 400 | `username is required` | blank username; the client catches this first |
| 400 | `password is too long` | over `MAX_PASSWORD_BYTES` (1024) |
| 401 | `invalid username or password` | wrong password **or** no such account |
| 401 | `use Try it to open a sample account` | username `demo` with the hosted guest pool enabled; unreachable self-hosted |
| 429 | `too many authentication attempts; try again shortly` | 20 hits in 60 s for `login:{username}` |
| 500 | `internal server error` | anything else; the real cause stays in the server log |

### `POST /v1/auth/register`

| Status | Text |
|---|---|
| 400 | `username must be 1–128 chars (alphanumeric, _, -, .)` |
| 400 | `password must be at least 8 characters` |
| 400 | `password is too long` |
| 409 | `username already taken: {username}` |
| 429 | `too many authentication attempts; try again shortly` |
| 500 | `internal server error` |

### `POST /v1/auth/hanko/session`

| Status | Text |
|---|---|
| 400 | `hanko_jwt is too long` |
| 401 | `invalid or expired session` |
| 500 | `internal server error` (hides `HANKO_API_URL is not configured`) |

### `POST /v1/account/profile`

| Status | Text |
|---|---|
| 400 | `unsupported handle service: {service}` |
| 401 | `invalid API token` — expired or cleared session |
| 500 | `internal server error` |

### Client-side, before any request

`Username is required.` · `Passwords do not match.` · `Not signed in` ·
`Could not reach server. …` (two variants, desktop and browser)

### Transport failures

`Failed to fetch` (Chrome) · `NetworkError when attempting to fetch resource.` (Firefox) ·
`Load failed` (Safari). These are routed to the vault line as **disconnected**, never to the form
footer.

## Appendix B — mockup

Every state above is rendered at true size, in both Ocean Depths themes, in the design mockup
published for this work — "Auth Card Frame",
<https://claude.ai/code/artifact/64df5d0c-cac6-4a6f-a591-f5c2096b0f24>. The mockup and this document
were kept in step; where they disagree, this document wins.
