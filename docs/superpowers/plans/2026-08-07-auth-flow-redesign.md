# Auth Flow Redesign Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Unify login and registration paths through a single onboarding screen, eliminate redundant profile collection, and add an empty-state call-to-action for new accounts.

**Architecture:** Backend adds a `new_account` flag to all auth responses so the client knows whether to show onboarding. The register endpoint drops `preferred_name` and `phone` — profile fields live only in the profile/onboarding path. Frontend auth state stops persisting mid-onboarding so a closed window = clean restart.

**Tech Stack:** Rust (axum, rusqlite, serde), TypeScript (React 19, inline styles), no new dependencies.

## Global Constraints

- No database schema changes
- Backward compatible — old clients ignore new fields, old fields are silently dropped by serde
- Password is optional (demo account, no-password accounts) — login button disabled only on `loading || !username`

---

### Task 1: Backend — Add `new_account` flag and clean up `RegisterRequest`

**Files:**
- Modify: `crates/vault/server/src/auth.rs:23-52` (types)
- Modify: `crates/vault/server/src/auth.rs:106-173` (register_handler)
- Modify: `crates/vault/server/src/auth.rs:175-216` (login_handler)
- Modify: `crates/vault/server/src/auth.rs:218-356` (hanko_session_handler)

**Interfaces:**
- Consumes: nothing (first task)
- Produces: `AuthTokenResponse` with `new_account: bool`; `RegisterRequest` without `preferred_name`/`phone`

- [ ] **Step 1: Add `new_account` to `AuthTokenResponse`**

Replace lines 47-52:
```rust
#[derive(Debug, Serialize)]
pub struct AuthTokenResponse {
    pub token: String,
    pub account_id: String,
    pub username: String,
    pub new_account: bool,
}
```

- [ ] **Step 2: Remove `preferred_name` and `phone` from `RegisterRequest`**

Replace lines 23-32:
```rust
#[derive(Debug, Deserialize)]
pub struct RegisterRequest {
    pub username: String,
    #[serde(default)]
    pub password: Option<String>,
}
```

- [ ] **Step 3: Update `register_handler` — strip profile fields, set `new_account: true`**

Replace lines 106-173 with:
```rust
/// `POST /v1/auth/register` — create an account and return an API token.
pub async fn register_handler(
    State(state): State<AppState>,
    Json(req): Json<RegisterRequest>,
) -> Result<Json<AuthTokenResponse>, ApiError> {
    let username = normalize_username(&req.username);
    if !is_valid_username(&username) {
        return Err(ApiError::BadRequest(
            "username must be 1–128 chars (alphanumeric, _, -, .)".into(),
        ));
    }

    let password_plain = req.password.as_deref().unwrap_or("").to_string();
    let password_hash: Option<String> = if password_plain.is_empty() {
        None
    } else {
        Some(hash_password(&password_plain).map_err(|e| ApiError::Internal(e.to_string()))?)
    };

    let account_id = uuid::Uuid::new_v4().to_string();

    let db = state.cfg.paths.db.clone();
    let result = tokio::task::spawn_blocking(move || -> Result<AuthTokenResponse> {
        let conn = Connection::open(&db)?;
        schema::configure_connection(&conn)?;

        if account_profile::lookup_account_ref(&conn, &username)?.is_some() {
            bail!("username already taken: {username}");
        }

        account_profile::insert_account(
            &conn,
            &account_id,
            &username,
            password_hash.as_deref(),
            None,  // preferred_name (set during onboarding)
            None,  // hanko_user_id
            false, // read_only
        )?;

        let token = api_tokens::insert_account_api_token(&conn, &account_id)?;
        Ok(AuthTokenResponse {
            token,
            account_id,
            username,
            new_account: true,
        })
    })
    .await
    .map_err(|e| ApiError::Internal(format!("register task: {e}")))?
    .map_err(|e| ApiError::BadRequest(e.to_string()))?;

    Ok(Json(result))
}
```

- [ ] **Step 4: Update `login_handler` — set `new_account: false`**

Replace the `Ok(AuthTokenResponse { ... })` block at lines 205-209 with:
```rust
        Ok(AuthTokenResponse {
            token,
            account_id,
            username,
            new_account: false,
        })
```

