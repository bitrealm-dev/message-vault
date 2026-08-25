/** @vitest-environment jsdom */

import { cleanup, render, screen } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { afterEach, describe, expect, it } from "vitest";
import GroupsNav from "./GroupsNav";

afterEach(() => {
  cleanup();
});

function renderNav(path: string) {
  return render(
    <MemoryRouter initialEntries={[path]}>
      <GroupsNav groups={["College"]} />
    </MemoryRouter>,
  );
}

function classTokens(el: Element): string[] {
  return el.className.split(/\s+/).filter(Boolean);
}

describe("GroupsNav", () => {
  it("keeps the No group active fill from navGlyphRowClass", () => {
    renderNav("/no-group");
    const btn = screen.getByRole("button", { name: "No group" });
    const tokens = classTokens(btn);
    expect(tokens).toContain("bg-hover");
    expect(tokens).toContain("font-semibold");
    expect(tokens).toContain("px-0");
    expect(tokens).not.toContain("bg-transparent");
  });

  it("puts group and No group icons in the shared 15px leading slot", () => {
    renderNav("/contacts");
    const college = screen.getByRole("button", { name: "College" });
    const noGroup = screen.getByRole("button", { name: "No group" });
    expect(college.querySelector('[class*="size-[15px]"]')).not.toBeNull();
    expect(noGroup.querySelector('[class*="size-[15px]"]')).not.toBeNull();
  });

  it("indents nested rows so icons line up with the heading title", () => {
    renderNav("/contacts");
    const college = screen.getByRole("button", { name: "College" });
    expect(college.className).toContain("pl-[calc(15px+0.5rem)]");
    const noGroupInner = screen
      .getByRole("button", { name: "No group" })
      .querySelector('[class*="pl-[calc(15px+0.5rem)]"]');
    expect(noGroupInner).not.toBeNull();
  });
});
