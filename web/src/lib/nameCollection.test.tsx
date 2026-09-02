/** @vitest-environment jsdom */

/**
 * The name-based interface over id-addressed routes.
 *
 * Screens hold names; the vault addresses a set by id. Everything about that
 * translation — where the id comes from, what happens when the name is not
 * there, and which lists go stale after a write — is this module's, so it is
 * tested here at the interface with the routes faked by name.
 */

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { renderHook, waitFor } from "@testing-library/react";
import type { ReactNode } from "react";
import { beforeEach, describe, expect, it, type Mock, vi } from "vitest";
import {
  createNameCollection,
  type NameCollectionRoutes,
  useNameCollection,
  useNameCollectionActions,
} from "./nameCollection";

vi.mock("./auth", () => ({
  useAuth: () => ({ accountId: "account-1" }),
}));

let client: QueryClient;

function wrapper({ children }: { children: ReactNode }) {
  return <QueryClientProvider client={client}>{children}</QueryClientProvider>;
}

function fakeRoutes(): { [K in keyof NameCollectionRoutes]: Mock<NameCollectionRoutes[K]> } {
  return {
    list: vi.fn().mockResolvedValue({ items: [] }),
    create: vi.fn(),
    update: vi.fn(),
    remove: vi.fn().mockResolvedValue(undefined),
    updateMembers: vi.fn().mockResolvedValue({ added: 0, removed: 0 }),
  };
}

function groupsOver(routes: NameCollectionRoutes) {
  return createNameCollection({
    routes,
    cacheKey: "contact-groups",
    invalidates: [["contacts"], ["contact-detail"]],
    label: "group",
    queryToken: "group",
    reservedNames: new Set(["trash"]),
    reservedError: (name) => `${name} is reserved`,
  });
}

const KEY = ["vault", "account-1", "contact-groups"];

beforeEach(() => {
  client = new QueryClient({
    defaultOptions: { queries: { retry: false, gcTime: 0, staleTime: 0 } },
  });
});

describe("useNameCollection", () => {
  it("answers the names in the vault's order", async () => {
    const routes = fakeRoutes();
    routes.list.mockResolvedValue({
      items: [
        { id: 2, name: "Family" },
        { id: 1, name: "Work" },
      ],
    });
    const { result } = renderHook(() => useNameCollection(groupsOver(routes)), { wrapper });
    await waitFor(() => expect(result.current.loading).toBe(false));
    expect(result.current.names).toEqual(["Family", "Work"]);
  });
});

describe("useNameCollectionActions", () => {
  it("renames by the id the cache holds and invalidates the lists that show the name", async () => {
    const routes = fakeRoutes();
    routes.update.mockResolvedValue({ id: 12, name: "Fam" });
    client.setQueryData(KEY, [{ id: 12, name: "Family" }]);
    const invalidate = vi.spyOn(client, "invalidateQueries");

    const { result } = renderHook(() => useNameCollectionActions(groupsOver(routes)), { wrapper });
    await expect(result.current.rename("Family", "Fam")).resolves.toBe("Fam");

    expect(routes.update).toHaveBeenCalledWith(12, { name: "Fam" });
    expect(routes.list).not.toHaveBeenCalled();
    const keys = invalidate.mock.calls.map((call) => call[0]?.queryKey);
    expect(keys).toEqual(
      expect.arrayContaining([
        ["vault", "account-1", "contact-groups"],
        ["vault", "account-1", "contacts"],
        ["vault", "account-1", "contact-detail"],
      ]),
    );
  });

  it("matches a name without regard to letter case", async () => {
    const routes = fakeRoutes();
    client.setQueryData(KEY, [{ id: 12, name: "Family" }]);
    const { result } = renderHook(() => useNameCollectionActions(groupsOver(routes)), { wrapper });
    await result.current.remove("family");
    expect(routes.remove).toHaveBeenCalledWith(12);
  });

  it("asks the vault once when the cache does not hold the name", async () => {
    const routes = fakeRoutes();
    routes.list.mockResolvedValue({ items: [{ id: 7, name: "Holiday" }] });
    routes.updateMembers.mockResolvedValue({ added: 2, removed: 0 });
    const { result } = renderHook(() => useNameCollectionActions(groupsOver(routes)), { wrapper });

    await expect(result.current.setMembers("Holiday", { add: [1, 2] })).resolves.toEqual({
      added: 2,
      removed: 0,
    });
    expect(routes.list).toHaveBeenCalledTimes(1);
    expect(routes.updateMembers).toHaveBeenCalledWith(7, { add: [1, 2], remove: [] });
    expect(client.getQueryData(KEY)).toEqual([{ id: 7, name: "Holiday" }]);
  });

  it("throws without a request when the vault has no set of that name", async () => {
    const routes = fakeRoutes();
    const { result } = renderHook(() => useNameCollectionActions(groupsOver(routes)), { wrapper });
    await expect(result.current.remove("Nope")).rejects.toThrow("group not found");
    expect(routes.list).toHaveBeenCalledTimes(1);
    expect(routes.remove).not.toHaveBeenCalled();
  });

  it("refuses a reserved name before asking the vault", async () => {
    const routes = fakeRoutes();
    const { result } = renderHook(() => useNameCollectionActions(groupsOver(routes)), { wrapper });
    await expect(result.current.create("Trash")).rejects.toThrow("Trash is reserved");
    expect(routes.create).not.toHaveBeenCalled();
  });

  it("answers the created name and invalidates its own list", async () => {
    const routes = fakeRoutes();
    routes.create.mockResolvedValue({ id: 3, name: "Work" });
    const invalidate = vi.spyOn(client, "invalidateQueries");
    const { result } = renderHook(() => useNameCollectionActions(groupsOver(routes)), { wrapper });
    await expect(result.current.create(" Work ")).resolves.toBe("Work");
    expect(routes.create).toHaveBeenCalledWith({ name: "Work" });
    expect(invalidate).toHaveBeenCalledWith({ queryKey: KEY });
  });
});
