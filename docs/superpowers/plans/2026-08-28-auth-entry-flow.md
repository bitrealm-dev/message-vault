# Auth Entry Flow Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Collapse vault selection into the sign-in card and rebuild profile setup, so entering a vault takes two screens in one card that never changes size.

**Architecture:** `LoginScreen` becomes the entry point: it resolves a vault address on mount, detects the auth mode itself, and shows the address with its connection state on a line above the tabs, editable in place. `OnboardingScreen` follows on first run only. Both live in a fixed 448 × 560 flex column with the action row pinned to the bottom. Supporting changes are small and land first: the API client stops leaking JSON envelopes into error messages, service placeholders move beside the service list, and the health vocabulary becomes connecting… / connected / disconnected.

**Tech Stack:** React 19 + TypeScript, react-aria-components, Tailwind v4 (tokens in `web/src/theme.css`), Vitest + Testing Library, Biome.

**Spec:** `docs/superpowers/specs/2026-08-28-auth-entry-flow-design.md`

**Branch:** `auth-entry-flow` (already created; the spec is committed there as `4405ab2c`).

## Global Constraints

- **Web only.** No files outside `web/` change. No server changes, no new dependencies.
- **Card frame:** exactly `448 × 560` px — `w-full max-w-md` (28rem) and `h-[35rem]`. Never scrolls, never resizes, in any state.
- **Connection vocabulary:** only `connecting…`, `connected`, `disconnected`. Never "reachable", "unreachable", "answering", "checking".
- **Account rows on profile setup: maximum 3.** At the cap the Add button is replaced by the text `Add the rest in Settings after setup.`
- **No username enumeration.** No message may distinguish a wrong password from an account that does not exist. Never add one.
- **Every input is `TextField`** (`components/TextField.tsx`). `authInput` is not used on these screens.
- **Biome:** imports sorted, unused bindings prefixed `_`, no `biome-ignore` without a real reason.
- **Run from `web/`:** `npx vitest run <file>` for one file, `npm test` for all, `npm run lint` for Biome.
- Commit after every task. Do not push or open a PR unless asked.

---

### Task 1: API client stops leaking the error envelope

The vault answers errors as `{"ok":false,"error":"invalid username or password"}` and the client throws `` `${res.status}: ${text}` ``, so users read a status code and raw JSON. This task makes the thrown message the server's own sentence and attaches the status for callers that need to branch.

**Files:**
- Modify: `web/src/lib/api.ts:25-50` (the `request` helper), `web/src/lib/api.ts:56-72` (`fetchAssetObjectUrl`)
- Test: `web/src/lib/api.test.ts` (create)

**Interfaces:**
- Consumes: nothing.
- Produces: `class VaultApiError extends Error { readonly status: number }` and `errorMessageFromBody(status: number, text: string): string`, both exported from `web/src/lib/api.ts`. Task 6 and Task 7 rely on thrown errors carrying a human sentence in `.message`.

- [ ] **Step 1: Write the failing test**

Create `web/src/lib/api.test.ts`:

```ts
import { afterEach, describe, expect, it, vi } from "vitest";
import { apiClient, errorMessageFromBody, setBaseUrl, VaultApiError } from "./api";

afterEach(() => {
  vi.unstubAllGlobals();
  setBaseUrl("");
});

describe("errorMessageFromBody", () => {
  it("pulls the sentence out of the vault's error envelope", () => {
    expect(
      errorMessageFromBody(401, '{"ok":false,"error":"invalid username or password"}'),
    ).toBe("invalid username or password");
  });

  it("falls back to the raw body when it is not an envelope", () => {
    expect(errorMessageFromBody(502, "<html>Bad Gateway</html>")).toBe("<html>Bad Gateway</html>");
  });

  it("falls back to a generic sentence for an empty body", () => {
    expect(errorMessageFromBody(500, "   ")).toBe("Request failed (500)");
  });

  it("ignores an envelope whose error is blank", () => {
    expect(errorMessageFromBody(400, '{"ok":false,"error":"  "}')).toBe('{"ok":false,"error":"  "}');
  });
});

describe("apiClient errors", () => {
  it("throws a VaultApiError carrying the status and the server's message", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn().mockResolvedValue({
        ok: false,
        status: 409,
        text: async () => '{"ok":false,"error":"username already taken: matt"}',
      }),
    );

    await expect(apiClient.post("/v1/auth/register", {})).rejects.toMatchObject({
      name: "VaultApiError",
      status: 409,
      message: "username already taken: matt",
    });
  });

  it("is an Error, so existing catch blocks keep working", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn().mockResolvedValue({
        ok: false,
        status: 401,
        text: async () => '{"ok":false,"error":"invalid username or password"}',
      }),
    );

    const caught = await apiClient.get("/v1/whoami").catch((e: unknown) => e);
    expect(caught).toBeInstanceOf(Error);
    expect(caught).toBeInstanceOf(VaultApiError);
  });
});
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cd web && npx vitest run src/lib/api.test.ts`
Expected: FAIL — `errorMessageFromBody` and `VaultApiError` are not exported from `./api`.

- [ ] **Step 3: Write the implementation**

In `web/src/lib/api.ts`, add above `async function request`:

```ts
/** An error response from the vault: its own message, plus the HTTP status. */
export class VaultApiError extends Error {
  readonly status: number;

  constructor(status: number, message: string) {
    super(message);
    this.name = "VaultApiError";
    this.status = status;
  }
}

/**
 * Human-readable message for a failed response.
 *
 * The vault answers `{"ok":false,"error":"..."}`, and that sentence is what a
 * user should read — not the status code and not the envelope around it.
 * Anything else (a proxy's HTML error page, an empty body) falls back to the
 * raw text, then to a generic sentence.
 */
export function errorMessageFromBody(status: number, text: string): string {
  const trimmed = text.trim();
  if (!trimmed) return `Request failed (${status})`;

  try {
    const parsed: unknown = JSON.parse(trimmed);
    if (parsed && typeof parsed === "object" && "error" in parsed) {
      const { error } = parsed as { error: unknown };
      if (typeof error === "string" && error.trim()) return error.trim();
    }
  } catch {
    // Not JSON — the raw text is the best available message.
  }
  return trimmed;
}
```

Then replace both throw sites. In `request`:

```ts
  if (!res.ok) {
    const text = await res.text();
    throw new VaultApiError(res.status, errorMessageFromBody(res.status, text));
  }
```

And in `fetchAssetObjectUrl`:

```ts
  if (!res.ok) {
    const text = await res.text();
    throw new VaultApiError(res.status, errorMessageFromBody(res.status, text));
  }
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cd web && npx vitest run src/lib/api.test.ts`
Expected: PASS, 6 tests.

- [ ] **Step 5: Check nothing else asserted on the old format**

Run: `cd web && npm test`
Expected: PASS. (A search for tests asserting `"401: "`-style messages found none, so this should be green. If a test does fail on a message, update that assertion to the bare sentence — do not restore the prefix.)

- [ ] **Step 6: Commit**

```bash
git add web/src/lib/api.ts web/src/lib/api.test.ts
git commit -m "fix(web): surface the vault's error sentence, not its JSON envelope"
```

---

### Task 2: Per-service field examples

