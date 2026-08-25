/** @vitest-environment jsdom */

import { cleanup, render, screen } from "@testing-library/react";
import {
  Column,
  ResizableTableContainer,
  Table,
  TableBody,
  TableHeader,
} from "react-aria-components";
import { afterEach, describe, expect, it } from "vitest";
import type { CachedContactHandle } from "../../lib/contactDetailCache";
import { renderHandleTableRow } from "./HandleTableRow";

afterEach(() => {
  cleanup();
});

const sample: CachedContactHandle & { id: string } = {
  id: "h1",
  handle: "+1555121212",
  service: "phone",
  name_alias: "Mary Elizabeth Katherine",
  start_date: "2024-01-15T00:00:00Z",
  end_date: "2024-06-01T00:00:00Z",
  individual_conversations: 2,
  group_conversations: 1,
  individual_message_count: 10,
  group_message_count: 5,
};

describe("renderHandleTableRow", () => {
  it("left-aligns count cells and wraps alias only on spaces", () => {
    render(
      <ResizableTableContainer>
        <Table aria-label="Contact handles">
          <TableHeader>
            <Column isRowHeader>Service</Column>
            <Column>Identity</Column>
            <Column>Alias</Column>
            <Column>First Seen</Column>
            <Column>Last Seen</Column>
            <Column>Threads</Column>
            <Column>Direct</Column>
            <Column>Group</Column>
            <Column>Actions</Column>
          </TableHeader>
          <TableBody>
            {renderHandleTableRow(sample, {
              busy: false,
              loading: false,
              onRequestRemove: () => {},
            })}
          </TableBody>
        </Table>
      </ResizableTableContainer>,
    );

    const grid = screen.getByRole("grid", { name: "Contact handles" });
    const cells = grid.querySelectorAll("[role='gridcell'], [role='rowheader']");
    // Service, Identity, Alias, dates, Threads, Direct, Group, Actions
    expect(cells.length).toBeGreaterThanOrEqual(8);

    const aliasSpan = screen.getByTitle("Mary Elizabeth Katherine");
    expect(aliasSpan.className).toMatch(/break-normal/);
    expect(aliasSpan.className).not.toMatch(/break-all/);

    const threadsCell = Array.from(cells).find((c) => c.textContent === "3");
    expect(threadsCell?.className).toMatch(/text-left/);
  });
});
