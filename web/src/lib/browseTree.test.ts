import assert from "node:assert/strict";
import { describe, it } from "node:test";
import {
  browseTreeMode,
  contactRowKey,
  directRowKey,
  flattenBrowseTree,
  groupRowKey,
  sameKindKeys,
  searchRowKey,
} from "./browseTree";
import type { CollapsedGroupConversation } from "./groupChatList";
import type { ContactListItem } from "./types";

function contact(id: number, name = `C${id}`): ContactListItem {
  return {
    id,
    displayName: name,
    preferredName: name,
    preferredHandle: null,
    handleType: null,
    firstName: name,
    lastName: null,
    sortFirst: name,
    sortLast: name,
    letter: name[0] ?? "C",
    labels: [],
    messageCount: 0,
    groupMessageCount: 0,
    dateStart: null,
    dateEnd: null,
  };
}

function group(id: number): CollapsedGroupConversation {
  return {
    conversationId: id,
    conversationIds: [id],
    title: "G",
    titleFull: "G",
    namedTitle: "G",
    participantCount: 2,
    participantNames: ["A", "B"],
    participantHandles: ["a", "b"],
    participants: [],
    messageCount: 3,
    dateStart: "2024-01-01",
    dateEnd: "2024-06-01",
    newestYear: 2024,
  };
}

describe("browseTreeMode", () => {
  it("prefers search over shared groups and browse", () => {
    assert.equal(
      browseTreeMode({ resultsMode: true, hasContactSelection: true }),
      "search",
    );
    assert.equal(
      browseTreeMode({ resultsMode: false, hasContactSelection: true }),
      "shared-groups",
    );
    assert.equal(
      browseTreeMode({ resultsMode: false, hasContactSelection: false }),
      "browse",
    );
  });
});

describe("sameKindKeys", () => {
  it("keeps only keys of the same kind as the anchor", () => {
    const keys = [
      contactRowKey(1),
      contactRowKey(2),
      groupRowKey(10),
      directRowKey(1),
      searchRowKey(5),
    ];
    assert.deepEqual(sameKindKeys(keys, contactRowKey(2)), [
      contactRowKey(1),
      contactRowKey(2),
    ]);
    assert.deepEqual(sameKindKeys(keys, groupRowKey(10)), [groupRowKey(10)]);
  });
});

describe("flattenBrowseTree", () => {
  const contacts = [contact(1), contact(2), contact(3)];
  const groups = [group(10)];

  it("flattens expanded contact with direct + groups", () => {
    const rows = flattenBrowseTree({
      mode: "browse",
      contacts,
      expandedContactId: 2,
      yearly: [
        {
          year: 2024,
          messageCount: 2,
          attachmentCount: 0,
          dateStart: "2024-01-01",
          dateEnd: "2024-02-01",
          conversationIds: [99],
        },
      ],
      groups,
      sharedGroups: [],
      searchHits: [],
    });
    assert.deepEqual(
      rows.map((r) => r.key),
      [
        contactRowKey(1),
        contactRowKey(2),
        directRowKey(2),
        groupRowKey(10),
        contactRowKey(3),
      ],
    );
  });

  it("keeps contacts visible and appends shared groups in multiselect mode", () => {
    const rows = flattenBrowseTree({
      mode: "shared-groups",
      contacts,
      expandedContactId: 1,
      yearly: [],
      groups: [],
      sharedGroups: groups,
      searchHits: [],
    });
    assert.deepEqual(
      rows.map((r) => r.key),
      [
        contactRowKey(1),
        contactRowKey(2),
        contactRowKey(3),
        groupRowKey(10),
      ],
    );
  });

  it("uses search hits in search mode", () => {
    const rows = flattenBrowseTree({
      mode: "search",
      contacts,
      expandedContactId: 1,
      yearly: [],
      groups,
      sharedGroups: groups,
      searchHits: [
        {
          conversationId: 5,
          conversationType: "group",
          contactId: null,
          title: "Hit",
          chatIdentifier: "chat-5",
          matchCount: 1,
          dateStart: null,
          dateEnd: null,
          topMatch: null,
        },
      ],
    });
    assert.deepEqual(
      rows.map((r) => r.key),
      [searchRowKey(5)],
    );
  });
});