The account row's placeholder is a ternary: email gets `you@example.com`, everything else gets a phone number. That is already wrong for WhatsApp's own format and would be wrong for any username-shaped service. Move the example beside the service that owns it.

Note this also renames the setup picker's phone label from `Phone` to `Text message`, matching what the contact drawer already shows via `formatHandleServiceLabel`.

**Files:**
- Modify: `web/src/lib/handleService.ts:19-23` (`HANDLE_SERVICE_OPTIONS`)
- Test: `web/src/lib/handleService.test.ts` (extend)

**Interfaces:**
- Consumes: nothing.
- Produces: `handlePlaceholder(service: HandleService): string`, and `HANDLE_SERVICE_OPTIONS` entries gaining a `placeholder: string` field. Task 7 uses both.

- [ ] **Step 1: Write the failing test**

Append to `web/src/lib/handleService.test.ts`:

```ts
describe("handlePlaceholder", () => {
  it("gives each service its own example", () => {
    expect(handlePlaceholder("phone")).toBe("+1 555-123-4567");
    expect(handlePlaceholder("email")).toBe("you@example.com");
    expect(handlePlaceholder("whatsapp")).toBe("+1 555-123-4567");
  });

  it("gives every option in the picker an example", () => {
    for (const option of HANDLE_SERVICE_OPTIONS) {
      expect(option.placeholder.length).toBeGreaterThan(0);
    }
  });

  it("calls a phone number what the contact drawer calls it", () => {
    expect(HANDLE_SERVICE_OPTIONS.find((o) => o.value === "phone")?.label).toBe("Text message");
  });
});
```

Add `handlePlaceholder` and `HANDLE_SERVICE_OPTIONS` to that file's existing import from `./handleService`.

- [ ] **Step 2: Run the test to verify it fails**

Run: `cd web && npx vitest run src/lib/handleService.test.ts`
Expected: FAIL — `handlePlaceholder` is not exported.

- [ ] **Step 3: Write the implementation**

In `web/src/lib/handleService.ts`, replace `HANDLE_SERVICE_OPTIONS` and add the lookup below it:

```ts
/**
 * Services offered on setup and account profile, each with the example shown
 * in an empty field. The example lives next to the service so adding one means
 * adding its example on the same line — there is no second place to forget.
 */
export const HANDLE_SERVICE_OPTIONS = [
  { value: "phone", label: "Text message", placeholder: "+1 555-123-4567" },
  { value: "email", label: "Email", placeholder: "you@example.com" },
  { value: "whatsapp", label: "WhatsApp", placeholder: "+1 555-123-4567" },
] as const satisfies ReadonlyArray<{
  value: HandleService;
  label: string;
  placeholder: string;
}>;

/** Example shown in an empty value field for `service`. */
export function handlePlaceholder(service: HandleService): string {
  return HANDLE_SERVICE_OPTIONS.find((option) => option.value === service)?.placeholder ?? "";
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cd web && npx vitest run src/lib/handleService.test.ts`
Expected: PASS.

- [ ] **Step 5: Check the label rename broke nothing**

Run: `cd web && npm test`
Expected: PASS. If a test asserted the old `Phone` label in a picker, update it to `Text message` — the rename is intended.

- [ ] **Step 6: Commit**

```bash
git add web/src/lib/handleService.ts web/src/lib/handleService.test.ts
git commit -m "feat(web): give each handle service its own field example"
```

---

### Task 3: Connection vocabulary and vault helpers

Three words replace five. This task also adds the two helpers the sign-in card needs: the host to display, and a timeout budget for the mode probe so `connecting…` cannot hang forever.

**Files:**
- Modify: `web/src/lib/vaultHealth.ts:65-80` (`healthStatusLabel`), plus a new export at the end
- Modify: `web/src/lib/authGuards.ts` (add `vaultDisplayHost`)
- Modify: `web/src/lib/vaultHealth.test.ts:51-57`
- Modify: `web/src/screens/LoginScreen.test.tsx:67,144,170,196` (four label assertions)
- Test: `web/src/lib/authGuards.test.ts` (extend)

**Interfaces:**
- Consumes: nothing.
- Produces: `vaultDisplayHost(url: string, locationHost: string): string` from `authGuards.ts`; `probeTimeoutSignal(): AbortSignal` from `vaultHealth.ts`; `healthStatusLabel` returning `"Connected" | "Disconnected" | "Connecting…"`. Tasks 5 and 6 use all three.

- [ ] **Step 1: Write the failing tests**

Replace the `healthStatusLabel` block in `web/src/lib/vaultHealth.test.ts`:

```ts
describe("healthStatusLabel", () => {
  it("uses one vocabulary: connecting, connected, disconnected", () => {
    expect(healthStatusLabel("ok")).toBe("Connected");
    expect(healthStatusLabel("fail")).toBe("Disconnected");
    // Not yet answered and still trying read the same from the user's side.
    expect(healthStatusLabel("checking")).toBe("Connecting…");
    expect(healthStatusLabel("unknown")).toBe("Connecting…");
  });
});
```

Append to `web/src/lib/authGuards.test.ts`:

```ts
describe("vaultDisplayHost", () => {
  it("shows this page's host for a blank address", () => {
    expect(vaultDisplayHost("", "vault.bitrealm.io")).toBe("vault.bitrealm.io");
  });

  it("shows host and port for an absolute address", () => {
    expect(vaultDisplayHost("http://127.0.0.1:8080", "example.test")).toBe("127.0.0.1:8080");
  });

  it("drops a trailing path and slash", () => {
    expect(vaultDisplayHost("https://vault.example.com/", "example.test")).toBe(
      "vault.example.com",
    );
  });

  it("shows an unparseable address as typed, rather than nothing", () => {
    expect(vaultDisplayHost("not a url", "example.test")).toBe("not a url");
  });
});
```

Add `vaultDisplayHost` to that file's import from `./authGuards`.

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd web && npx vitest run src/lib/vaultHealth.test.ts src/lib/authGuards.test.ts`
Expected: FAIL — labels still say "Server reachable", and `vaultDisplayHost` is not exported.

- [ ] **Step 3: Write the implementation**

In `web/src/lib/vaultHealth.ts`, replace `healthStatusLabel` and append the signal helper:

```ts
export function healthStatusLabel(status: VaultHealthStatus): string {
  switch (status) {
    case "ok":
      return "Connected";
    case "fail":
      return "Disconnected";
    // "No answer yet" and "still trying" are the same thing to a reader, so
    // both grey states say the same word.
    case "checking":
    case "unknown":
      return "Connecting…";
    default: {
      const _exhaustive: never = status;
      return _exhaustive;
    }
  }
}

/**
 * Abort signal giving one request the same budget as one health probe.
 *
 * Used for `/v1/auth/mode` on the sign-in card: without it, a host that accepts
 * the connection and never answers leaves the card saying "connecting…" until
 * the browser's own default timeout, which can be minutes.
 */
export function probeTimeoutSignal(): AbortSignal {
  if (typeof AbortSignal !== "undefined" && typeof AbortSignal.timeout === "function") {
    return AbortSignal.timeout(HEALTH_PROBE_TIMEOUT_MS);
  }
  const controller = new AbortController();
  setTimeout(() => controller.abort(), HEALTH_PROBE_TIMEOUT_MS);
  return controller.signal;
}
```

In `web/src/lib/authGuards.ts`, append:

```ts
/**
 * Host to show on the vault line. A blank address means this origin, so the
 * line names a real host instead of showing emptiness.
 */
