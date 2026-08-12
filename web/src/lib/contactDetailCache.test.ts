import { describe, it, expect, beforeEach, vi } from "vitest";
import {
  clearContactDetailCache,
  fetchContactDetail,
  getCachedContactDetail,
  invalidateContactDetail,
  type CachedContactDetail,
} from "./contactDetailCache";

function detail(id: string): CachedContactDetail {
  return {
    id,
    name: `Contact ${id}`,
    handles: [],
    direct_conversations: 0,
    group_conversations: 0,
    total_messages: 0,
  };
}

describe("contactDetailCache", () => {
  beforeEach(() => {
    clearContactDetailCache();
  });

  it("stores detail after fetch for getCachedContactDetail", async () => {
    const get = vi.fn(async () => detail("9"));
    const first = await fetchContactDetail("9", get);
    expect(first.name).toBe("Contact 9");
    expect(get).toHaveBeenCalledTimes(1);
    expect(getCachedContactDetail("9")).toEqual(first);
  });

  it("dedupes in-flight fetches for the same id", async () => {
    let release!: (d: CachedContactDetail) => void;
    const pending = new Promise<CachedContactDetail>((resolve) => {
      release = resolve;
    });
    const get = vi.fn(() => pending);

    const a = fetchContactDetail("1", get);
    const b = fetchContactDetail("1", get);
    expect(get).toHaveBeenCalledTimes(1);

    release(detail("1"));
    await expect(Promise.all([a, b])).resolves.toEqual([detail("1"), detail("1")]);
    expect(getCachedContactDetail("1")).toEqual(detail("1"));
  });

  it("invalidate removes cache and allows a fresh fetch", async () => {
    const get = vi
      .fn()
      .mockResolvedValueOnce(detail("2"))
      .mockResolvedValueOnce({ ...detail("2"), name: "Renamed" });

    await fetchContactDetail("2", get);
    expect(getCachedContactDetail("2")?.name).toBe("Contact 2");

    invalidateContactDetail("2");
    expect(getCachedContactDetail("2")).toBeNull();

    const again = await fetchContactDetail("2", get);
    expect(again.name).toBe("Renamed");
    expect(get).toHaveBeenCalledTimes(2);
  });

  it("clear wipes all entries", async () => {
    const get = vi.fn(async (path: string) =>
      detail(path.split("/").pop() ?? "x"),
    );
    await fetchContactDetail("3", get);
    await fetchContactDetail("4", get);
    clearContactDetailCache();
    expect(getCachedContactDetail("3")).toBeNull();
    expect(getCachedContactDetail("4")).toBeNull();
  });
});
