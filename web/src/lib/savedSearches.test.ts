// @vitest-environment jsdom
import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  createSavedSearch,
  deleteSavedSearch,
  fetchSavedSearches,
  invalidateSavedSearches,
  SAVED_SEARCHES_CHANGED_EVENT,
  type SavedSearch,
  updateSavedSearch,
} from "./savedSearches";
import {
  createSavedSearch as createVaultSavedSearch,
  deleteSavedSearch as deleteVaultSavedSearch,
  listSavedSearches,
  updateSavedSearch as updateVaultSavedSearch,
} from "./vaultApi";

// Every module imports something from vaultApi, so replace only the four
// calls under test and leave the rest of the module real.
vi.mock("./vaultApi", async (importOriginal) => ({
  ...(await importOriginal<typeof import("./vaultApi")>()),
  listSavedSearches: vi.fn(),
  createSavedSearch: vi.fn(),
  updateSavedSearch: vi.fn(),
  deleteSavedSearch: vi.fn(),
}));

const get = vi.mocked(listSavedSearches);
const post = vi.mocked(createVaultSavedSearch);
const patch = vi.mocked(updateVaultSavedSearch);
const del = vi.mocked(deleteVaultSavedSearch);

function search(id: number, name: string, kind = "manual"): SavedSearch {
  return { id, name, query: `is:group ${name}`, kind };
}

describe("savedSearches", () => {
  beforeEach(() => {
    get.mockReset();
    post.mockReset();
    patch.mockReset();
    del.mockReset();
    invalidateSavedSearches();
  });

  it("reads the list from the vault, not from browser storage", async () => {
    get.mockResolvedValue({ savedSearches: [search(1, "Family")] });
    expect(await fetchSavedSearches()).toEqual([search(1, "Family")]);
    expect(get).toHaveBeenCalledWith({ signal: undefined });
  });

  it("serves a second read from cache without asking the vault again", async () => {
    get.mockResolvedValue({ savedSearches: [search(1, "Family")] });
    await fetchSavedSearches();
    await fetchSavedSearches();
    expect(get).toHaveBeenCalledTimes(1);
  });

  it("shares one request between callers that arrive together", async () => {
    get.mockResolvedValue({ savedSearches: [] });
    await Promise.all([fetchSavedSearches(), fetchSavedSearches()]);
    expect(get).toHaveBeenCalledTimes(1);
  });

  it("treats a response without a list as empty rather than throwing", async () => {
    get.mockResolvedValue({});
    expect(await fetchSavedSearches()).toEqual([]);
  });

  it("addresses an update by id, sending both fields", async () => {
    patch.mockResolvedValue({ savedSearches: [search(3, "Renamed")] });
    await updateSavedSearch(3, "Renamed", "is:direct");
    expect(patch).toHaveBeenCalledWith(3, { name: "Renamed", query: "is:direct" });
  });

  it("addresses a delete by id", async () => {
    del.mockResolvedValue({ savedSearches: [] });
    await deleteSavedSearch(7);
    expect(del).toHaveBeenCalledWith(7);
  });

  it("takes the refreshed list from a mutation instead of refetching", async () => {
    post.mockResolvedValue({ savedSearches: [search(1, "Alpha"), search(2, "Beta")] });
    const afterCreate = await createSavedSearch("Beta", "is:group");
    expect(afterCreate).toHaveLength(2);
    expect(await fetchSavedSearches()).toHaveLength(2);
    expect(get).not.toHaveBeenCalled();
  });

  it("announces a change so an open sidebar refreshes", async () => {
    const listener = vi.fn();
    globalThis.addEventListener(SAVED_SEARCHES_CHANGED_EVENT, listener);
    post.mockResolvedValue({ savedSearches: [] });
    await createSavedSearch("Family", "is:group");
    expect(listener).toHaveBeenCalledTimes(1);
    globalThis.removeEventListener(SAVED_SEARCHES_CHANGED_EVENT, listener);
  });

  it("keeps the kind the vault reports, so import rows stay identifiable", async () => {
    get.mockResolvedValue({
      savedSearches: [search(1, "Family"), search(2, "Import imessage 2026-08-30", "import")],
    });
    const kinds = (await fetchSavedSearches()).map((s) => s.kind);
    expect(kinds).toEqual(["manual", "import"]);
  });
});
