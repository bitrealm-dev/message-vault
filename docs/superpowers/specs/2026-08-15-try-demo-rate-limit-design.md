# Try it rate limit (two layers)

## Goal

`POST /v1/auth/try-demo` must not share one tight counter for every visitor. A classroom or office that shares one public internet address should still get in. One computer on that address must not be able to click hundreds of times in a minute. The whole server still has a high safety cap so many different addresses cannot run the handler without any upper bound.

Traffic is expected to stay small. The numbers below are larger than normal use.

## Why the current rule is wrong

The handler uses one in-memory counter named `try-demo`: **200** accepted calls in **60** seconds, then HTTP 429 for everyone. Login is keyed per username (`login:{username}`, 20 per minute), so one person does not block another. **Try it** has no username, so the first fix raised that single counter. A public page and a shared school address still share that one pile.

The hosted site sits behind Cloudflare. The TCP peer is a Cloudflare machine, not the visitor. The visitor address is in the `CF-Connecting-IP` header, which Cloudflare sets.

## Decision

Two layers, both using the existing 60-second sliding window (`check_auth_rate_limit_max`).

| Layer | Bucket | Cap / 60s | Purpose |
|---|---|---|---|
| Per visitor address | `try-demo:{ip}` | **60** | One home or one script cannot accept hundreds of Try it calls |
| Whole process | `try-demo` | **2000** | Many addresses cannot run the handler without bound |

Login, register, and Hanko stay at 20 per their existing keys.

A classroom that shares one address fits in 60 clicks per minute (about 30 people once each, plus retries). The 61st click from that address in the window is 429, even if it is a new person. The server cannot tell people apart when they share an address. A browser cookie would tell them apart; this change does not add one.

The guest pool ceiling and one-at-a-time clone still bound how many sample copies exist. These counters only bound how often the HTTP handler runs.

## Reading the address

Read `CF-Connecting-IP` only. Do not read `X-Forwarded-For` (a client can invent it if it can reach the server). Do not use the TCP peer (Cloudflare’s edge).

Accept the header when it is a single IPv4 or IPv6 address after trim. Reject empty values, lists (comma), and extra words. `std::net::IpAddr` parse is the check.

If the header is missing or rejected (self-hosted Compose, tests, no Cloudflare), use the key `try-demo:unknown` with the **same** 60-per-minute cap. Local clicks then share one pile of 60, not the 2000 global cap as a per-client limit.

## Handler order

In `try_demo_handler`, before assign or clone:

1. Per-address check (60). On 429, return. Do not increment the global counter.
2. Global check (2000). On 429, return.

A rejected call does not create a session. The 429 body stays the same as login: `too many authentication attempts; try again shortly`. Do not say which layer fired.

## Out of scope

- Browser cookies
- Checking that the TCP peer is in Cloudflare’s published ranges
- Changing the guest pool, clone lock, or session lifetime
- Cloudflare dashboard bot rules (operators may add those; the app does not)

## Tests

- Valid IPv4 and IPv6 become `try-demo:{addr}`; missing, empty, comma list, and garbage become `try-demo:unknown`.
- One address: 60 accepts, 61st is 429; a second address still accepts.
- Global: the process counter trips at 2000; login stays 20 per username.
- Missing header uses `unknown` and shares that 60-cap.

## Docs

`config-and-accounts.md`: both caps, that hosted Try it keys on `CF-Connecting-IP`, and that a shared building address shares the 60. One line that Cloudflare bot rules can sit in front.

## Acceptance

- Thirty people on one school address can each click Try it once in a minute.
- One address cannot accept more than 60 Try it calls in 60 seconds.
- The whole process cannot accept more than 2000 Try it calls in 60 seconds.
- Self-hosted without the Cloudflare header is limited to 60 Try it calls per minute for that process (the `unknown` pile).
