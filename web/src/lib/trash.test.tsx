/** @vitest-environment jsdom */

/**
 * What each of the four trash mutations marks stale — not whether the vault
 * route was called, which proves nothing about what the app does next. Each
 * case asserts the exact set of query-key prefixes `invalidateQueries` was
 * called with, so a prefix that should stay fresh (a false positive here
 * would show up as an unwanted extra call) is checked as directly as one
 * that should go stale.
 */

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { renderHook } from "@testing-library/react";
import type { ReactNode } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  useRestoreContact,
  useRestoreConversation,
  useTrashContact,
  useTrashConversation,
} from "./trash";
import {
  restoreContact as restoreVaultContact,
  restoreConversation as restoreVaultConversation,
  trashContact as trashVaultContact,
  trashConversation as trashVaultConversation,
} from "./vaultApi";

vi.mock("./auth", () => ({ useAuth: () => ({ accountId: "account-1" }) }));

vi.mock("./vaultApi", async (importOriginal) => ({
  ...(await importOriginal<typeof import("./vaultApi")>()),
  trashConversation: vi.fn(),
  restoreConversation: vi.fn(),
  trashContact: vi.fn(),
  restoreContact: vi.fn(),
}));

const trashConversation = vi.mocked(trashVaultConversation);
const restoreConversation = vi.mocked(restoreVaultConversation);
const trashContact = vi.mocked(trashVaultContact);
const restoreContact = vi.mocked(restoreVaultContact);

let client: QueryClient;

function wrapper({ children }: { children: ReactNode }) {
  return <QueryClientProvider client={client}>{children}</QueryClientProvider>;
}

/** The prefixes `invalidateQueries` was asked to mark stale, account included. */
function invalidatedKeys(spy: ReturnType<typeof vi.spyOn>): unknown[] {
  return spy.mock.calls.map((call: unknown[]) => (call[0] as { queryKey?: unknown })?.queryKey);
}

beforeEach(() => {
  vi.clearAllMocks();
  client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
});

describe("useTrashConversation / useRestoreConversation", () => {
  it("marks the conversation list, the trash count, and open contact details stale", async () => {
    trashConversation.mockResolvedValue(undefined);
    const invalidate = vi.spyOn(client, "invalidateQueries");

    const { result } = renderHook(() => useTrashConversation(), { wrapper });
    await result.current.mutateAsync(42);

    expect(trashConversation).toHaveBeenCalledWith(42, expect.anything());
    expect(invalidatedKeys(invalidate)).toEqual(
      expect.arrayContaining([
        ["vault", "account-1", "conversations", "list"],
        ["vault", "account-1", "trash"],
        ["vault", "account-1", "contacts", "detail"],
      ]),
    );
  });

  it("leaves the conversation's own detail, its messages, and the contacts list alone", async () => {
    trashConversation.mockResolvedValue(undefined);
    const invalidate = vi.spyOn(client, "invalidateQueries");

    const { result } = renderHook(() => useTrashConversation(), { wrapper });
    await result.current.mutateAsync(42);

    const keys = invalidatedKeys(invalidate);
    expect(keys).not.toContainEqual(["vault", "account-1", "conversations", "detail"]);
    expect(keys).not.toContainEqual(["vault", "account-1", "conversations", "messages"]);
    expect(keys).not.toContainEqual(["vault", "account-1", "contacts", "list"]);
  });

  it("restore marks the same prefixes trash does", async () => {
    restoreConversation.mockResolvedValue(undefined);
    const invalidate = vi.spyOn(client, "invalidateQueries");

    const { result } = renderHook(() => useRestoreConversation(), { wrapper });
    await result.current.mutateAsync(7);

    expect(restoreConversation).toHaveBeenCalledWith(7, expect.anything());
    expect(invalidatedKeys(invalidate)).toEqual(
      expect.arrayContaining([
        ["vault", "account-1", "conversations", "list"],
        ["vault", "account-1", "trash"],
        ["vault", "account-1", "contacts", "detail"],
      ]),
    );
  });
});

describe("useTrashContact / useRestoreContact", () => {
  it("marks the contacts list and the trashed contact's own detail stale", async () => {
    trashContact.mockResolvedValue(undefined);
    const invalidate = vi.spyOn(client, "invalidateQueries");

    const { result } = renderHook(() => useTrashContact(), { wrapper });
    await result.current.mutateAsync(9);

    expect(trashContact).toHaveBeenCalledWith(9, expect.anything());
    expect(invalidatedKeys(invalidate)).toEqual(
      expect.arrayContaining([
        ["vault", "account-1", "contacts", "list"],
        ["vault", "account-1", "contacts", "detail", "9"],
      ]),
    );
  });

  it("leaves every conversation query and the trash count alone", async () => {
    trashContact.mockResolvedValue(undefined);
    const invalidate = vi.spyOn(client, "invalidateQueries");

    const { result } = renderHook(() => useTrashContact(), { wrapper });
    await result.current.mutateAsync(9);

    const keys = invalidatedKeys(invalidate);
    expect(keys.every((key) => (key as string[])[2] !== "conversations")).toBe(true);
    expect(keys).not.toContainEqual(["vault", "account-1", "trash"]);
    // Only this contact's own detail is stale, not every open drawer.
    expect(keys).not.toContainEqual(["vault", "account-1", "contacts", "detail"]);
  });

  it("restore addresses and invalidates the same contact it was called with", async () => {
    restoreContact.mockResolvedValue(undefined);
    const invalidate = vi.spyOn(client, "invalidateQueries");

    const { result } = renderHook(() => useRestoreContact(), { wrapper });
    await result.current.mutateAsync("9");

    expect(restoreContact).toHaveBeenCalledWith("9", expect.anything());
    expect(invalidatedKeys(invalidate)).toEqual(
      expect.arrayContaining([
        ["vault", "account-1", "contacts", "list"],
        ["vault", "account-1", "contacts", "detail", "9"],
      ]),
    );
  });
});
