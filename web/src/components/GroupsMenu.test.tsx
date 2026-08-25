/** @vitest-environment jsdom */

import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import GroupsMenu from "./GroupsMenu";

describe("GroupsMenu", () => {
  it("filters labeled group names as the user types", async () => {
    const user = userEvent.setup();
    render(
      <GroupsMenu
        allGroups={["College", "Family", "Work"]}
        checks={{ College: "on", Family: "off", Work: "off" }}
      />,
    );

    await user.click(screen.getByRole("button", { name: "Contact Groups" }));
    expect(screen.getByText("College")).toBeTruthy();
    expect(screen.getByText("Family")).toBeTruthy();
    expect(screen.getByText("Work")).toBeTruthy();

    await user.type(screen.getByPlaceholderText("Search groups…"), "fam");
    expect(screen.queryByText("College")).toBeNull();
    expect(screen.getByText("Family")).toBeTruthy();
    expect(screen.queryByText("Work")).toBeNull();
  });
});