export function vaultDisplayHost(url: string, locationHost: string): string {
  const trimmed = url.trim();
  if (!trimmed) return locationHost;
  try {
    return new URL(trimmed).host;
  } catch {
    return trimmed;
  }
}
```

- [ ] **Step 4: Update the four assertions in the existing screen test**

In `web/src/screens/LoginScreen.test.tsx`, replace the health-dot label names — the old screen still renders `HealthDot`, so these keep passing until Task 6 rewrites the file:

- line 67: `"Server status unknown"` → `"Connecting…"`
- line 144: `"Server reachable"` → `"Connected"`
- line 170: `"Server reachable"` → `"Connected"`
- line 196: `"Server unreachable"` → `"Disconnected"`

- [ ] **Step 5: Run the full suite**

Run: `cd web && npm test`
Expected: PASS, all files.

- [ ] **Step 6: Commit**

```bash
git add web/src/lib/vaultHealth.ts web/src/lib/vaultHealth.test.ts web/src/lib/authGuards.ts web/src/lib/authGuards.test.ts web/src/screens/LoginScreen.test.tsx
git commit -m "refactor(web): one connection vocabulary, plus vault host and probe helpers"
```

---

### Task 4: The fixed card frame

The card is `w-full max-w-md … p-8` today, so its height is whatever the content happens to be. Pin it, and give callers the two class strings that put content at the top and the action row at the bottom.

**Files:**
- Modify: `web/src/lib/uiStyles.ts:4-6` (`authCard`)
- Modify: `web/src/components/AuthErrorFooter.tsx`
- Test: `web/src/components/AuthErrorFooter.test.tsx` (create)

**Interfaces:**
- Consumes: nothing.
- Produces: `authCard`, `authCardBody`, `authCardFooter` from `lib/uiStyles.ts`. Tasks 6 and 7 compose all three. `AuthErrorFooter` keeps its `{ error: string }` prop.

- [ ] **Step 1: Write the failing test**

Create `web/src/components/AuthErrorFooter.test.tsx`:

```tsx
/** @vitest-environment jsdom */

import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import AuthErrorFooter from "./AuthErrorFooter";

afterEach(cleanup);

describe("AuthErrorFooter", () => {
  it("capitalizes the server's lowercase sentence", () => {
    render(<AuthErrorFooter error="invalid username or password" />);
    expect(screen.getByText("Invalid username or password")).toBeInTheDocument();
  });

  it("leaves an already-capitalized message alone", () => {
    render(<AuthErrorFooter error="Passwords do not match." />);
    expect(screen.getByText("Passwords do not match.")).toBeInTheDocument();
  });

  it("reserves its space when there is no message", () => {
    const { container } = render(<AuthErrorFooter error="" />);
    const line = container.firstElementChild;
    expect(line).toHaveClass("min-h-10");
    expect(line).toHaveAttribute("aria-live", "polite");
  });
});
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cd web && npx vitest run src/components/AuthErrorFooter.test.tsx`
Expected: FAIL — the first case renders the lowercase string.

- [ ] **Step 3: Write the implementation**

Replace `web/src/components/AuthErrorFooter.tsx`:

```tsx
/** The vault's messages start lowercase; a line of UI text should not. */
function sentenceCase(text: string): string {
  return text ? text.charAt(0).toUpperCase() + text.slice(1) : text;
}

/**
 * Error line for an auth form. Always occupies space (transparent when empty)
 * so the surrounding form does not shift when a message appears. Callers place
 * it above the primary action, inside the card's pinned footer, so a message
 * grows upward into the card's slack rather than out of the fixed frame.
 */
export default function AuthErrorFooter({ error }: { error: string }) {
  return (
    <div
      className="mb-2 min-h-10 text-[0.813rem] leading-[1.35]"
      style={{ color: error ? "var(--danger)" : "transparent" }}
      aria-live="polite"
    >
      {sentenceCase(error) || " "}
    </div>
  );
}
```

Note the margin flipped from `mt-5` to `mb-2`: it now sits above the button rather than below it. Between this task and Tasks 6–7 the existing forms still render it *after* their submit, so the spacing looks momentarily wrong on the branch. That is expected and those tasks fix it; no test asserts on it.

In `web/src/lib/uiStyles.ts`, replace `authCard` and add the two companions:

```ts
/**
 * Every auth card is the same 448 × 560 box on every screen and in every
 * state — it never resizes and never scrolls, so nothing moves underneath the
 * user as they step through sign-in and setup.
 */
export const authCard =
  "box-border flex h-[35rem] w-full max-w-md flex-col bg-panel border border-border rounded-lg shadow-[0_4px_24px_rgba(0,0,0,0.15)] p-8";

/** Content region of an auth card: everything above the pinned action row. */
export const authCardBody = "flex min-h-0 flex-1 flex-col";

/**
 * Action row pinned to the bottom of the frame, so Sign in, Create account and
 * Continue to Vault all land on the same row on every screen.
 */
export const authCardFooter = "mt-auto";
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cd web && npx vitest run src/components/AuthErrorFooter.test.tsx && npm test`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add web/src/lib/uiStyles.ts web/src/components/AuthErrorFooter.tsx web/src/components/AuthErrorFooter.test.tsx
git commit -m "feat(web): fixed 448x560 auth card frame with a pinned action row"
```

---

### Task 5: The vault line

One row above the tabs that names the vault, says whether it is connected, and expands into an address field in place. It owns no network calls — the screen passes state in and gets intent out.

**Files:**
- Create: `web/src/screens/auth/VaultLine.tsx`
- Test: `web/src/screens/auth/VaultLine.test.tsx` (create)

**Interfaces:**
- Consumes: `healthStatusLabel`, `VaultHealthStatus` from `lib/vaultHealth`; `HealthDot`; `TextField`; `Button`.
- Produces:

```ts
export type VaultConnection = "connecting" | "connected" | "editing" | "disconnected";

export interface VaultLineProps {
  state: VaultConnection;
  host: string;
  draft: string;
  health: VaultHealthStatus;
  onDraftChange: (value: string) => void;
  onEdit: () => void;
  onCancel: () => void;
  onSubmit: () => void;
}
```

Task 6 renders exactly this.

- [ ] **Step 1: Write the failing test**

Create `web/src/screens/auth/VaultLine.test.tsx`:

