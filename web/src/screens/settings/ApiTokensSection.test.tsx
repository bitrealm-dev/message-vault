/** @vitest-environment jsdom */

import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { ApiTokensSection } from "./ApiTokensSection";

const apiGet = vi.hoisted(() => vi.fn());
const apiPost = vi.hoisted(() => vi.fn());

vi.mock("../../lib/api", () => ({
  apiClient: {
    get: (...args: unknown[]) => apiGet(...args),
    post: (...args: unknown[]) => apiPost(...args),
    patch: vi.fn(),
    delete: vi.fn(),
  },
}));

afterEach(() => {
  cleanup();
});

beforeEach(() => {
  apiGet.mockReset();
  apiPost.mockReset();
  apiGet.mockResolvedValue({ items: [] });
});

async function openComposeForm(accountCanDelete = false) {
  const user = userEvent.setup();
  render(
    <ApiTokensSection
      accountCanImport={true}
      accountCanExport={true}
      accountCanDelete={accountCanDelete}
    />,
  );
  await waitFor(() => {
    expect(apiGet).toHaveBeenCalled();
  });
  await user.click(screen.getByRole("button", { name: "Add" }));
  return user;
}

describe("ApiTokensSection create form", () => {
  it("sends exactly label, can_import, can_export, can_delete as the request body", async () => {
    apiPost.mockResolvedValue({
      id: "tok_1",
      label: "My token",
      can_import: true,
      can_export: true,
      can_delete: false,
      created_at: "1700000000",
      token: "mv-api-secret",
      token_hint: "mv-api-se..et",
    });

    const user = await openComposeForm();
    await user.type(screen.getByLabelText("API key name"), "My token");
    await user.click(screen.getByRole("button", { name: "Save" }));

    await waitFor(() => {
      expect(apiPost).toHaveBeenCalledTimes(1);
    });
    const [path, body] = apiPost.mock.calls[0] as [string, unknown];
    expect(path).toBe("/v1/account/api-tokens");
    expect(body).toEqual({
      label: "My token",
      can_import: true,
      can_export: true,
      can_delete: false,
    });
  });

  it("includes can_delete: true once the checkbox is checked", async () => {
    apiPost.mockResolvedValue({
      id: "tok_2",
      label: "Destroyer",
      can_import: true,
      can_export: true,
      can_delete: true,
      created_at: "1700000000",
      token: "mv-api-secret2",
      token_hint: "mv-api-se..t2",
    });

    const user = await openComposeForm(true);
    await user.type(screen.getByLabelText("API key name"), "Destroyer");
    await user.click(screen.getByRole("checkbox", { name: "Delete messages and attachments" }));
    await user.click(screen.getByRole("button", { name: "Save" }));

    await waitFor(() => {
      expect(apiPost).toHaveBeenCalledTimes(1);
    });
    expect(apiPost.mock.calls[0][1]).toEqual({
      label: "Destroyer",
      can_import: true,
      can_export: true,
      can_delete: true,
    });
  });

  it("disables a permission checkbox the account itself does not hold", async () => {
    const user = userEvent.setup();
    render(
      <ApiTokensSection accountCanImport={true} accountCanExport={true} accountCanDelete={false} />,
    );
    await waitFor(() => {
      expect(apiGet).toHaveBeenCalled();
    });
    await user.click(screen.getByRole("button", { name: "Add" }));

    const deleteCheckbox = screen.getByRole("checkbox", {
      name: "Delete messages and attachments",
    });
    expect(deleteCheckbox).toBeDisabled();
    expect(screen.getByText("Your account cannot do this.")).toBeTruthy();

    const importCheckbox = screen.getByRole("checkbox", { name: "Import" });
    expect(importCheckbox).not.toBeDisabled();
  });
});
