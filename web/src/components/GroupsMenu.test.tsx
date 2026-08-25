/** @vitest-environment jsdom */

import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it } from "vitest";
import GroupsMenu from "./GroupsMenu";

afterEach(() => {
  cleanup();
});

const GROUPS = ["College", "Family", "Work"] as const;

function renderMenu(labeled = true) {
  return render(
    <GroupsMenu
      allGroups={[...GROUPS]}
      checks={{ College: "on", Family: "off", Work: "off" }}
      labeled={labeled}
      ariaLabel={labeled ? "Contact Groups" : "Tags"}
      title={labeled ? "Contact Groups" : "Tags"}
      searchPlaceholder={labeled ? "Search groups…" : "Search tags…"}
      emptyText={labeled ? "No groups" : "No tags"}
      noMatchText={labeled ? "No matching groups" : "No matching tags"}
    />,
  );
}

describe("GroupsMenu", () => {
  it("filters labeled group names as the user types", async () => {
    const user = userEvent.setup();
    renderMenu();

    await user.click(screen.getByRole("button", { name: "Contact Groups" }));
    expect(screen.getByText("College")).toBeTruthy();
    expect(screen.getByText("Family")).toBeTruthy();
    expect(screen.getByText("Work")).toBeTruthy();

    await user.type(screen.getByRole("searchbox", { name: "Search groups…" }), "fam");
    expect(screen.queryByText("College")).toBeNull();
    expect(screen.getByText("Family")).toBeTruthy();
    expect(screen.queryByText("Work")).toBeNull();
  });

  it("says no matching groups when the query hits nothing", async () => {
    const user = userEvent.setup();
    renderMenu();

    await user.click(screen.getByRole("button", { name: "Contact Groups" }));
    await user.type(screen.getByRole("searchbox", { name: "Search groups…" }), "zzz");
    const empty = screen.getByText("No matching groups");
    expect(empty).toBeTruthy();
    expect(empty.tagName).toBe("DIV");
    expect(empty.className).toContain("py-1.5");
    expect(empty.className).not.toContain("py-2");
    expect(screen.queryByText("No groups")).toBeNull();
  });

  it("filters the icon-only tags menu", async () => {
    const user = userEvent.setup();
    renderMenu(false);

    await user.click(screen.getByRole("button", { name: "Tags" }));
    await user.type(screen.getByRole("searchbox", { name: "Search tags…" }), "wor");
    expect(screen.getByText("Work")).toBeTruthy();
    expect(screen.queryByText("College")).toBeNull();
    expect(screen.queryByText("Family")).toBeNull();
  });
});