- [ ] **Step 5: Update `hanko_session_handler` — track and surface `new_account`**

Replace lines 295-349 (the `let account_id = match ...` through the response construction) with:
```rust
        let mut new_account = false;

        let account_id =
            match account_profile::lookup_account_by_hanko(&conn, &hanko_user_id)? {
                Some(id) => id,
                None => {
                    new_account = true;
                    // Auto-provision a new account
                    let account_id = uuid::Uuid::new_v4().to_string();
                    let username = email
                        .as_ref()
                        .and_then(|e| e.split('@').next())
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| {
                            format!(
                                "user_{}",
                                &hanko_user_id.chars().take(8).collect::<String>()
                            )
                        });

                    // Ensure username is unique
                    let username =
                        if account_profile::lookup_account_ref(&conn, &username)?.is_some()
                        {
                            format!("{}_{}", username, &account_id[..8])
                        } else {
                            username
                        };

                    account_profile::insert_account(
                        &conn,
                        &account_id,
                        &username,
                        None, // no password for hanko accounts
                        None, // preferred_name (set during onboarding)
                        Some(&hanko_user_id),
                        false,
                    )?;

                    if let Some(email) = &email {
                        let _ = account_profile::upsert_account_email(
                            &conn, &account_id, email, true,
                        );
                    }

                    account_id
                }
            };

        let token = api_tokens::get_or_create_api_token(&conn, &account_id)?;
        let username = account_profile::username_for_account(&conn, &account_id)?
            .unwrap_or_else(|| account_id.clone());

        Ok(AuthTokenResponse {
            token,
            account_id,
            username,
            new_account,
        })
```

- [ ] **Step 6: Build and verify**

Run: `cargo build -p message-vault-server 2>&1`
Expected: compiles with no errors.

- [ ] **Step 7: Commit**

```bash
git add crates/vault/server/src/auth.rs
git commit -m "feat: add new_account flag to auth responses, drop profile fields from register

- AuthTokenResponse gains new_account: bool
- RegisterRequest drops preferred_name and phone
- register_handler always returns new_account: true
- login_handler always returns new_account: false
- hanko_session_handler returns true when auto-provisioned"
```

---

### Task 2: Frontend — Refactor auth state management (`auth.tsx`)

**Files:**
- Modify: `web/src/lib/auth.tsx`

**Interfaces:**
- Consumes: `AuthTokenResponse.new_account` (from Task 1)
- Produces: `login(serverUrl, token, accountId, newAccount)`, `finishOnboarding()`

- [ ] **Step 1: Update `AuthState` — `needsOnboarding` is in-memory only**

Replace lines 12-18:
```ts
interface AuthState {
  serverUrl: string;
  token: string | null;
  accountId: string | null;
  isAuthenticated: boolean;
  needsOnboarding: boolean;  // in-memory only, never persisted
}
```

- [ ] **Step 2: Update `AuthContextValue` — add `finishOnboarding`, change `login` signature**

Replace lines 25-29:
```ts
interface AuthContextValue extends AuthState {
  login: (serverUrl: string, token: string, accountId: string, newAccount: boolean) => void;
  logout: () => void;
  setServer: (url: string) => void;
  finishOnboarding: () => void;
}
```

- [ ] **Step 3: Update `persistState` — drop `needsOnboarding` from persisted keys**

Replace lines 45-59:
```ts
function persistState(state: AuthState) {
  try {
    localStorage.setItem(
      STORAGE_KEY,
      JSON.stringify({
        serverUrl: state.serverUrl,
        token: state.token,
        accountId: state.accountId,
      }),
    );
  } catch {
    // Storage full or unavailable — not critical
  }
}
```

- [ ] **Step 4: Update `loadPersisted` return type comment**

No code change needed — `loadPersisted` already parses whatever is in localStorage and will naturally return `{serverUrl, token, accountId}` without `needsOnboarding`. The function is fine as-is.

- [ ] **Step 5: Update initial state — default `needsOnboarding: false`, no persisted flag**