```tsx
/** @vitest-environment jsdom */

import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import VaultLine, { type VaultLineProps } from "./VaultLine";

afterEach(cleanup);

function renderLine(overrides: Partial<VaultLineProps> = {}) {
  const props: VaultLineProps = {
    state: "connected",
    host: "vault.bitrealm.io",
    draft: "https://vault.bitrealm.io",
    health: "ok",
    onDraftChange: vi.fn(),
    onEdit: vi.fn(),
    onCancel: vi.fn(),
    onSubmit: vi.fn(),
    ...overrides,
  };
  render(<VaultLine {...props} />);
  return props;
}

describe("VaultLine", () => {
  it("names the host and says it is connected", () => {
    renderLine();
    expect(screen.getByText("vault.bitrealm.io")).toBeInTheDocument();
    expect(screen.getByText("connected")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Change" })).toBeInTheDocument();
    expect(screen.queryByRole("textbox", { name: "Vault address" })).not.toBeInTheDocument();
  });

  it("says connecting while a probe is in flight, with no way to change yet", () => {
    renderLine({ state: "connecting", health: "checking" });
    expect(screen.getByText("connecting…")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Change" })).not.toBeInTheDocument();
  });

  it("opens the address field when Change is pressed", async () => {
    const user = userEvent.setup();
    const props = renderLine();
    await user.click(screen.getByRole("button", { name: "Change" }));
    expect(props.onEdit).toHaveBeenCalledOnce();
  });

  it("offers the address and Use while editing", async () => {
    const user = userEvent.setup();
    const props = renderLine({ state: "editing", health: "checking" });

    expect(screen.getByRole("textbox", { name: "Vault address" })).toHaveValue(
      "https://vault.bitrealm.io",
    );
    await user.click(screen.getByRole("button", { name: "Use" }));
    expect(props.onSubmit).toHaveBeenCalledOnce();

    await user.click(screen.getByRole("button", { name: "Cancel" }));
    expect(props.onCancel).toHaveBeenCalledOnce();
  });

  it("says disconnected and offers Retry with a way forward", () => {
    renderLine({ state: "disconnected", host: "127.0.0.1:8080", health: "fail" });
    expect(screen.getByText("127.0.0.1:8080")).toBeInTheDocument();
    expect(screen.getByText("disconnected")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Retry" })).toBeInTheDocument();
    expect(
      screen.getByText("Start your vault, or enter another address."),
    ).toBeInTheDocument();
  });
});
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cd web && npx vitest run src/screens/auth/VaultLine.test.tsx`
Expected: FAIL — `./VaultLine` does not exist.

- [ ] **Step 3: Write the implementation**

Create `web/src/screens/auth/VaultLine.tsx`:

```tsx
import Button from "../../components/Button";
import HealthDot from "../../components/HealthDot";
import TextField from "../../components/TextField";
import type { VaultHealthStatus } from "../../lib/vaultHealth";

/** How the sign-in card is getting on with the vault it resolved. */
export type VaultConnection = "connecting" | "connected" | "editing" | "disconnected";

export interface VaultLineProps {
  state: VaultConnection;
  /** Host to name, never blank — a blank address shows this page's host. */
  host: string;
  /** Address being typed while the editor is open. */
  draft: string;
  health: VaultHealthStatus;
  onDraftChange: (value: string) => void;
  onEdit: () => void;
  onCancel: () => void;
  onSubmit: () => void;
}

const STATUS_WORD: Record<VaultConnection, string> = {
  connecting: "connecting…",
  connected: "connected",
  editing: "connecting…",
  disconnected: "disconnected",
};

const STATUS_COLOR: Record<VaultConnection, string> = {
  connecting: "text-muted",
  connected: "text-ok",
  editing: "text-muted",
  disconnected: "text-danger",
};

/**
 * The vault a sign-in card is talking to: its host, its connection state, and
 * the way to point somewhere else. The address sits above the password rather
 * than one screen back, so it is in front of you while you type.
 */
export default function VaultLine({
  state,
  host,
  draft,
  health,
  onDraftChange,
  onEdit,
  onCancel,
  onSubmit,
}: VaultLineProps) {
  const open = state === "editing" || state === "disconnected";

  return (
    <div className="mb-3.5">
      <div className="flex items-center gap-1.5 text-[0.75rem] text-muted">
        <span className="min-w-0 truncate font-medium text-text">{host}</span>
        <span aria-hidden="true">·</span>
        <span className={STATUS_COLOR[state]}>{STATUS_WORD[state]}</span>
        <span className="ml-auto flex items-center gap-2">
          {open ? <HealthDot status={health} /> : null}
          {state === "connected" ? (
            <Button variant="ghost" size="xs" onPress={onEdit}>
              Change
            </Button>
          ) : null}
          {state === "editing" ? (
            <Button variant="ghost" size="xs" onPress={onCancel}>
              Cancel
            </Button>
          ) : null}
        </span>
      </div>

      {open ? (
        <>
          <div className="mt-2 flex gap-2">
            <TextField
              aria-label="Vault address"
              value={draft}
              onChange={onDraftChange}
              onKeyDown={(e) => e.key === "Enter" && onSubmit()}
              placeholder="https://vault.example.com"
              className="min-w-0 flex-1"
            />
            <Button variant="secondary" size="sm" onPress={onSubmit} className="shrink-0">
              {state === "disconnected" ? "Retry" : "Use"}
            </Button>
          </div>
          <p className="mt-1 text-[0.75rem] text-muted">
            Start your vault, or enter another address.
          </p>
        </>
      ) : null}
    </div>
  );
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cd web && npx vitest run src/screens/auth/VaultLine.test.tsx`
Expected: PASS, 5 tests.

- [ ] **Step 5: Commit**

```bash
git add web/src/screens/auth/VaultLine.tsx web/src/screens/auth/VaultLine.test.tsx
git commit -m "feat(web): vault line naming the vault and its connection state"
```

---

### Task 6: Sign-in becomes the entry screen

Delete the vault-selection step. The card resolves its address on mount, detects the auth mode itself, and shows the form when the vault answers.

**Files:**
- Modify: `web/src/screens/LoginScreen.tsx` (substantial rewrite)
- Modify: `web/src/screens/auth/LocalAuthTabs.tsx` (flex column so the forms can pin their footers)
- Modify: `web/src/screens/auth/LoginForm.tsx`, `web/src/screens/auth/CreateAccountForm.tsx` (footer holds the error above the submit)
- Test: `web/src/screens/LoginScreen.test.tsx` (rewrite)

**Interfaces:**
- Consumes: `authCard`, `authCardBody`, `authCardFooter` (Task 4); `VaultLine`, `VaultConnection`, `VaultLineProps` (Task 5); `vaultDisplayHost` (Task 3); `probeTimeoutSignal` (Task 3); existing `initialLoginServerUrl`, `isAuthMode`, `useVaultHealth`, `LocalAuthTabs`.
- Produces: `LocalAuthTabs` gains a `disabled?: boolean` prop, forwarded to both forms as `disabled`.

**Decision to carry:** when the vault has never answered, the auth mode is unknown, so the card shows a skeleton rather than guessing at tabs. Once a mode is known, a later failure dims the real form instead. The spec's state table says "dimmed" for `disconnected`; this is how that is honored without inventing a mode.

- [ ] **Step 1: Write the failing test**

Replace `web/src/screens/LoginScreen.test.tsx` entirely:

