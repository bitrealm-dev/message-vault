import assert from "node:assert/strict";
import { describe, it, beforeEach } from "node:test";
import {
  listGroups,
  uniqueImportGroupName,
  shouldSaveImportGroup,
  saveImportSavedGroup,
  SAVED_GROUPS_CHANGED_EVENT,
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
    assert.equal(
      uniqueImportGroupName("imessage-ios", "2026-08-11", []),
      "Import imessage-ios 2026-08-11",
    );
  });

  it("appends 2, 3 for collisions", () => {
    const names = ["Import imessage-ios 2026-08-11"];
    assert.equal(
      uniqueImportGroupName("imessage-ios", "2026-08-11", names),
      "Import imessage-ios 2026-08-11 2",
    );
    names.push("Import imessage-ios 2026-08-11 2");
    assert.equal(
      uniqueImportGroupName("imessage-ios", "2026-08-11", names),
      "Import imessage-ios 2026-08-11 3",
    );
  });
});

describe("shouldSaveImportGroup", () => {
  it("requires session id and messages_inserted > 0", () => {
    assert.equal(shouldSaveImportGroup(42, 1), true);
    assert.equal(shouldSaveImportGroup(42, 0), false);
    assert.equal(shouldSaveImportGroup(42, undefined), false);
    assert.equal(shouldSaveImportGroup(null, 5), false);
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
    assert.ok(g);
    assert.equal(g!.name, "Import imessage-ios 2026-08-11");
    assert.equal(g!.query, "import:7");
    assert.equal(listGroups().length, 1);
    assert.equal(notified, 1);
  });

  it("skips when no messages inserted", () => {
    assert.equal(
      saveImportSavedGroup({
        importSessionId: 7,
        source: "imessage-ios",
        messagesInserted: 0,
      }),
      null,
    );
    assert.equal(listGroups().length, 0);
  });
});
