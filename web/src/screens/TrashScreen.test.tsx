/** @vitest-environment jsdom */

import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter } from "react-router-dom";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Conversation } from "../lib/types";
import {
  getConversation,
  listContacts,
  listConversations,
  restoreContact,
  restoreConversation,
} from "../lib/vaultApi";
import { mockedAuth, VaultProviders } from "../test/vaultProviders";
import TrashScreen from "./TrashScreen";

vi.mock("../lib/auth", () => ({ useAuth: () => mockedAuth }));

vi.mock("../lib/vaultApi", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../lib/vaultApi")>()),
  listConversations: vi.fn(),
  listContacts: vi.fn(),
  getConversation: vi.fn(),
  restoreConversation: vi.fn(),
  restoreContact: vi.fn(),
}));

const listConversationsMock = vi.mocked(listConversations);
const listContactsMock = vi.mocked(listContacts);
const getConversationMock = vi.mocked(getConversation);
const restoreConversationMock = vi.mocked(restoreConversation);
const restoreContactMock = vi.mocked(restoreContact);

function conversation(overrides: Partial<Conversation> = {}): Conversation {
  return {
    id: 42,
    participants: [{ name: "Ada Lovelace" }],
    message_count: 5,
    last_message_at: "",
    date_range_start: null,
    date_range_end: null,
    service: "sms",
    is_group: false,
    label: null,
    tags: [],
    ...overrides,
  };
}

/** One trashed contact as `GET /v1/contacts?q=trashed:yes` returns it. */
function contact(id: number, name: string) {
  return { id, name, handle_count: 1, last_modified: "2026-09-04T00:00:00Z" };
}

function contactPage(items: ReturnType<typeof contact>[]) {
  return { items, total: items.length, limit: 100, offset: 0 };
}

function renderAt(path: string) {
  return render(
    <VaultProviders>
      <MemoryRouter initialEntries={[path]}>
        <TrashScreen />
      </MemoryRouter>
    </VaultProviders>,
  );
}