```tsx
/** @vitest-environment jsdom */

import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter } from "react-router-dom";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const login = vi.fn();
const setServer = vi.fn();

vi.mock("../lib/auth", () => ({
  useAuth: () => ({ login, setServer, serverUrl: "" }),
}));

vi.mock("../lib/tauri-check", () => ({
  isTauri: () => false,
}));

import LoginScreen from "./LoginScreen";

/** Answer `/v1/auth/mode` and `/health` as a healthy local-auth vault. */
function stubVault(mode: "local" | "hanko" = "local") {
  vi.stubGlobal(
    "fetch",
    vi.fn(async (url: string) => {
      if (String(url).endsWith("/v1/auth/mode")) {
        return {
          ok: true,
          json: async () => ({ mode, hanko_api_url: mode === "hanko" ? "https://hanko.test" : null }),
        };
      }
      return { ok: true, text: async () => "" };
    }),
  );
}

function renderScreen() {
  render(
    <MemoryRouter>
      <LoginScreen />
    </MemoryRouter>,
  );
}

describe("LoginScreen", () => {
  beforeEach(() => {
    login.mockReset();
    setServer.mockReset();
  });

  afterEach(() => {
    cleanup();
    vi.unstubAllGlobals();
  });

  it("signs in without a vault-selection step", async () => {
    stubVault();
    renderScreen();

    expect(await screen.findByRole("tab", { name: "Login" })).toBeInTheDocument();
    expect(screen.getByRole("textbox", { name: "Username" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Sign in" })).toBeInTheDocument();

    expect(screen.queryByRole("button", { name: "Connect" })).not.toBeInTheDocument();
    expect(screen.queryByRole("textbox", { name: "Server URL" })).not.toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: "Back to Vault Selection" }),
    ).not.toBeInTheDocument();
  });

  it("names the vault it connected to", async () => {
    stubVault();
    renderScreen();

    expect(await screen.findByText("connected")).toBeInTheDocument();
    expect(setServer).toHaveBeenCalledWith("");
  });

  it("keeps both tabs, Login first", async () => {
    stubVault();
    renderScreen();

    await screen.findByRole("tab", { name: "Login" });
    const tabs = screen.getAllByRole("tab");
    expect(tabs.map((t) => t.textContent)).toEqual(["Login", "Create Account"]);
    expect(tabs[0]).toHaveAttribute("aria-selected", "true");
  });

  it("still asks for the password twice on Create Account", async () => {
    stubVault();
    const user = userEvent.setup();
    renderScreen();

    await screen.findByRole("tab", { name: "Create Account" });
    await user.click(screen.getByRole("tab", { name: "Create Account" }));

    expect(screen.getByLabelText("Password")).toBeInTheDocument();
    expect(screen.getByLabelText("Confirm Password")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Create account" })).toBeInTheDocument();
  });

  it("rejects a new account when the two passwords disagree", async () => {
    stubVault();
    const user = userEvent.setup();
    renderScreen();

    await screen.findByRole("tab", { name: "Create Account" });
    await user.click(screen.getByRole("tab", { name: "Create Account" }));
    await user.type(screen.getByRole("textbox", { name: "Username" }), "ada");
    await user.type(screen.getByLabelText("Password"), "hunter22");
    await user.type(screen.getByLabelText("Confirm Password"), "hunter23");
    await user.click(screen.getByRole("button", { name: "Create account" }));

    expect(await screen.findByText("Passwords do not match.")).toBeInTheDocument();
    expect(login).not.toHaveBeenCalled();
  });

  it("offers the address field when nothing answers", async () => {
    vi.stubGlobal("fetch", vi.fn().mockRejectedValue(new TypeError("Failed to fetch")));
    renderScreen();

    expect(await screen.findByText("disconnected")).toBeInTheDocument();
    expect(screen.getByRole("textbox", { name: "Vault address" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Retry" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Sign in" })).not.toBeInTheDocument();
  });

  it("retries against a typed address", async () => {
    vi.stubGlobal("fetch", vi.fn().mockRejectedValue(new TypeError("Failed to fetch")));
    const user = userEvent.setup();
    renderScreen();

    await screen.findByText("disconnected");

    stubVault();
    const field = screen.getByRole("textbox", { name: "Vault address" });
    await user.clear(field);
    await user.type(field, "http://127.0.0.1:8080");
    await user.click(screen.getByRole("button", { name: "Retry" }));

    expect(await screen.findByRole("tab", { name: "Login" })).toBeInTheDocument();
    await waitFor(() => {
      expect(setServer).toHaveBeenCalledWith("http://127.0.0.1:8080");
    });
  });

  it("opens the address field from Change without losing the form", async () => {
    stubVault();
    const user = userEvent.setup();
    renderScreen();

    await screen.findByRole("tab", { name: "Login" });
    await user.click(screen.getByRole("button", { name: "Change" }));

    expect(screen.getByRole("textbox", { name: "Vault address" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Use" })).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: "Login" })).toBeInTheDocument();
  });

  it("renders Hanko sign-in when the vault says so", async () => {
    stubVault("hanko");
    renderScreen();

    await screen.findByText("connected");
    expect(screen.queryByRole("tab", { name: "Login" })).not.toBeInTheDocument();
  });
});
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cd web && npx vitest run src/screens/LoginScreen.test.tsx`
Expected: FAIL — the screen still starts on the Server URL card and needs a Connect press.

- [ ] **Step 3: Let the tabs and forms pin their own footer**

In `web/src/screens/auth/LocalAuthTabs.tsx`, take a `disabled` prop and make the tab stack fill the card:

```tsx
export default function LocalAuthTabs({
  serverUrl,
  disabled = false,
}: {
  serverUrl: string;
  disabled?: boolean;
}) {
  return (
    <Tabs defaultSelectedKey="login" className="flex min-h-0 flex-1 flex-col">
```

and give both panels the same treatment, passing `disabled` through:

```tsx
      <TabPanel id="login" className="flex min-h-0 flex-1 flex-col outline-none">
        <LoginForm serverUrl={serverUrl} disabled={disabled} />
      </TabPanel>
      <TabPanel id="create" className="flex min-h-0 flex-1 flex-col outline-none">
        <CreateAccountForm serverUrl={serverUrl} disabled={disabled} />
      </TabPanel>
```

In `web/src/screens/auth/LoginForm.tsx`, add `authCardFooter` to the import from `../../lib/uiStyles`, then replace the signature and the whole `return`. The fragment becomes a flex column so `mt-auto` on the footer has something to push against:

```tsx
export default function LoginForm({
  serverUrl,
  disabled = false,
}: {
  serverUrl: string;
  disabled?: boolean;
}) {
```

```tsx
  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <TextField
        label="Username"
        value={username}
        onChange={setUsername}
        onKeyDown={(e) => e.key === "Enter" && submit()}
        autoComplete="username"
        isDisabled={disabled}
      />

      <PasswordField
        label="Password"
        className="mt-3"
        value={password}
        onChange={setPassword}
        onKeyDown={(e) => e.key === "Enter" && submit()}
        autoComplete="current-password"
        showPassword={showPassword}
        onToggle={() => setShowPassword((v) => !v)}
        isDisabled={disabled}
      />

      <div className={authCardFooter}>
        <AuthErrorFooter error={error} />
        <AuthSubmitButton onClick={submit} disabled={busy || disabled}>
          {busy ? "Signing in…" : "Sign in"}
        </AuthSubmitButton>
      </div>
    </div>
  );
```

Then the same in `web/src/screens/auth/CreateAccountForm.tsx` — same import, same signature shape, same footer, its own fields:

```tsx
export default function CreateAccountForm({
  serverUrl,
  disabled = false,
}: {
  serverUrl: string;
  disabled?: boolean;
}) {
```

