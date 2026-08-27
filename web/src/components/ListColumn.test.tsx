/** @vitest-environment jsdom */

import { render } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { ColumnResizeProvider } from "./ColumnResizeContext";
import ListColumn from "./ListColumn";

describe("ListColumn", () => {
  it("prefers the stored width but can shrink below it", () => {
    localStorage.setItem("listColumnWidth:v1", "300");
    const { container } = render(
      <ColumnResizeProvider>
        <ListColumn>
          <div>rows</div>
        </ListColumn>
      </ColumnResizeProvider>,
    );
    const column = container.querySelector("[data-list-column]");
    expect(column).toBeTruthy();
    expect(column).toHaveStyle({
      flex: "0 1 300px",
      minWidth: "0px",
      maxWidth: "300px",
      width: "300px",
    });
  });
});
