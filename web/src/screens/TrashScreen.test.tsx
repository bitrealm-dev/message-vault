/** @vitest-environment jsdom */

import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter } from "react-router-dom";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Conversation } from "../lib/types";
import { getConversation, listConversations, restoreConversation } from "../lib/vaultApi";
import { mockedAuth, VaultProviders } from "../test/vaultProviders";
import TrashScreen from "./TrashScreen";

vi.mock("../lib/auth", () => ({ useAuth: () => mockedAuth }));

vi.mock("../lib/vaultApi", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../lib/vaultApi")>()),
  listConversations: vi.fn(),
  getConversation: vi.fn(),
  restoreConversation: vi.fn(),
}));

const listConversationsMock = vi.mocked(listConversations);
const getConversationMock = vi.mocked(getConversation);
const restoreConversationMock = vi.mocked(restoreConversation);

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
    getConversationMock.mockReset();
    restoreConversationMock.mockReset();
    listConversationsMock.mockResolvedValue({ items: [], total: 1, limit: 1, offset: 0 });
    getConversationMock.mockResolvedValue(conversation());
    restoreConversationMock.mockResolvedValue(undefined);
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
});