```tsx
  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <TextField
        label="Username"
        value={username}
        onChange={setUsername}
        onKeyDown={(e) => e.key === "Enter" && submit()}
        autoComplete="username"
        isDisabled={disabled}
      />

      <PasswordField
        label="Password"
        className="mt-3"
        value={password}
        onChange={setPassword}
        onKeyDown={(e) => e.key === "Enter" && submit()}
        autoComplete="new-password"
        showPassword={showPassword}
        onToggle={() => setShowPassword((v) => !v)}
        isDisabled={disabled}
      />
      <p className="mt-1 text-[0.75rem] text-muted">At least 8 characters.</p>

      <PasswordField
        label="Confirm Password"
        className="mt-3"
        value={confirmPassword}
        onChange={setConfirmPassword}
        onKeyDown={(e) => e.key === "Enter" && submit()}
        autoComplete="new-password"
        showPassword={showConfirm}
        onToggle={() => setShowConfirm((v) => !v)}
        isDisabled={disabled}
      />

      <div className={authCardFooter}>
        <AuthErrorFooter error={error} />
        <AuthSubmitButton onClick={submit} disabled={busy || disabled}>
          {busy ? "Creating account…" : "Create account"}
        </AuthSubmitButton>
      </div>
    </div>
  );
```

`AuthSubmitButton` already carries `mt-6 w-full`; leave it. `TextField` and `PasswordField` both wrap react-aria's `TextField`, so `isDisabled` is the prop they take — not `disabled`.

- [ ] **Step 4: Rewrite the screen**

Replace `web/src/screens/LoginScreen.tsx`:

```tsx
import { useCallback, useEffect, useRef, useState } from "react";
import AuthErrorFooter from "../components/AuthErrorFooter";
import { apiClient, setBaseUrl } from "../lib/api";
import { useAuth } from "../lib/auth";
import {
  type AuthMode,
  initialLoginServerUrl,
  isAuthMode,
  type SessionResponse,
  vaultDisplayHost,
} from "../lib/authGuards";
import { isTauri } from "../lib/tauri-check";
import { authCard, authCardBody, authCardFooter, pageCenter } from "../lib/uiStyles";
import { useAsyncAction } from "../lib/useAsyncAction";
import { useVaultHealth } from "../lib/useVaultHealth";
import { probeTimeoutSignal } from "../lib/vaultHealth";
import LocalAuthTabs from "./auth/LocalAuthTabs";
import VaultLine, { type VaultConnection } from "./auth/VaultLine";

interface AuthModeResponse {
  mode: string;
  hanko_api_url?: string | null;
  try_demo?: boolean;
}

/** Placeholder shaped like the form, so the card does not flicker into shape. */
function FormSkeleton({ dimmed }: { dimmed: boolean }) {
  return (
    <div className={dimmed ? "opacity-40" : ""} aria-hidden="true">
      <div className="mb-6 h-9 rounded bg-elevated" />
      <div className="h-3.5 w-1/3 rounded bg-elevated" />
      <div className="mt-2 h-10 rounded bg-elevated" />
      <div className="mt-5 h-3.5 w-1/4 rounded bg-elevated" />
      <div className="mt-2 h-10 rounded bg-elevated" />
    </div>
  );
}

/**
 * The way into a vault. The card resolves an address on mount and detects the
 * auth mode itself, so the only question the old first screen asked — which
 * vault — is answered by default and changed in place when the default is
 * wrong.
 */
export default function LoginScreen() {
  const { login, setServer: setAuthServer, serverUrl: savedUrl } = useAuth();
  const [address, setAddress] = useState(() => initialLoginServerUrl(savedUrl, isTauri()));
  const [draft, setDraft] = useState(address);
  const [state, setState] = useState<VaultConnection>("connecting");
  const [authMode, setAuthMode] = useState<AuthMode | null>(null);
  const [hankoApiUrl, setHankoApiUrl] = useState<string | null>(null);
  const { error, run, clearError } = useAsyncAction();
  const [hankoError, setHankoError] = useState("");

  const editorOpen = state === "editing" || state === "disconnected";
  // Only probe while the address is being chosen. Once connected, the mode
  // request has already proved the vault is there.
  const health = useVaultHealth(editorOpen ? draft : null);
  const hankoRef = useRef<HTMLDivElement>(null);

  const connect = useCallback(
    async (url: string) => {
      const trimmed = url.trim();
      setState("connecting");
      clearError();
      setHankoError("");
      setBaseUrl(trimmed);
      try {
        const res = await apiClient.get<AuthModeResponse>("/v1/auth/mode", {
          signal: probeTimeoutSignal(),
        });
        setAddress(trimmed);
        setDraft(trimmed);
        setAuthMode(isAuthMode(res.mode) ? res.mode : "local");
        setHankoApiUrl(res.hanko_api_url || null);
        setAuthServer(trimmed);
        setState("connected");
      } catch {
        // Nothing answered. That is the vault line's problem, not the form's.
        setState("disconnected");
      }
    },
    [clearError, setAuthServer],
  );

  // Resolve the vault once on mount; Use and Retry call `connect` again.
  const started = useRef(false);
  useEffect(() => {
    if (started.current) return;
    started.current = true;
    void connect(address);
  }, [connect, address]);

  useEffect(() => {
    if (state !== "connected" || authMode !== "hanko" || !hankoApiUrl || !hankoRef.current) return;

    let cancelled = false;
    // `loadHanko` is async, so anything it returns is a promise the effect
    // cannot use as a cleanup — the unsubscribe has to be handed back this way
    // or every run leaks a Hanko instance and its session listener.
    let unsubscribe: (() => void) | null = null;

    const loadHanko = async () => {
      try {
        const mod = await import("@teamhanko/hanko-elements");
        if (cancelled) return;

        mod.register(hankoApiUrl).catch(() => {
          if (!cancelled) setHankoError("Failed to load Hanko sign-in.");
        });

        const hanko = new mod.Hanko(hankoApiUrl);
        const remove = hanko.onSessionCreated(() => {
          if (cancelled) return;
          void run(async () => {
            const jwt = hanko.getSessionToken();
            setBaseUrl(address);
            const res = await apiClient.post<SessionResponse>("/v1/auth/hanko/session", {
              hanko_jwt: jwt,
            });
            await login(address, res.token, res.account_id);
          });
        });

        if (cancelled) {
          remove();
          return;
        }
        unsubscribe = remove;
      } catch {
        if (!cancelled) {
          setHankoError("Failed to load Hanko. Is @teamhanko/hanko-elements installed?");
        }
      }
    };

    void loadHanko();

    return () => {
      cancelled = true;
      unsubscribe?.();
    };
  }, [state, authMode, hankoApiUrl, address, login, run]);

  const host = vaultDisplayHost(
    state === "connected" ? address : draft,
    typeof window === "undefined" ? "" : window.location.host,
  );

  return (
    <div className={pageCenter}>
      <div className={authCard}>
        <div className={authCardBody}>
          <VaultLine
            state={state}
            host={host}
            draft={draft}
            health={health}
            onDraftChange={setDraft}
            onEdit={() => setState("editing")}
            onCancel={() => {
              setDraft(address);
              setState("connected");
            }}
            onSubmit={() => void connect(draft)}
          />

          {authMode === null ? (
            <FormSkeleton dimmed={state === "disconnected"} />
          ) : authMode === "local" ? (
            <LocalAuthTabs serverUrl={address} disabled={state !== "connected"} />
          ) : (
            <div className={state === "connected" ? "" : "opacity-40"}>
              <div ref={hankoRef}>
                {hankoApiUrl ? (
                  <hanko-auth />
                ) : (
                  <div className="p-4 text-center text-[0.875rem] text-muted">
                    Hanko API URL not configured on server.
                  </div>
                )}
              </div>
            </div>
          )}
        </div>

        {authMode === "local" ? null : (
          <div className={authCardFooter}>
            <AuthErrorFooter error={error || hankoError} />
          </div>
        )}
      </div>
    </div>
  );
}
```

