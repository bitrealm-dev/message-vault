/** @vitest-environment jsdom */

import { cleanup, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { mockedAuth, renderWithVault as render } from "../../test/vaultProviders";
import { AdminUsersPanel } from "./AdminUsersPanel";

vi.mock("../../lib/auth", () => ({ useAuth: () => mockedAuth }));

afterEach(() => {
  cleanup();
});

function jsonResponse(status: number, body: unknown) {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json" },
  });
}

function requestMethod(init: RequestInit | undefined): string {
  return (init?.method ?? "GET").toUpperCase();
}

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

  it("toggling the admin checkbox PATCHes is_admin", async () => {
    const user = userEvent.setup();
    const listItem = {
      account_id: "a1",
      username: "alice",
      is_admin: false,
      disabled: false,
      can_import: true,
      can_export: true,
      can_delete: true,
      message_count: 10,
      storage_bytes: 0,
    };
    const fetchMock = vi.spyOn(globalThis, "fetch").mockImplementation(async (input, init) => {
      const url = String(input);
      const method = requestMethod(init);
      if (method === "GET" && url === "/v1/admin/users") {
        return jsonResponse(200, { items: [listItem] });
      }
      if (method === "PATCH" && url === "/v1/admin/users/a1") {
        return jsonResponse(200, { ...listItem, is_admin: true });
      }
      throw new Error(`unexpected fetch: ${method} ${url}`);
    });

    render(<AdminUsersPanel />);
    await waitFor(() => {
      expect(screen.getByText("alice")).toBeInTheDocument();
    });

    const adminCheckbox = screen.getByLabelText("Allow alice to manage the vault");
    expect(adminCheckbox).not.toBeChecked();
    await user.click(adminCheckbox);

    await waitFor(() => {
      const patchCall = fetchMock.mock.calls.find(([, init]) => requestMethod(init) === "PATCH");
      expect(patchCall).toBeDefined();
    });
    const [patchUrl, patchInit] = fetchMock.mock.calls.find(
      ([, init]) => requestMethod(init) === "PATCH",
    ) as [string, RequestInit];
    expect(patchUrl).toBe("/v1/admin/users/a1");
    expect(JSON.parse(String(patchInit.body))).toEqual({ is_admin: true });
  });

  it("keeps the delete confirmation open and shows the server's refusal when the delete fails", async () => {
    const user = userEvent.setup();
    const listItem = {
      account_id: "a1",
      username: "alice",
      is_admin: true,
      disabled: false,
      can_import: true,
      can_export: true,
      can_delete: true,
      message_count: 5,
      storage_bytes: 0,
    };
    vi.spyOn(globalThis, "fetch").mockImplementation(async (input, init) => {
      const url = String(input);
      const method = requestMethod(init);
      if (method === "GET" && url === "/v1/admin/users") {
        return jsonResponse(200, { items: [listItem] });
      }
      if (method === "DELETE" && url === "/v1/admin/users/a1") {
        return jsonResponse(400, {
          ok: false,
          error: "this is the only administrator; promote another account first",
        });
      }
      throw new Error(`unexpected fetch: ${method} ${url}`);
    });

    render(<AdminUsersPanel />);
    await waitFor(() => {
      expect(screen.getByText("alice")).toBeInTheDocument();
    });

    await user.click(screen.getByRole("button", { name: "Delete account" }));
    const dialog = await screen.findByRole("dialog", { name: "Delete account" });
    await user.click(within(dialog).getByRole("button", { name: "Delete" }));

    await waitFor(() => {
      expect(
        within(dialog).getByText("this is the only administrator; promote another account first"),
      ).toBeInTheDocument();
    });
    // A failed delete must not read as success: the dialog stays open with
    // the server's reason inside it, rather than closing.
    expect(screen.getByRole("dialog", { name: "Delete account" })).toBeInTheDocument();
  });

  it("saving the add-user form POSTs username, password, and is_admin", async () => {
    const user = userEvent.setup();
    const created = {
      account_id: "a2",
      username: "carol",
      is_admin: true,
      disabled: false,
      can_import: true,
      can_export: true,
      can_delete: true,
      message_count: 0,
      storage_bytes: 0,
    };
    const fetchMock = vi.spyOn(globalThis, "fetch").mockImplementation(async (input, init) => {
      const url = String(input);
      const method = requestMethod(init);
      if (method === "GET" && url === "/v1/admin/users") {
        return jsonResponse(200, { items: [] });
      }
      if (method === "POST" && url === "/v1/admin/users") {
        return jsonResponse(200, created);
      }
      throw new Error(`unexpected fetch: ${method} ${url}`);
    });

    render(<AdminUsersPanel />);
    // Wait for the panel to finish loading, not merely for the request to go
    // out: the button only exists once the list has rendered.
    await user.click(await screen.findByRole("button", { name: "Add user" }));
    await user.type(screen.getByLabelText("New user's username"), "carol");
    await user.type(screen.getByLabelText("New user's password"), "hunter2hunter2");
    await user.click(screen.getByLabelText("Allow this user to manage the vault"));
    await user.click(screen.getByRole("button", { name: "Save" }));

    await waitFor(() => {
      const postCall = fetchMock.mock.calls.find(([, init]) => requestMethod(init) === "POST");
      expect(postCall).toBeDefined();
    });
    const [postUrl, postInit] = fetchMock.mock.calls.find(
      ([, init]) => requestMethod(init) === "POST",
    ) as [string, RequestInit];
    expect(postUrl).toBe("/v1/admin/users");
    expect(JSON.parse(String(postInit.body))).toEqual({
      username: "carol",
      password: "hunter2hunter2",
      is_admin: true,
    });
  });

  it("saving the reset-password dialog PUTs the new password to the account's password route", async () => {
    const user = userEvent.setup();
    const listItem = {
      account_id: "a1",
      username: "alice",
      is_admin: true,
      disabled: false,
      can_import: true,
      can_export: true,
      can_delete: true,
      message_count: 5,
      storage_bytes: 0,
    };
    const fetchMock = vi.spyOn(globalThis, "fetch").mockImplementation(async (input, init) => {
      const url = String(input);
      const method = requestMethod(init);
      if (method === "GET" && url === "/v1/admin/users") {
        return jsonResponse(200, { items: [listItem] });
      }
      if (method === "PUT" && url === "/v1/admin/users/a1/password") {
        return jsonResponse(200, { ok: true });
      }
      throw new Error(`unexpected fetch: ${method} ${url}`);
    });

    render(<AdminUsersPanel />);
    await waitFor(() => {
      expect(screen.getByText("alice")).toBeInTheDocument();
    });

    await user.click(screen.getByRole("button", { name: "Reset password" }));
    const dialog = await screen.findByRole("dialog", { name: "Reset password" });
    await user.type(within(dialog).getByLabelText("New password"), "newpassword123");
    await user.click(within(dialog).getByRole("button", { name: "Save" }));

    await waitFor(() => {
      const putCall = fetchMock.mock.calls.find(([, init]) => requestMethod(init) === "PUT");
      expect(putCall).toBeDefined();
    });
    const [putUrl, putInit] = fetchMock.mock.calls.find(
      ([, init]) => requestMethod(init) === "PUT",
    ) as [string, RequestInit];
    expect(putUrl).toBe("/v1/admin/users/a1/password");
    expect(JSON.parse(String(putInit.body))).toEqual({ password: "newpassword123" });

    await waitFor(() => {
      expect(screen.queryByRole("dialog", { name: "Reset password" })).not.toBeInTheDocument();
    });
  });
});