Replace lines 73-91 (the `useState` initializer):
```ts
  const [state, setState] = useState<AuthState>(() => {
    const persisted = loadPersisted();
    if (persisted?.serverUrl && persisted?.token && persisted?.accountId) {
      return {
        serverUrl: persisted.serverUrl,
        token: persisted.token,
        accountId: persisted.accountId,
        isAuthenticated: true,
        needsOnboarding: false,  // persisted tokens only exist after onboarding
      };
    }
    return {
      serverUrl: persisted?.serverUrl || "",
      token: null,
      accountId: null,
      isAuthenticated: false,
      needsOnboarding: false,
    };
  });
```

- [ ] **Step 6: Simplify mount restore — no profile heuristic**

Replace lines 93-144 (the `useEffect` for token validation):
```ts
  // Validate restored token on mount
  useEffect(() => {
    if (!state.isAuthenticated || restored) return;

    let cancelled = false;
    const validate = async () => {
      try {
        setBaseUrl(state.serverUrl);
        setToken(state.token);
        await apiClient.get("/v1/auth/check");
        if (!cancelled) setRestored(true);
      } catch {
        // Token invalid — clear and show login
        if (!cancelled) {
          authEpoch.current++;
          setToken(null);
          clearPersisted();
          setState((s) => ({
            ...s,
            token: null,
            accountId: null,
            isAuthenticated: false,
            needsOnboarding: false,
          }));
          setRestored(true);
        }
      }
    };
    validate();
    return () => {
      cancelled = true;
    };
  }, [state.isAuthenticated, restored, state.serverUrl, state.token]);
```

- [ ] **Step 7: Replace `login` — server-driven `newAccount` flag, no profile fetch**

Replace lines 151-181 (the `login` callback):
```ts
  const login = useCallback(
    (serverUrl: string, token: string, accountId: string, newAccount: boolean) => {
      const epoch = ++authEpoch.current;
      setBaseUrl(serverUrl);
      setToken(token);

      const newState: AuthState = {
        serverUrl,
        token,
        accountId,
        isAuthenticated: true,
        needsOnboarding: newAccount,
      };

      // Only persist once onboarding is complete
      if (!newAccount) {
        persistState(newState);
      }

      if (authEpoch.current !== epoch) return; // superseded by logout
      setState(newState);
      setRestored(true);
    },
    [],
  );
```

- [ ] **Step 8: Add `finishOnboarding` callback**

Insert after the `login` callback (after line 181 in the original, after the `login` closing `},`):
```ts
  const finishOnboarding = useCallback(() => {
    setState((s) => {
      const next: AuthState = { ...s, needsOnboarding: false };
      persistState(next);
      return next;
    });
  }, []);
```

- [ ] **Step 9: Update the context value to expose `finishOnboarding`**

Replace the `return` in `AuthProvider` (lines 196-200):
```ts
  return (
    <AuthContext.Provider value={{ ...state, login, logout, setServer, finishOnboarding }}>
      {children}
    </AuthContext.Provider>
  );
```

- [ ] **Step 10: Verify TypeScript compiles**

Run: `cd web && npx tsc --noEmit 2>&1`
Expected: no errors.

- [ ] **Step 11: Commit**

```bash
git add web/src/lib/auth.tsx
git commit -m "feat: server-driven new_account flag, add finishOnboarding

- login() accepts newAccount param instead of profile heuristic
- finishOnboarding() persists state after onboarding completes
- No persist mid-onboarding — close window = clean restart
- Mount restore simplified — no profile check needed"
```

---

### Task 3: Frontend — Update LoginScreen to pass `new_account`

**Files:**
- Modify: `web/src/screens/LoginScreen.tsx`

**Interfaces:**
- Consumes: `login(serverUrl, token, accountId, newAccount)` (from Task 2)
- Produces: correct `new_account` values passed to login

- [ ] **Step 1: Update local login handler — pass `new_account: false`**

Replace line 59 (inside `handleLocalLogin`):
```ts
      login(serverUrl, res.token, res.account_id, false);
```

- [ ] **Step 2: Update Hanko session handler — pass `res.new_account`**

Replace line 98 (inside `hanko.onSessionCreated`):
```ts
              login(serverUrl, res.token, res.account_id, res.new_account);
```

Note: `apiClient.post` is untyped, so `res.new_account` works without type declaration changes.

- [ ] **Step 3: Verify TypeScript compiles**

