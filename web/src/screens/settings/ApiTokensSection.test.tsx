/** @vitest-environment jsdom */

import { cleanup, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { mockedAuth, renderWithVault as render } from "../../test/vaultProviders";
import { ApiTokensSection } from "./ApiTokensSection";

const apiGet = vi.hoisted(() => vi.fn());
const apiPost = vi.hoisted(() => vi.fn());

vi.mock("../../lib/vaultApi", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../../lib/vaultApi")>()),
  listApiTokens: (...args: unknown[]) => apiGet(...args),
  createApiToken: (...args: unknown[]) => apiPost(...args),
  renameApiToken: vi.fn(),
  deleteApiToken: vi.fn(),
}));

vi.mock("../../lib/auth", () => ({ useAuth: () => mockedAuth }));

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
    const [body] = apiPost.mock.calls[0] as [unknown];
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
    expect(apiPost.mock.calls[0][0]).toEqual({
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

  it("forces a permission checkbox unchecked when the account lacks it, even though the form defaults it on", async () => {
    const user = userEvent.setup();
    render(
      <ApiTokensSection
        accountCanImport={false}
        accountCanExport={true}
        accountCanDelete={false}
      />,
    );
    await waitFor(() => {
      expect(apiGet).toHaveBeenCalled();
    });
    await user.click(screen.getByRole("button", { name: "Add" }));

    // The form defaults `canImport` to true, but the account cannot import —
    // the box must read unchecked, not checked-but-disabled.
    const importCheckbox = screen.getByRole("checkbox", { name: "Import" });
    expect(importCheckbox).toBeDisabled();
    expect(importCheckbox).not.toBeChecked();

    apiPost.mockResolvedValue({
      id: "tok_3",
      label: "No import",
      can_import: false,
      can_export: true,
      can_delete: false,
      created_at: "1700000000",
      token: "mv-api-secret3",
      token_hint: "mv-api-se..t3",
    });
    await user.type(screen.getByLabelText("API key name"), "No import");
    await user.click(screen.getByRole("button", { name: "Save" }));

    await waitFor(() => {
      expect(apiPost).toHaveBeenCalledTimes(1);
    });
    expect(apiPost.mock.calls[0][0]).toEqual({
      label: "No import",
      can_import: false,
      can_export: true,
      can_delete: false,
    });
  });
});
