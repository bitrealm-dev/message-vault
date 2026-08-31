/** @vitest-environment jsdom */

import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import BackupIdentityList from "./BackupIdentityList";

afterEach(() => {
  cleanup();
});

const profile = { phones: ["+15550001111"], emails: [] };

describe("BackupIdentityList", () => {
  it("marks matched addresses and offers to add unmatched ones", () => {
    render(
      <BackupIdentityList
        identities={["+15550001111", "owner@example.com"]}
        profile={profile}
        onAdd={vi.fn()}
      />,
    );
    expect(screen.getByText("+15550001111")).toBeInTheDocument();
    expect(screen.getByText("On your profile")).toBeInTheDocument();
    expect(screen.getByText("owner@example.com")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Add to profile" })).toBeInTheDocument();
  });

  it("sends the value and its service to onAdd", async () => {
    const onAdd = vi.fn().mockResolvedValue(undefined);
    render(
      <BackupIdentityList identities={["owner@example.com"]} profile={profile} onAdd={onAdd} />,
    );
    await userEvent.click(screen.getByRole("button", { name: "Add to profile" }));
    expect(onAdd).toHaveBeenCalledWith("owner@example.com", "email");
  });

  it("states the fact when the backup records no identities", () => {
    render(<BackupIdentityList identities={[]} profile={profile} onAdd={vi.fn()} />);
    expect(
      screen.getByText("This backup doesn't record which account it came from."),
    ).toBeInTheDocument();
  });
});