Note: `LocalAuthTabs` carries its own footer per form, so the screen only renders one when the mode is not local.

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cd web && npx vitest run src/screens/LoginScreen.test.tsx`
Expected: PASS, 9 tests.

- [ ] **Step 6: Typecheck and lint**

Run: `cd web && npx tsc --noEmit && npm run lint`
Expected: clean. (`npm run lint` runs Biome; fix sorting or unused-binding complaints rather than suppressing them.)

- [ ] **Step 7: Commit**

```bash
git add web/src/screens/LoginScreen.tsx web/src/screens/LoginScreen.test.tsx web/src/screens/auth/LocalAuthTabs.tsx web/src/screens/auth/LoginForm.tsx web/src/screens/auth/CreateAccountForm.tsx
git commit -m "feat(web): sign in is the entry screen, with the vault named in place"
```

---

### Task 7: Profile setup rebuilt

Nine changes to the busiest card, all agreed in the spec.

**Files:**
- Modify: `web/src/screens/OnboardingScreen.tsx` (substantial rewrite)
- Test: `web/src/screens/OnboardingScreen.test.tsx` (create)

**Interfaces:**
- Consumes: `handlePlaceholder`, `HANDLE_SERVICE_OPTIONS` (Task 2); `authCard`, `authCardBody`, `authCardFooter` (Task 4); existing `AuthBackButton`, `AuthSubmitButton`, `AuthErrorFooter`, `TextField`, `Select`, `Button`.
- Produces: nothing consumed elsewhere.

- [ ] **Step 1: Write the failing test**

Create `web/src/screens/OnboardingScreen.test.tsx`:

```tsx
/** @vitest-environment jsdom */

import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const logout = vi.fn();

vi.mock("../lib/auth", () => ({
  useAuth: () => ({
    login: vi.fn(),
    logout,
    token: "t",
    serverUrl: "",
    accountId: "acct",
  }),
}));

import OnboardingScreen from "./OnboardingScreen";

const rowValue = (n: number) => screen.getByRole("textbox", { name: `Account ${n} value` });

describe("OnboardingScreen", () => {
  beforeEach(() => {
    logout.mockReset();
  });

  afterEach(cleanup);

  it("names the section Your Accounts and explains nothing further", () => {
    render(<OnboardingScreen />);

    expect(screen.getByText("Your Accounts")).toBeInTheDocument();
    expect(screen.queryByText(/How you show up/i)).not.toBeInTheDocument();
    expect(screen.queryByText(/Source Accounts/i)).not.toBeInTheDocument();
    expect(screen.queryByText(/Welcome to the Message Vault/i)).not.toBeInTheDocument();
  });

  it("shows an example in the empty value field", () => {
    render(<OnboardingScreen />);
    expect(rowValue(1)).toHaveAttribute("placeholder", "+1 555-123-4567");
  });

  it("hides the remove control until there is more than one row", async () => {
    const user = userEvent.setup();
    render(<OnboardingScreen />);

    expect(screen.queryByRole("button", { name: "Remove account 1" })).not.toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "+ Add account" }));
    expect(screen.getByRole("button", { name: "Remove account 1" })).toBeInTheDocument();
    expect(rowValue(2)).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Remove account 2" }));
    expect(screen.queryByRole("button", { name: "Remove account 1" })).not.toBeInTheDocument();
  });

  it("stops at three accounts and points at Settings", async () => {
    const user = userEvent.setup();
    render(<OnboardingScreen />);

    await user.click(screen.getByRole("button", { name: "+ Add account" }));
    await user.click(screen.getByRole("button", { name: "+ Add account" }));

    expect(rowValue(3)).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "+ Add account" })).not.toBeInTheDocument();
    expect(screen.getByText("Add the rest in Settings after setup.")).toBeInTheDocument();
  });

  it("keeps Continue to Vault disabled until there is a name and an account", async () => {
    const user = userEvent.setup();
    render(<OnboardingScreen />);

    const submit = screen.getByRole("button", { name: "Continue to Vault" });
    expect(submit).toBeDisabled();

    await user.type(screen.getByRole("textbox", { name: "Display Name" }), "Matt");
    expect(submit).toBeDisabled();

    await user.type(rowValue(1), "+1 555-123-4567");
    expect(submit).toBeEnabled();
  });

  it("goes back one screen, to sign-in", async () => {
    const user = userEvent.setup();
    render(<OnboardingScreen />);

    await user.click(screen.getByRole("button", { name: "Back to Sign In" }));
    expect(logout).toHaveBeenCalledOnce();
  });
});
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cd web && npx vitest run src/screens/OnboardingScreen.test.tsx`
Expected: FAIL — the screen still says "Source Accounts" and "+ Add another account".

- [ ] **Step 3: Rewrite the screen**

Replace `web/src/screens/OnboardingScreen.tsx`:

```tsx
import { useState } from "react";
import AuthBackButton from "../components/AuthBackButton";
import AuthErrorFooter from "../components/AuthErrorFooter";
import AuthSubmitButton from "../components/AuthSubmitButton";
import Button from "../components/Button";
import Select, { ListBoxItem, selectItemClassName } from "../components/Select";
import TextField from "../components/TextField";
import { apiClient } from "../lib/api";
import { useAuth } from "../lib/auth";
import {
  HANDLE_SERVICE_OPTIONS,
  HANDLE_SERVICES,
  type HandleService,
  handlePlaceholder,
} from "../lib/handleService";
import { parseSelectKey } from "../lib/selectKey";
import { authCard, authCardBody, authCardFooter, authTitle, pageCenter } from "../lib/uiStyles";
import { useAsyncAction } from "../lib/useAsyncAction";

/**
 * The card never scrolls and never resizes, so the list of accounts is bounded.
 * Three covers a number, an address, and one more; longer lists finish in
 * Settings → Profile.
 */
const MAX_ACCOUNT_ROWS = 3;

interface HandleInput {
  id: string;
  handle: string;
  service: HandleService;
}

function newHandleRow(): HandleInput {
  return { id: crypto.randomUUID(), handle: "", service: "phone" };
}

