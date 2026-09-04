/** @vitest-environment jsdom */

import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter, Route, Routes } from "react-router-dom";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Conversation } from "../../lib/types";
import { trashConversation } from "../../lib/vaultApi";
import { mockedAuth, VaultProviders } from "../../test/vaultProviders";
import ConversationHeader from "./ConversationHeader";

vi.mock("../../lib/auth", () => ({ useAuth: () => mockedAuth }));

vi.mock("../../lib/vaultApi", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../../lib/vaultApi")>()),
  trashConversation: vi.fn(),
}));

const trashConversationMock = vi.mocked(trashConversation);

function conversation(overrides: Partial<Conversation> = {}): Conversation {
  return {
    id: 42,
    participants: [],
    message_count: 3,
    last_message_at: "",
    date_range_start: null,
    date_range_end: null,
    service: "sms",
    is_group: false,
    label: "Chat 42",
    tags: [],
    ...overrides,
  };
}

function renderHeader(c: Conversation) {
  return render(
    <VaultProviders>
      <MemoryRouter initialEntries={["/messages/42"]}>
        <Routes>
          <Route
            path="/messages/:id"
            element={
              <ConversationHeader
                conversation={c}
                displayParticipants={[]}
                participantsOpen={false}
                onToggleParticipants={() => {}}
                sourceLabel="unknown"
                years={[]}
                activeYear={null}
                onSelectAllYears={() => {}}
                onSelectYear={() => {}}
                onShowSources={() => {}}
              />
            }
          />
          <Route path="/" element={<div>Conversations list</div>} />
        </Routes>
      </MemoryRouter>
    </VaultProviders>,
  );
}

describe("ConversationHeader", () => {
  beforeEach(() => {
    trashConversationMock.mockReset();
  });

  afterEach(() => {
    cleanup();
  });

  it("moves the conversation to trash and navigates back to the conversations list", async () => {
    trashConversationMock.mockResolvedValue(undefined);
    const user = userEvent.setup();
    renderHeader(conversation());

    const button = screen.getByRole("button", { name: "Move to trash" });
    await user.click(button);

    expect(trashConversationMock).toHaveBeenCalledWith(42, expect.anything());
    await waitFor(() => {
      expect(screen.getByText("Conversations list")).toBeInTheDocument();
    });
  });

  it("shows an error and stays put when trashing fails", async () => {
    trashConversationMock.mockRejectedValue(new Error("Could not move this conversation."));
    const user = userEvent.setup();
    renderHeader(conversation());

    await user.click(screen.getByRole("button", { name: "Move to trash" }));

    expect(await screen.findByText("Could not move this conversation.")).toBeInTheDocument();
    expect(screen.queryByText("Conversations list")).not.toBeInTheDocument();
  });
});
