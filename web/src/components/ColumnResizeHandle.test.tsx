/** @vitest-environment jsdom */

import { render } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import ColumnResizeHandle from "./ColumnResizeHandle";

const noopHandleProps = {
  onPointerDown: vi.fn(),
  onPointerMove: vi.fn(),
  onPointerUp: vi.fn(),
  onPointerCancel: vi.fn(),
  onKeyDown: vi.fn(),
  onMouseEnter: vi.fn(),
  onMouseLeave: vi.fn(),
};

describe("ColumnResizeHandle", () => {
  it("keeps the grip on the inner right edge so the next column cannot cover it", () => {
    const { getByRole } = render(
      <ColumnResizeHandle
        ariaLabel="Resize navigation panel"
        width={220}
        minWidth={160}
        maxWidth={520}
        dragging={false}
        handleHover={false}
        handleProps={noopHandleProps}
      />,
    );

    const handle = getByRole("separator", { name: "Resize navigation panel" });
    expect(handle.className).toContain("right-0");
    expect(handle.className).not.toContain("translate-x-full");
  });
});