export default function OnboardingScreen() {
  const { login, logout, token, serverUrl, accountId } = useAuth();
  const [displayName, setDisplayName] = useState("");
  const [handles, setHandles] = useState<HandleInput[]>(() => [newHandleRow()]);
  const { busy, error, run } = useAsyncAction();

  const addHandle = () => {
    if (handles.length >= MAX_ACCOUNT_ROWS) return;
    setHandles([...handles, newHandleRow()]);
  };

  const updateHandle = (index: number, field: "handle" | "service", value: string) => {
    const next = [...handles];
    if (field === "service") {
      const service = parseSelectKey(value, HANDLE_SERVICES);
      if (!service) return;
      next[index] = { ...next[index], service };
    } else {
      next[index] = { ...next[index], handle: value };
    }
    setHandles(next);
  };

  const removeHandle = (index: number) => {
    if (handles.length === 1) return;
    setHandles(handles.filter((_, i) => i !== index));
  };

  const handleSubmit = () => {
    void run(async () => {
      if (!token || !accountId) {
        throw new Error("Not signed in");
      }
      await apiClient.post("/v1/account/profile", {
        preferred_name: displayName.trim(),
        handles: handles
          .filter((h) => h.handle.trim())
          .map((h) => ({ handle: h.handle.trim(), service: h.service })),
      });
      // Log in again so "needs setup" is recomputed from the saved profile.
      await login(serverUrl, token, accountId);
    });
  };

  const canSubmit = Boolean(displayName.trim()) && handles.some((h) => h.handle.trim());

  return (
    <div className={pageCenter}>
      <div className={authCard}>
        <div className={authCardBody}>
          <h1 className={`${authTitle} !mb-2`}>Profile Setup</h1>
          <p className="mb-6 text-[0.875rem] text-muted">
            So we can match imported messages to you.
          </p>

          <TextField
            label="Display Name"
            value={displayName}
            onChange={setDisplayName}
            placeholder="Your name"
          />

          <div className="mt-4 mb-2 block text-[0.875rem] font-medium text-text">Your Accounts</div>

          {handles.map((h, i) => (
            <div key={h.id} className="mb-2 flex items-center gap-2">
              <Select
                selectedKey={h.service}
                onSelectionChange={(k) => {
                  const service = parseSelectKey(k, HANDLE_SERVICES);
                  if (service) updateHandle(i, "service", service);
                }}
                className="w-[140px] shrink-0"
                aria-label={`Account ${i + 1} type`}
              >
                {HANDLE_SERVICE_OPTIONS.map((s) => (
                  <ListBoxItem key={s.value} id={s.value} className={selectItemClassName}>
                    {s.label}
                  </ListBoxItem>
                ))}
              </Select>
              <TextField
                value={h.handle}
                onChange={(v) => updateHandle(i, "handle", v)}
                placeholder={handlePlaceholder(h.service)}
                className="min-w-0 flex-1"
                aria-label={`Account ${i + 1} value`}
              />
              {handles.length > 1 ? (
                <Button
                  variant="ghostDanger"
                  size="icon"
                  onPress={() => removeHandle(i)}
                  aria-label={`Remove account ${i + 1}`}
                >
                  ×
                </Button>
              ) : null}
            </div>
          ))}

          {handles.length < MAX_ACCOUNT_ROWS ? (
            <div className="mt-1 flex justify-end">
              <Button variant="secondary" size="sm" onPress={addHandle}>
                + Add account
              </Button>
            </div>
          ) : (
            <p className="mt-1.5 text-right text-[0.75rem] text-muted">
              Add the rest in Settings after setup.
            </p>
          )}
        </div>

        <div className={authCardFooter}>
          <AuthErrorFooter error={error} />
          <AuthSubmitButton onClick={handleSubmit} disabled={!canSubmit || busy}>
            {busy ? "Saving…" : "Continue to Vault"}
          </AuthSubmitButton>
          <AuthBackButton label="Back to Sign In" onClick={logout} />
        </div>
      </div>
    </div>
  );
}
```

Two details worth not losing: `AuthBackButton` renders a `Button`, so its accessible name is its label — the test finds it by `Back to Sign In`. And `authTitle` carries `mb-6`, so the `!mb-2` override is what closes the gap to the new subtitle.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cd web && npx vitest run src/screens/OnboardingScreen.test.tsx`
Expected: PASS, 6 tests.

- [ ] **Step 5: Typecheck and lint**

Run: `cd web && npx tsc --noEmit && npm run lint`
Expected: clean.

- [ ] **Step 6: Commit**

```bash
git add web/src/screens/OnboardingScreen.tsx web/src/screens/OnboardingScreen.test.tsx
git commit -m "feat(web): rebuild profile setup around Your Accounts"
```

---

### Task 8: Verify the whole flow in a browser

Unit tests do not catch a card that scrolls, a button that moves between tabs, or an unreadable color. This task is the check that the frame actually holds.

**Files:**
- Modify: none expected. Fix whatever this turns up, in the file that owns it.

**Interfaces:**
- Consumes: everything above.
- Produces: nothing.

- [ ] **Step 1: Run the full local gate**

```bash
cd web && npm run lint && npm test && npx tsc --noEmit && npm run build
```
Expected: all clean. `npm run build` matters here — it catches type errors Vitest's transform tolerates.

- [ ] **Step 2: Start a vault and the browser UI**

```bash
./scripts/run-vault-dev.sh --reset-demo   # vault on http://127.0.0.1:8080, user `demo`, empty password
cd web && npm run dev                     # http://127.0.0.1:5173
```

Use `127.0.0.1`, never `localhost` — the vault does not listen on IPv6.

- [ ] **Step 3: Walk the states with the Playwright MCP (`plugin-playwright-playwright`)**

Check each, in both light and dark (theme switcher is in Settings → Appearance):

1. Load `http://127.0.0.1:5173` — the card lands on Login / Create Account with no Connect press, and the vault line names `127.0.0.1:5173` as connected.
2. Switch between the Login and Create Account tabs — the card does not change height and the button does not move.
3. Sign in as `demo` with an empty password, then sign out — you land back on this card, one screen.
4. Sign in with a wrong password — the message reads `Invalid username or password` with no status code and no JSON, above the button.
5. Press Change — the address field opens in place and the card does not resize.
6. Stop the vault (Ctrl-C in its terminal) and reload — the line reads `disconnected`, the address field and Retry are there, and the form is dimmed. Restart the vault and press Retry.
7. Create a new account, land on profile setup, add accounts to the cap of three — the card never scrolls and the Add button gives way to the Settings note.
8. Press Back to Sign In — one screen back.

- [ ] **Step 4: Confirm the frame with a measurement, not an eyeball**

In the Playwright console on each screen:

```js
const card = document.querySelector('[class*="max-w-md"]');
[card.clientWidth, card.clientHeight, card.scrollHeight > card.clientHeight]
```
Expected: `[448, 560, false]` on every screen and in every state. A `true` in the third slot means content is overflowing the frame — fix the content, not the frame.

- [ ] **Step 5: Commit any fixes**

```bash
git add -A web/
git commit -m "fix(web): <what the browser pass turned up>"
```

If nothing needed fixing, skip this step and say so rather than making an empty commit.

---

## Notes for the executor

- **Do not "improve" the 401.** Wrong password and unknown username share one message and one timing on purpose. See the spec's "no username enumeration" section.
- **Do not add a global fetch timeout** in `api.ts`. The 8-second budget belongs at the `/v1/auth/mode` call site only; a global one would change every request in the app.
- **Four findings are deliberately out of scope** and belong in their own changes: dark-theme primary button contrast (~1.6:1), rate limiting keyed by username rather than IP, `HANKO_API_URL is not configured` being swallowed as a 500, and `password.len()` counting bytes while the message says characters. Note them, do not fix them here.
- **The mockup** for every state is at <https://claude.ai/code/artifact/64df5d0c-cac6-4a6f-a591-f5c2096b0f24>. Where it and the spec disagree, the spec wins.
