# Try it two-layer rate limit Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `POST /v1/auth/try-demo` uses a 60-per-address cap and a 2000-per-process cap so a shared school IP can still get in while one address cannot flood the handler.

**Architecture:** Keep the in-memory sliding window. Add `try_demo_client_key` from `CF-Connecting-IP` (or `unknown`). `try_demo_handler` checks the per-address bucket first, then the global `try-demo` bucket, then assign/clone as today.

**Tech Stack:** Rust (`message-vault-server`, axum `HeaderMap`), existing `check_auth_rate_limit_max`.

**Spec:** [docs/superpowers/specs/2026-08-15-try-demo-rate-limit-design.md](../specs/2026-08-15-try-demo-rate-limit-design.md)

## Global Constraints

- Per-address cap: `60` accepted Try it calls per 60 seconds (`TRY_DEMO_PER_IP_RATE_MAX`).
- Process cap: `2000` accepted Try it calls per 60 seconds (`TRY_DEMO_RATE_MAX`).
- Window stays `AUTH_RATE_WINDOW` (60 seconds).
- Login / register / Hanko stay at `AUTH_RATE_MAX` (20) on their existing keys.
- Header: `CF-Connecting-IP` only. Do not read `X-Forwarded-For`. Do not use the TCP peer.
- Missing or invalid header → bucket `try-demo:unknown` with the 60 cap.
- Check per-address first, then global. A per-address 429 must not increment the global bucket.
- 429 body: `too many authentication attempts; try again shortly` (same as login).
- `message-vault-server` is a binary crate: `cargo test -p message-vault-server <filter>`, not `--lib`.
- `/tmp` may be over quota: `export TMPDIR="$HOME/.cache/tmp"` and a home-filesystem `CARGO_TARGET_DIR`.
- User-facing copy: no “we/us/our.”

## File map

| File | Role |
|---|---|
| `crates/vault/server/src/auth.rs` | Key helper, constants, `check_try_demo_rate_limits`, handler reads headers |
| `docs/src/content/docs/reference/config-and-accounts.md` | Caps and `CF-Connecting-IP` |

---

### Task 1: Parse `CF-Connecting-IP` into a bucket key

**Files:**
- Modify: `crates/vault/server/src/auth.rs`

**Interfaces:**
- Consumes: optional header string
- Produces:
  - `fn try_demo_client_key(cf_connecting_ip: Option<&str>) -> String`
  - Valid single IPv4/IPv6 → `try-demo:{ip}` (use the `IpAddr` `Display` form)
  - Anything else → `try-demo:unknown`

- [ ] **Step 1: Write the failing tests**

Add to `auth.rs` `mod tests` (next to the existing rate-limit tests):

```rust
#[test]
fn try_demo_client_key_accepts_single_ipv4_and_ipv6() {
    assert_eq!(
        try_demo_client_key(Some("203.0.113.10")),
        "try-demo:203.0.113.10"
    );
    assert_eq!(
        try_demo_client_key(Some(" 2001:db8::1 ")),
        "try-demo:2001:db8::1"
    );
}

#[test]
fn try_demo_client_key_rejects_missing_list_and_garbage() {
    assert_eq!(try_demo_client_key(None), "try-demo:unknown");
    assert_eq!(try_demo_client_key(Some("")), "try-demo:unknown");
    assert_eq!(
        try_demo_client_key(Some("203.0.113.10, 198.51.100.1")),
        "try-demo:unknown"
    );
    assert_eq!(try_demo_client_key(Some("not-an-ip")), "try-demo:unknown");
}
```

- [ ] **Step 2: Run tests and confirm they fail**

Run:

```bash
export TMPDIR="$HOME/.cache/tmp"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$HOME/.cache/cargo-target-hosted-guest-demo}"
mkdir -p "$TMPDIR" "$CARGO_TARGET_DIR"
cargo test -p message-vault-server try_demo_client_key
```

Expected: FAIL (`try_demo_client_key` not found)

- [ ] **Step 3: Implement the helper**

Near the rate-limit helpers in `auth.rs`:

```rust
use std::net::IpAddr;

fn try_demo_client_key(cf_connecting_ip: Option<&str>) -> String {
    match cf_connecting_ip.and_then(parse_single_ip) {
        Some(ip) => format!("try-demo:{ip}"),
        None => "try-demo:unknown".to_string(),
    }
}

fn parse_single_ip(raw: &str) -> Option<IpAddr> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed.contains(',') || trimmed.contains(' ') {
        return None;
    }
    trimmed.parse().ok()
}
```

Put `use std::net::IpAddr` with the other `std` imports at the top of the file (do not add a second `use std::...` block in the middle).

- [ ] **Step 4: Run tests and confirm they pass**

Run: `cargo test -p message-vault-server try_demo_client_key`

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/vault/server/src/auth.rs
git commit -m "$(cat <<'EOF'
feat(vault): key Try it rate limits by Cloudflare visitor address

A shared school IP stays one bucket; a missing header uses try-demo:unknown so local Compose does not treat every click as a new client.
EOF
)"
```

---

### Task 2: Two-layer check on `try-demo`

**Files:**
- Modify: `crates/vault/server/src/auth.rs`

**Interfaces:**
- Consumes: `try_demo_client_key`, `check_auth_rate_limit_max`
- Produces:
  - `TRY_DEMO_PER_IP_RATE_MAX = 60`
  - `TRY_DEMO_RATE_MAX = 2000` (replace 200)
  - `fn check_try_demo_rate_limits(cf_connecting_ip: Option<&str>) -> Result<(), ApiError>`
  - `try_demo_handler` takes `HeaderMap` and calls that function before assign/clone

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn try_demo_per_ip_trips_at_60_and_does_not_block_another_ip() {
    reset_auth_rate_limit_bucket_for_test("try-demo:203.0.113.10");
    reset_auth_rate_limit_bucket_for_test("try-demo:198.51.100.1");
    reset_auth_rate_limit_bucket_for_test("try-demo");
    for _ in 0..TRY_DEMO_PER_IP_RATE_MAX {
        check_try_demo_rate_limits(Some("203.0.113.10")).unwrap();
    }
    match check_try_demo_rate_limits(Some("203.0.113.10")).unwrap_err() {
        ApiError::TooManyRequests(_) => {}
        other => panic!("expected TooManyRequests, got {other:?}"),
    }
    check_try_demo_rate_limits(Some("198.51.100.1")).unwrap();
    reset_auth_rate_limit_bucket_for_test("try-demo:203.0.113.10");
    reset_auth_rate_limit_bucket_for_test("try-demo:198.51.100.1");
    reset_auth_rate_limit_bucket_for_test("try-demo");
}

#[test]
fn try_demo_per_ip_429_does_not_increment_global() {
    reset_auth_rate_limit_bucket_for_test("try-demo:203.0.113.10");
    reset_auth_rate_limit_bucket_for_test("try-demo");
    for _ in 0..TRY_DEMO_PER_IP_RATE_MAX {
        check_try_demo_rate_limits(Some("203.0.113.10")).unwrap();
    }
    let _ = check_try_demo_rate_limits(Some("203.0.113.10")).unwrap_err();
    // Global saw only the 60 accepts, not the rejected 61st.
    for _ in 0..(TRY_DEMO_RATE_MAX - TRY_DEMO_PER_IP_RATE_MAX) {
        check_auth_rate_limit_max("try-demo", TRY_DEMO_RATE_MAX).unwrap();
    }
    match check_auth_rate_limit_max("try-demo", TRY_DEMO_RATE_MAX).unwrap_err() {
        ApiError::TooManyRequests(_) => {}
        other => panic!("expected TooManyRequests, got {other:?}"),
    }
    reset_auth_rate_limit_bucket_for_test("try-demo:203.0.113.10");
    reset_auth_rate_limit_bucket_for_test("try-demo");
}

#[test]
fn try_demo_rate_limit_allows_more_than_login() {
    let bucket = "test:try-demo-rate-limit";
    reset_auth_rate_limit_bucket_for_test(bucket);
    assert!(TRY_DEMO_RATE_MAX > AUTH_RATE_MAX);
    for _ in 0..TRY_DEMO_RATE_MAX {
        check_auth_rate_limit_max(bucket, TRY_DEMO_RATE_MAX).unwrap();
    }
    let err = check_auth_rate_limit_max(bucket, TRY_DEMO_RATE_MAX).unwrap_err();
    match err {
        ApiError::TooManyRequests(_) => {}
        other => panic!("expected TooManyRequests, got {other:?}"),
    }
    reset_auth_rate_limit_bucket_for_test(bucket);
}
```

