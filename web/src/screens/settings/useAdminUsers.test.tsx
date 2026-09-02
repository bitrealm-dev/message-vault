/** @vitest-environment jsdom */

/**
 * What an administration write leaves behind, and what it answers.
 *
 * The panel keeps its confirmation dialog open when a delete is refused, so
 * `deleteUser` has to answer whether the vault agreed — a mutation that
 * swallows its error would close the dialog on a failure.
 */

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, renderHook, waitFor } from "@testing-library/react";
import type { ReactNode } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { deleteUser, listUsers, updateUser } from "../../lib/vaultApi";
import { useAdminUsers } from "./useAdminUsers";

vi.mock("../../lib/auth", () => ({ useAuth: () => ({ accountId: "account-1" }) }));

vi.mock("../../lib/vaultApi", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../../lib/vaultApi")>()),
  listUsers: vi.fn(),
  createUser: vi.fn(),
  updateUser: vi.fn(),
  deleteUser: vi.fn(),
  deleteUserMessages: vi.fn(),
  setUserPassword: vi.fn(),
}));

const list = vi.mocked(listUsers);
const patchUser = vi.mocked(updateUser);
const removeUser = vi.mocked(deleteUser);

let client: QueryClient;

function wrapper({ children }: { children: ReactNode }) {
  return <QueryClientProvider client={client}>{children}</QueryClientProvider>;
}

const alice = {
  account_id: "a1",
  username: "alice",
  is_admin: false,
  disabled: false,
  can_import: true,
  can_export: true,
  can_delete: false,
  message_count: 12,
  storage_bytes: 2048,
};

beforeEach(() => {
  vi.clearAllMocks();
  list.mockResolvedValue({ items: [alice] } as unknown as Awaited<ReturnType<typeof listUsers>>);
  client = new QueryClient({
    defaultOptions: { queries: { retry: false, gcTime: 0, staleTime: 0 } },
  });
});

describe("useAdminUsers", () => {
  it("asks for the list again after one account is changed", async () => {
    patchUser.mockResolvedValue(undefined);
    const { result } = renderHook(() => useAdminUsers(), { wrapper });
    await waitFor(() => expect(result.current.loading).toBe(false));
    expect(list).toHaveBeenCalledTimes(1);

    await act(async () => {
      await result.current.patch("a1", { disabled: true });
    });

    expect(patchUser).toHaveBeenCalledWith("a1", { disabled: true });
    await waitFor(() => expect(list).toHaveBeenCalledTimes(2));
  });

  it("answers false and reports why when a delete is refused", async () => {
    removeUser.mockRejectedValue(new Error("the last administrator cannot be deleted"));
    const { result } = renderHook(() => useAdminUsers(), { wrapper });
    await waitFor(() => expect(result.current.loading).toBe(false));

    let answered = true;
    await act(async () => {
      answered = await result.current.deleteUser("a1");
    });

    expect(answered).toBe(false);
    await waitFor(() =>
      expect(result.current.actionError).toBe("the last administrator cannot be deleted"),
    );
    // A refused delete still means the list in hand may be out of date.
    await waitFor(() => expect(list).toHaveBeenCalledTimes(2));
  });

  it("answers true when the vault agrees", async () => {
    removeUser.mockResolvedValue(undefined);
    const { result } = renderHook(() => useAdminUsers(), { wrapper });
    await waitFor(() => expect(result.current.loading).toBe(false));

    let answered = false;
    await act(async () => {
      answered = await result.current.deleteUser("a1");
    });

    expect(answered).toBe(true);
    expect(result.current.actionError).toBe("");
  });
});