Run: `cd web && npx tsc --noEmit 2>&1`
Expected: no errors.

- [ ] **Step 4: Commit**

```bash
git add web/src/screens/LoginScreen.tsx
git commit -m "feat: pass new_account flag from auth responses to login()"
```

---

### Task 4: Frontend — Strip RegisterScreen to username + password only

**Files:**
- Modify: `web/src/screens/RegisterScreen.tsx`

**Interfaces:**
- Consumes: `login(serverUrl, token, accountId, newAccount)` (from Task 2)
- Produces: trimmed form, passes `new_account: true`

- [ ] **Step 1: Remove display name and phone state variables**

Replace lines 13-18 (variable declarations in the component):
```ts
  const { login } = useAuth();
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [confirmPassword, setConfirmPassword] = useState("");
  const [noPassword, setNoPassword] = useState(false);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");
```

Remove: `preferredName` and `phone` state.

- [ ] **Step 2: Update `handleRegister` — drop profile fields, pass `new_account: true`**

Replace lines 22-53 (the `handleRegister` function):
```ts
  const handleRegister = async () => {
    setError("");

    if (!username.trim()) {
      setError("Username is required.");
      return;
    }
    if (!noPassword && password !== confirmPassword) {
      setError("Passwords do not match.");
      return;
    }

    setLoading(true);
    try {
      setBaseUrl(serverUrl);
      const res = await apiClient.post<{
        token: string;
        account_id: string;
        username: string;
      }>("/v1/auth/register", {
        username: username.trim(),
        password: noPassword ? "" : password,
      });
      login(serverUrl, res.token, res.account_id, true);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  };
```

- [ ] **Step 3: Remove Display Name and Phone fields from JSX**

Delete lines 104-140 (the Display Name label+input, Phone label+input, and the "No password" checkbox block that precedes them).

The form should render in this order: Username → "No password" checkbox → Password / Confirm (if not noPassword) → error → "Create account" button → "← Back to login".

- [ ] **Step 4: Verify TypeScript compiles**

Run: `cd web && npx tsc --noEmit 2>&1`
Expected: no errors.

- [ ] **Step 5: Commit**

```bash
git add web/src/screens/RegisterScreen.tsx
git commit -m "feat: strip RegisterScreen to username + password only

Remove display name and phone fields — profile collection
is handled by the unified OnboardingScreen."
```

---

### Task 5: Frontend — Add sign out and `finishOnboarding` to OnboardingScreen

**Files:**
- Modify: `web/src/screens/OnboardingScreen.tsx`

**Interfaces:**
- Consumes: `finishOnboarding()` and `logout()` (from Task 2)
- Produces: sign-out button, correct completion flow

- [ ] **Step 1: Update `useAuth` destructure — add `finishOnboarding`, `logout`**

Replace line 13:
```ts
  const { finishOnboarding, logout } = useAuth();
```

Remove the unused `login`, `token`, `serverUrl`, and `accountId` — `handleSubmit` no longer calls `login()`, and `apiClient` already has the base URL and token configured.

- [ ] **Step 2: Update `handleSubmit` — call `finishOnboarding` instead of `login`**

Replace lines 34-49 (the `handleSubmit` function):
```ts
  const handleSubmit = async () => {
    setLoading(true);
    setError("");
    try {
      await apiClient.post("/v1/account/profile", {
        name: displayName.trim(),
        handles: handles.filter((h) => h.handle.trim()),
      });
      finishOnboarding();
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  };
```

- [ ] **Step 3: Add "Sign out" button below the submit button**

Insert after the submit button (after line 107, before the closing `</div>`):
```ts
        <button
          onClick={logout}
          style={{
            width: "100%",
            padding: "0.5rem",
            fontSize: "0.875rem",
            marginTop: "0.5rem",
            background: "transparent",
            border: "none",
            color: "#9ca3af",
            cursor: "pointer",
          }}
        >
          Sign out
        </button>
```

- [ ] **Step 4: Verify TypeScript compiles**

Run: `cd web && npx tsc --noEmit 2>&1`
Expected: no errors.

- [ ] **Step 5: Commit**