Replace the existing `try_demo_rate_limit_allows_more_than_login` body so it still asserts `TRY_DEMO_RATE_MAX > AUTH_RATE_MAX` and trips at the new 2000.

- [ ] **Step 2: Run tests and confirm they fail**

Run: `cargo test -p message-vault-server try_demo_per_ip`

Expected: FAIL (`check_try_demo_rate_limits` / `TRY_DEMO_PER_IP_RATE_MAX` missing)

- [ ] **Step 3: Implement constants, checker, and handler**

Change the Try it constant block to:

```rust
const TRY_DEMO_PER_IP_RATE_MAX: usize = 60;
const TRY_DEMO_RATE_MAX: usize = 2000;
```

Add:

```rust
fn check_try_demo_rate_limits(cf_connecting_ip: Option<&str>) -> Result<(), ApiError> {
    let per_ip = try_demo_client_key(cf_connecting_ip);
    check_auth_rate_limit_max(&per_ip, TRY_DEMO_PER_IP_RATE_MAX)?;
    check_auth_rate_limit_max("try-demo", TRY_DEMO_RATE_MAX)?;
    Ok(())
}
```

Change `try_demo_handler` to take headers and run the checker first:

```rust
pub async fn try_demo_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<AuthTokenResponse>, ApiError> {
    let cf_ip = headers
        .get("cf-connecting-ip")
        .and_then(|value| value.to_str().ok());
    check_try_demo_rate_limits(cf_ip)?;
    // ... existing body unchanged ...
}
```

`HeaderMap` is already imported. Axum header names are case-insensitive; `cf-connecting-ip` matches `CF-Connecting-IP`.

- [ ] **Step 4: Run tests**

Run:

```bash
cargo test -p message-vault-server try_demo
cargo test -p message-vault-server auth_rate_limit
```

Expected: PASS (including existing `try_demo_*` auth tests)

- [ ] **Step 5: Commit**

```bash
git add crates/vault/server/src/auth.rs
git commit -m "$(cat <<'EOF'
fix(vault): cap Try it per visitor address and raise the site-wide cap

A shared school IP can still sign in; one address cannot accept more than 60 Try it calls per minute.
EOF
)"
```

---

### Task 3: Document the two caps

**Files:**
- Modify: `docs/src/content/docs/reference/config-and-accounts.md`

**Interfaces:**
- Consumes: Task 2 constants
- Produces: operator-facing description of both caps

- [ ] **Step 1: Add a short paragraph under Guest demo pool**

After the env table (or in that section), add:

```markdown
**Try it** is limited in two ways. Each visitor internet address
(`CF-Connecting-IP` on a host behind Cloudflare) may accept 60 Try it
calls per minute. The whole server may accept 2000 per minute. People
who share one building address share the 60. If that header is missing,
those calls share one pile of 60. Cloudflare bot rules can sit in front;
this server does not configure them. Login stays 20 attempts per username
per minute.
```

No “we/us/our.” Do not tell the hosted Try it reader to install the desktop app.

- [ ] **Step 2: Commit**

```bash
git add docs/src/content/docs/reference/config-and-accounts.md
git commit -m "$(cat <<'EOF'
docs: describe the two Try it rate-limit caps

Operators can see why a shared school address is not locked out by the old site-wide 200.
EOF
)"
```

---

## Self-review

**Spec coverage**

| Spec requirement | Task |
|---|---|
| Per-address 60 / 60s | 2 |
| Process 2000 / 60s | 2 |
| `CF-Connecting-IP` only; invalid → `unknown` | 1, 2 |
| Per-address 429 does not increment global | 2 |
| Same 429 body as login | already in `check_auth_rate_limit_max` |
| Login stays 20 | 2 (unchanged call sites) |
| Docs | 3 |
| No cookie, no X-Forwarded-For, no TCP peer | 1, 2 |

**Type names:** `try_demo_client_key`, `parse_single_ip`, `check_try_demo_rate_limits`, `TRY_DEMO_PER_IP_RATE_MAX`, `TRY_DEMO_RATE_MAX`.
