/** @vitest-environment jsdom */

import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { getImportContacts } from "../../../lib/vaultApi";
import ImportContactsPanel from "./ImportContactsPanel";

vi.mock("../../../lib/vaultApi", () => ({
  getImportContacts: vi.fn(),
}));

const get = vi.mocked(getImportContacts);

describe("ImportContactsPanel", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  afterEach(() => {
    cleanup();
  });

  it("asks for the contacts of the run it was given", async () => {
    get.mockResolvedValue({ contacts: [], new_count: 0, changed_count: 0 });
    render(<ImportContactsPanel importId={42} />);
    expect(await screen.findByText("This import changed no contacts.")).toBeInTheDocument();
    expect(get).toHaveBeenCalledWith(42);
  });

  it("counts new against changed", async () => {
    get.mockResolvedValue({
      contacts: [
        { id: 1, name: "Ada Lovelace", is_new: true },
        { id: 2, name: "Grace Hopper", is_new: false },
      ],
      new_count: 1,
      changed_count: 1,
    });
    render(<ImportContactsPanel importId={7} />);
    expect(await screen.findByText("1 new, 1 changed")).toBeInTheDocument();
    expect(screen.getByText("Ada Lovelace")).toBeInTheDocument();
    expect(screen.getByText("Grace Hopper")).toBeInTheDocument();
    expect(screen.getByText("New")).toBeInTheDocument();
    expect(screen.getByText("Changed")).toBeInTheDocument();
  });

  it("shows a contact the run found an address for but no name", async () => {
    get.mockResolvedValue({
      contacts: [{ id: 3, name: "", is_new: true }],
      new_count: 1,
      changed_count: 0,
    });
    render(<ImportContactsPanel importId={9} />);
    expect(await screen.findByText("(unknown)")).toBeInTheDocument();
  });

  it("shows the reason when the load fails", async () => {
    get.mockRejectedValue(new Error("no such import"));
    render(<ImportContactsPanel importId={11} />);
    expect(await screen.findByText("no such import")).toBeInTheDocument();
  });
});
