/** @vitest-environment jsdom */

import { cleanup, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { AdminUsersPanel } from "./AdminUsersPanel";

afterEach(() => {
  cleanup();
});

describe("AdminUsersPanel", () => {
  it("lists each account with its counts and flags", async () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValue(
      new Response(
        JSON.stringify({
          items: [
            {
              account_id: "a1",
              username: "alice",
              is_admin: true,
              disabled: false,
              can_import: true,
              can_export: true,
              can_delete: true,
              message_count: 1200,
              storage_bytes: 4096,
            },
            {
              account_id: "a2",
              username: "bob",
              is_admin: false,
              disabled: true,
              can_import: false,
              can_export: true,
              can_delete: false,
              message_count: 0,
              storage_bytes: 0,
            },
          ],
        }),
        { status: 200, headers: { "content-type": "application/json" } },
      ),
    );

    render(<AdminUsersPanel />);

    await waitFor(() => {
      expect(screen.getByText("alice")).toBeInTheDocument();
    });
    expect(screen.getByText("bob")).toBeInTheDocument();
    expect(screen.getByText(/1,200/)).toBeInTheDocument();
    expect(screen.getByText(/disabled/i)).toBeInTheDocument();
  });

  it("labels each permission checkbox with the account it governs", async () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValue(
      new Response(
        JSON.stringify({
          items: [
            {
              account_id: "a1",
              username: "alice",
              is_admin: true,
              disabled: false,
              can_import: true,
              can_export: true,
              can_delete: true,
              message_count: 1200,
              storage_bytes: 4096,
            },
          ],
        }),
        { status: 200, headers: { "content-type": "application/json" } },
      ),
    );

    render(<AdminUsersPanel />);

    await waitFor(() => {
      expect(screen.getByText("alice")).toBeInTheDocument();
    });
    expect(screen.getByLabelText("Allow importing messages for alice")).toBeInTheDocument();
    expect(screen.getByLabelText("Allow exporting messages for alice")).toBeInTheDocument();
    expect(
      screen.getByLabelText("Allow deleting messages and attachments for alice"),
    ).toBeInTheDocument();
  });
});
