import { beforeEach, describe, expect, it } from "vitest";
import {
  listGroups,
  SAVED_GROUPS_CHANGED_EVENT,
  saveImportSavedGroup,
  shouldSaveImportGroup,
  uniqueImportGroupName,
} from "./savedGroups.ts";

const mem = new Map<string, string>();

beforeEach(() => {
  mem.clear();
  (globalThis as { localStorage?: Storage }).localStorage = {
    getItem: (k) => mem.get(k) ?? null,
    setItem: (k, v) => {
      mem.set(k, String(v));
    },
    removeItem: (k) => {
      mem.delete(k);
    },
    clear: () => mem.clear(),
    key: () => null,
    length: 0,
  };

  const events = new EventTarget();
  globalThis.addEventListener = events.addEventListener.bind(events);
  globalThis.removeEventListener = events.removeEventListener.bind(events);
  globalThis.dispatchEvent = events.dispatchEvent.bind(events);
  (globalThis as { window?: Window }).window = globalThis as unknown as Window;
});

describe("uniqueImportGroupName", () => {
  it("uses base name when free", () => {
    expect(uniqueImportGroupName("imessage-ios", "2026-08-11", [])).toBe(
      "Import imessage-ios 2026-08-11",
    );
  });

  it("appends 2, 3 for collisions", () => {
    const names = ["Import imessage-ios 2026-08-11"];
    expect(uniqueImportGroupName("imessage-ios", "2026-08-11", names)).toBe(
      "Import imessage-ios 2026-08-11 2",
    );
    names.push("Import imessage-ios 2026-08-11 2");
    expect(uniqueImportGroupName("imessage-ios", "2026-08-11", names)).toBe(
      "Import imessage-ios 2026-08-11 3",
    );
  });
});

describe("shouldSaveImportGroup", () => {
  it("requires session id and messages_inserted > 0", () => {
    expect(shouldSaveImportGroup(42, 1)).toBe(true);
    expect(shouldSaveImportGroup(42, 0)).toBe(false);
    expect(shouldSaveImportGroup(42, undefined)).toBe(false);
    expect(shouldSaveImportGroup(null, 5)).toBe(false);
  });
});

describe("saveImportSavedGroup", () => {
  it("writes group with import: query and notifies", () => {
    let notified = 0;
    const onChange = () => {
      notified += 1;
    };
    window.addEventListener(SAVED_GROUPS_CHANGED_EVENT, onChange);
    const g = saveImportSavedGroup({
      importSessionId: 7,
      source: "imessage-ios",
      messagesInserted: 3,
      now: new Date("2026-08-11T15:00:00"),
    });
    window.removeEventListener(SAVED_GROUPS_CHANGED_EVENT, onChange);
    expect(g).toBeTruthy();
    expect(g!.name).toBe("Import imessage-ios 2026-08-11");
    expect(g!.query).toBe("import:7");
    expect(listGroups()).toHaveLength(1);
    expect(notified).toBe(1);
  });

  it("skips when no messages inserted", () => {
    expect(
      saveImportSavedGroup({
        importSessionId: 7,
        source: "imessage-ios",
        messagesInserted: 0,
      }),
    ).toBeNull();
    expect(listGroups()).toHaveLength(0);
  });
});