describe("TrashScreen", () => {
  beforeEach(() => {
    listConversationsMock.mockReset();
    listContactsMock.mockReset();
    getConversationMock.mockReset();
    restoreConversationMock.mockReset();
    restoreContactMock.mockReset();
    listConversationsMock.mockResolvedValue({ items: [], total: 1, limit: 1, offset: 0 });
    listContactsMock.mockResolvedValue(contactPage([]));
    getConversationMock.mockResolvedValue(conversation());
    restoreConversationMock.mockResolvedValue(undefined);
    restoreContactMock.mockResolvedValue(undefined);
  });

  afterEach(() => {
    cleanup();
  });

  it("reports the trash count and prompts selection when nothing is selected", async () => {
    renderAt("/trash");

    expect(await screen.findByText(/in Trash\. Select one on the left to view it\./)).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Restore" })).toBeNull();
  });

  it("reads correctly when the trash is empty", async () => {
    listConversationsMock.mockResolvedValue({ items: [], total: 0, limit: 1, offset: 0 });
    renderAt("/trash");

    expect(await screen.findByText("Trash is empty.")).toBeTruthy();
    expect(screen.queryByText("Conversations")).toBeNull();
    expect(screen.queryByText("Contacts")).toBeNull();
  });

  it("shows Restore for a selected trashed conversation and calls the restore mutation", async () => {
    const user = userEvent.setup();
    renderAt("/trash?tsel=42");

    expect(await screen.findByText("Ada Lovelace")).toBeTruthy();
    const button = screen.getByRole("button", { name: "Restore" });

    await user.click(button);

    await waitFor(() => {
      expect(restoreConversationMock).toHaveBeenCalledWith(42, expect.anything());
    });
  });

  it("clears the selection once restore succeeds, leaving the row out of the trash view", async () => {
    const user = userEvent.setup();
    renderAt("/trash?tsel=42");

    expect(await screen.findByText("Ada Lovelace")).toBeTruthy();
    await user.click(screen.getByRole("button", { name: "Restore" }));

    await waitFor(() => {
      expect(screen.queryByText("Ada Lovelace")).toBeNull();
      expect(screen.queryByRole("button", { name: "Restore" })).toBeNull();
    });
    expect(
      screen.getByText(/in Trash\. Select one on the left to view it\.|Trash is empty\./),
    ).toBeTruthy();
  });

  it("shows an error and keeps the selection when restoring fails", async () => {
    restoreConversationMock.mockRejectedValue(new Error("Could not restore this conversation."));
    const user = userEvent.setup();
    renderAt("/trash?tsel=42");

    expect(await screen.findByText("Ada Lovelace")).toBeTruthy();
    await user.click(screen.getByRole("button", { name: "Restore" }));

    expect(await screen.findByText("Could not restore this conversation.")).toBeTruthy();
    expect(screen.getByText("Ada Lovelace")).toBeTruthy();
  });

  describe("trashed contacts", () => {
    it("lists trashed contacts, asking the vault for them with trashed:yes", async () => {
      listContactsMock.mockResolvedValue(contactPage([contact(7, "Grace Hopper")]));
      renderAt("/trash");

      expect(await screen.findByText("Grace Hopper")).toBeTruthy();
      expect(listContactsMock).toHaveBeenCalledWith(
        expect.objectContaining({ q: "trashed:yes" }),
        expect.anything(),
      );
    });

    it("restores a contact from its row and drops it from the list", async () => {
      const user = userEvent.setup();
      // The vault, modelled: restoring takes the contact out of the trash, so
      // the refetch the mutation triggers answers with an empty page.
      let trashed = [contact(7, "Grace Hopper")];
      listContactsMock.mockImplementation(async () => contactPage(trashed));
      restoreContactMock.mockImplementation(async () => {
        trashed = [];
      });
      renderAt("/trash");

      await user.click(await screen.findByRole("button", { name: "Restore Grace Hopper" }));

      await waitFor(() => {
        expect(restoreContactMock).toHaveBeenCalledWith(7, expect.anything());
      });

      await waitFor(() => {
        expect(screen.queryByText("Grace Hopper")).toBeNull();
      });
      expect(screen.getByText("No contacts in Trash.")).toBeTruthy();
    });

    it("says so when conversations are in the trash but no contacts are", async () => {
      renderAt("/trash");

      expect(await screen.findByText("No contacts in Trash.")).toBeTruthy();
      expect(screen.getByText(/in Trash\. Select one on the left to view it\./)).toBeTruthy();
    });

    it("shows both sections when the trash holds conversations and contacts", async () => {
      listContactsMock.mockResolvedValue(contactPage([contact(7, "Grace Hopper")]));
      renderAt("/trash");

      expect(await screen.findByText("Grace Hopper")).toBeTruthy();
      expect(screen.getByText("Conversations")).toBeTruthy();
      expect(screen.getByText("Contacts")).toBeTruthy();
    });

    it("keeps the contacts section when only contacts are in the trash", async () => {
      listConversationsMock.mockResolvedValue({ items: [], total: 0, limit: 1, offset: 0 });
      listContactsMock.mockResolvedValue(contactPage([contact(7, "Grace Hopper")]));
      renderAt("/trash");

      expect(await screen.findByText("Grace Hopper")).toBeTruthy();
      expect(screen.getByText("No conversations in Trash.")).toBeTruthy();
    });

    it("shows an error when restoring a contact fails", async () => {
      restoreContactMock.mockRejectedValue(new Error("Could not restore this contact."));
      listContactsMock.mockResolvedValue(contactPage([contact(7, "Grace Hopper")]));
      const user = userEvent.setup();
      renderAt("/trash");

      await user.click(await screen.findByRole("button", { name: "Restore Grace Hopper" }));

      expect(await screen.findByText("Could not restore this contact.")).toBeTruthy();
      expect(screen.getByText("Grace Hopper")).toBeTruthy();
    });

    it("narrows both kinds with the header search term", async () => {
      listContactsMock.mockResolvedValue(contactPage([]));
      renderAt("/trash?tq=ada");

      expect(await screen.findByText("No contacts match this search.")).toBeTruthy();
      expect(listContactsMock).toHaveBeenCalledWith(
        expect.objectContaining({ q: "trashed:yes ada" }),
        expect.anything(),
      );
    });
  });
});