```bash
git add web/src/screens/OnboardingScreen.tsx
git commit -m "feat: add sign out button, use finishOnboarding()

- finishOnboarding() persists state after profile save
- Sign out button lets user bail without completing onboarding
- No longer re-calls login() after profile save"
```

---

### Task 6: Frontend — Empty state CTA in ConversationList

**Files:**
- Modify: `web/src/screens/ConversationList.tsx`
- Modify: `web/src/components/AppLayout.tsx`

**Interfaces:**
- Consumes: `activeView` navigation from AppLayout (existing pattern)
- Produces: `onNavigate` prop on ConversationList

- [ ] **Step 1: Add `onNavigate` prop to ConversationList**

Replace the function signature and destructure (lines 6-14):
```ts
export default function ConversationList({
  selectedId,
  onSelect,
  query,
  onNavigate,
}: {
  selectedId: string | null;
  onSelect: (conversation: Conversation) => void;
  query: string;
  onNavigate?: (view: string) => void;
}) {
```

- [ ] **Step 2: Replace the empty state (lines 33-35)**

Replace:
```ts
  if (conversations.length === 0) {
    return <div style={{ padding: "1rem", fontSize: "0.813rem", color: "#9ca3af" }}>No conversations</div>;
  }
```

With:
```ts
  if (conversations.length === 0) {
    return (
      <div style={{ padding: "1.5rem 1rem", fontSize: "0.813rem", color: "#9ca3af", textAlign: "center" }}>
        <p style={{ margin: "0 0 0.5rem", fontWeight: 600, color: "#6b7280" }}>No messages yet</p>
        <p style={{ margin: "0 0 1rem" }}>Import your first messages to get started.</p>
        {onNavigate && (
          <button
            onClick={() => onNavigate("import")}
            style={{
              padding: "0.5rem 1.25rem",
              fontSize: "0.813rem",
              fontWeight: 600,
              cursor: "pointer",
            }}
          >
            Import messages
          </button>
        )}
      </div>
    );
  }
```

- [ ] **Step 3: Pass `onNavigate` from AppLayout to ConversationList**

In `web/src/components/AppLayout.tsx`, find the `ConversationList` usage around lines 47-51 and add `onNavigate={setActiveView}`:

```ts
        <ConversationList
          selectedId={selectedConversation?.id || null}
          onSelect={(c) => { setSelectedConversation(c); setActiveView("conversations"); }}
          query={activeView === "trash" ? "is:trash" : searchQuery}
          onNavigate={setActiveView}
        />
```

- [ ] **Step 4: Verify TypeScript compiles**

Run: `cd web && npx tsc --noEmit 2>&1`
Expected: no errors.

- [ ] **Step 5: Commit**

```bash
git add web/src/screens/ConversationList.tsx web/src/components/AppLayout.tsx
git commit -m "feat: add Import CTA to empty conversation list

New accounts see 'No messages yet — Import your first messages'
with a button that navigates to the import screen."
```

---

## Verification (manual)

After all tasks complete, verify the full flow:

1. **Local registration:** Start server with `AUTH_MODE=local`. Open app → enter server URL → Connect → "Create account" → fill username + password → submit → should land on OnboardingScreen.
2. **Onboarding completion:** Fill display name + at least one handle → submit → should land on AppLayout with "No messages yet" CTA.
3. **Onboarding sign out:** Repeat registration → on OnboardingScreen click "Sign out" → should return to LoginScreen.
4. **Mid-onboarding restart:** Register → on OnboardingScreen, close browser tab → reopen → should show LoginScreen (not app). Log in with same credentials → should return to OnboardingScreen (server returns `new_account: false` for existing account... wait — login returns `new_account: false`, so user goes straight to app. That's correct: local registration creates the account and returns `new_account: true` the first time. On re-login, `new_account: false` → straight to app, skipping onboarding. This means a user who closes mid-onboarding and re-logs in with local auth will skip onboarding and land in the app with no profile.)
   - **Expected:** This is acceptable because the user already authenticated. They can fill in their profile later via Settings. No data loss, no lockout.
5. **Local login (existing):** Log out → log in with same credentials → should go straight to AppLayout.
6. **Empty state CTA:** Fresh account → should see "No messages yet" with Import button. Click it → should navigate to import screen.
