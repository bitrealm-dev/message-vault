import assert from "node:assert/strict";
import { afterEach, describe, it } from "node:test";

import { redoCommand, undoCommand } from "./historyRunner";
import {
  clearContactLabelsHistoryLabel,
  labelMembershipHistoryLabel,
  redoToastTextForCommand,
  renameLabelHistoryLabel,
  sortedContactIds,
  toastTextForCommand,
  undoToastTextForCommand,
  type HistoryCommand,
} from "./historyTypes";

const originalFetch = globalThis.fetch;

afterEach(() => {
  globalThis.fetch = originalFetch;
});

describe("label history", () => {
  it("formats rename and membership toast copy", () => {
    const rename: Extract<HistoryCommand, { type: "renameLabel" }> = {
      type: "renameLabel",
      from: "Work",
      to: "Office",
      label: renameLabelHistoryLabel("Work", "Office"),
    };
    assert.equal(toastTextForCommand(rename), "Renamed label Work to Office");
    assert.equal(
      undoToastTextForCommand(rename),
      "Renamed label Office to Work",
    );
    assert.equal(
      redoToastTextForCommand(rename),
      "Redid — Renamed label Work to Office",
    );
    assert.equal(rename.label, "Rename label Work to Office");

    const membership: Extract<HistoryCommand, { type: "labelMembership" }> = {
      type: "labelMembership",
      name: "Friends",
      beforeContactIds: [1, 2],
      afterContactIds: [1, 2, 3],
      label: labelMembershipHistoryLabel("Friends", [1, 2], [1, 2, 3]),
    };
    assert.equal(toastTextForCommand(membership), "Added contact to Friends");
    assert.equal(
      undoToastTextForCommand(membership),
      "Removed contact from Friends",
    );
    assert.equal(membership.label, "Add contact to Friends");

    const clearAll: Extract<HistoryCommand, { type: "labelMembership" }> = {
      type: "labelMembership",
      name: "",
      beforeContactIds: [4, 5],
      afterContactIds: [4, 5],
      clearSnapshots: [
        { contactId: 4, labels: ["A", "B"] },
        { contactId: 5, labels: ["A"] },
      ],
      label: clearContactLabelsHistoryLabel(2),
    };
    assert.equal(toastTextForCommand(clearAll), "Cleared labels for 2 contacts");
    assert.equal(
      undoToastTextForCommand(clearAll),
      "Restored labels for 2 contacts",
    );
  });

  it("sorts and deduplicates contact ids", () => {
    assert.deepEqual(sortedContactIds([3, 1, 3, 2]), [1, 2, 3]);
  });

  it("undoes and redoes rename-label", async () => {
    const requests: Array<{ url: string; init?: RequestInit }> = [];
    globalThis.fetch = (async (input, init) => {
      requests.push({ url: String(input), init });
      return new Response(JSON.stringify({ ok: true }), { status: 200 });
    }) as typeof fetch;

    const cmd: Extract<HistoryCommand, { type: "renameLabel" }> = {
      type: "renameLabel",
      from: "Work",
      to: "Office",
      label: renameLabelHistoryLabel("Work", "Office"),
    };

    await undoCommand(cmd);
    await redoCommand(cmd);

    assert.deepEqual(
      requests.map(({ url, init }) => ({
        url,
        method: init?.method,
        body: JSON.parse(String(init?.body)),
      })),
      [
        {
          url: "/api/contact-labels",
          method: "PATCH",
          body: { from: "Office", to: "Work" },
        },
        {
          url: "/api/contact-labels",
          method: "PATCH",
          body: { from: "Work", to: "Office" },
        },
      ],
    );
  });

  it("undoes and redoes label membership via batched enable/disable", async () => {
    const requests: Array<{ url: string; init?: RequestInit }> = [];
    let members = [1, 2, 3];
    globalThis.fetch = (async (input, init) => {
      const url = String(input);
      requests.push({ url, init });
      if (url.startsWith("/api/contact-labels/members")) {
        return new Response(JSON.stringify({ memberContactIds: members }), {
          status: 200,
        });
      }
      const body = JSON.parse(String(init?.body)) as {
        ids: number[];
        enable: boolean;
      };
      if (body.enable) {
        members = sortedContactIds([...members, ...body.ids]);
      } else {
        const remove = new Set(body.ids);
        members = members.filter((id) => !remove.has(id));
      }
      return new Response(JSON.stringify({ ok: true }), { status: 200 });
    }) as typeof fetch;

    const cmd: Extract<HistoryCommand, { type: "labelMembership" }> = {
      type: "labelMembership",
      name: "Friends",
      beforeContactIds: [1, 2],
      afterContactIds: [1, 2, 3],
      label: labelMembershipHistoryLabel("Friends", [1, 2], [1, 2, 3]),
    };

    await undoCommand(cmd);
    await redoCommand(cmd);

    assert.deepEqual(
      requests
        .filter(({ url }) => !url.startsWith("/api/contact-labels/members"))
        .map(({ url, init }) => ({
          url,
          method: init?.method,
          body: JSON.parse(String(init?.body)),
        })),
      [
        {
          url: "/api/contacts/labels",
          method: "POST",
          body: { ids: [3], name: "Friends", enable: false },
        },
        {
          url: "/api/contacts/labels",
          method: "POST",
          body: { ids: [3], name: "Friends", enable: true },
        },
      ],
    );
  });

  it("undoes and redoes clear-all label snapshots", async () => {
    const requests: Array<{ url: string; init?: RequestInit }> = [];
    globalThis.fetch = (async (input, init) => {
      requests.push({ url: String(input), init });
      return new Response(JSON.stringify({ ok: true }), { status: 200 });
    }) as typeof fetch;

    const cmd: Extract<HistoryCommand, { type: "labelMembership" }> = {
      type: "labelMembership",
      name: "",
      beforeContactIds: [4],
      afterContactIds: [4],
      clearSnapshots: [{ contactId: 4, labels: ["Work", "Family"] }],
      label: clearContactLabelsHistoryLabel(1),
    };

    await undoCommand(cmd);
    await redoCommand(cmd);

    assert.deepEqual(
      requests.map(({ url, init }) => ({
        url,
        method: init?.method,
        body: JSON.parse(String(init?.body)),
      })),
      [
        {
          url: "/api/contacts/4",
          method: "PATCH",
          body: { labels: ["Work", "Family"] },
        },
        {
          url: "/api/contacts/4",
          method: "PATCH",
          body: { labels: [] },
        },
      ],
    );
  });
});
