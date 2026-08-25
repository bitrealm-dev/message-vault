/** @vitest-environment jsdom */

import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import {
  Cell,
  ResizableTableContainer,
  Row,
  type SortDescriptor,
  Table,
  TableBody,
  TableHeader,
} from "react-aria-components";
import { afterEach, describe, expect, it } from "vitest";
import { SortableColumn } from "./handleTableHelpers";

afterEach(() => {
  cleanup();
});

// biome-ignore lint/style/useComponentExportOnlyModules: local test harness only
function Harness() {
  const [sortDescriptor, setSortDescriptor] = useState<SortDescriptor | null>(null);
  return (
    <ResizableTableContainer>
      <Table
        aria-label="Contact handles"
        sortDescriptor={sortDescriptor ?? undefined}
        onSortChange={setSortDescriptor}
      >
        <TableHeader>
          <SortableColumn id="service" isRowHeader align="left" allowsResizing>
            Service
          </SortableColumn>
          <SortableColumn id="handle" align="left" allowsResizing>
            Identity
          </SortableColumn>
        </TableHeader>
        <TableBody>
          <Row id="r1">
            <Cell>Phone</Cell>
            <Cell>+15551212</Cell>
          </Row>
        </TableBody>
      </Table>
    </ResizableTableContainer>
  );
}

describe("SortableColumn", () => {
  it("renders left-aligned column headers with visible resizers", () => {
    const { container } = render(<Harness />);
    expect(screen.getByRole("grid", { name: "Contact handles" })).toBeTruthy();
    const service = screen.getByRole("columnheader", { name: /Service/i });
    expect(service.className).toMatch(/text-left/);
    // RAC ColumnResizer uses role="presentation"; identify by resize cursor class.
    const resizers = container.querySelectorAll(".cursor-col-resize");
    expect(resizers.length).toBeGreaterThanOrEqual(2);
  });

  it("pins the resizer as the last child of a full-width flex row", () => {
    render(<Harness />);
    const service = screen.getByRole("columnheader", { name: /Service/i });
    const flexRow = service.querySelector(".relative.flex.w-full");
    expect(flexRow).toBeTruthy();
    const children = Array.from(flexRow?.children ?? []);
    expect(children.length).toBeGreaterThanOrEqual(2);
    const last = children[children.length - 1] as HTMLElement;
    expect(last.className).toMatch(/cursor-col-resize/);
    expect(last.className).toMatch(/absolute/);
    const group = children[0] as HTMLElement;
    expect(group.className).toMatch(/flex-1/);
  });

  it("accents the active sort column and flips direction on second click", async () => {
    const user = userEvent.setup();
    render(<Harness />);
    const service = screen.getByRole("columnheader", { name: /Service/i });
    const identity = screen.getByRole("columnheader", { name: /Identity/i });

    await user.click(service);
    expect(service.getAttribute("aria-sort")).toBe("ascending");
    expect(service.textContent).toContain("▲");
    expect(service.querySelector(".text-accent")).toBeTruthy();
    expect(identity.getAttribute("aria-sort")).toBe("none");

    await user.click(service);
    expect(service.getAttribute("aria-sort")).toBe("descending");
    expect(service.textContent).toContain("▼");
  });
});
