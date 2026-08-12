/** @vitest-environment jsdom */
import { describe, it, expect } from "vitest";
import { render, screen } from "@testing-library/react";
import ListRangeHeader from "./ListRangeHeader";

describe("ListRangeHeader", () => {
  it("shows the range label", () => {
    render(<ListRangeHeader rangeLabel="1–20 of 100" />);
    expect(screen.getByText("1–20 of 100")).toBeInTheDocument();
  });

  it("appends updating and loading suffixes", () => {
    const { rerender } = render(
      <ListRangeHeader rangeLabel="1–20 of 100" refreshing />,
    );
    expect(screen.getByText(/updating…/)).toBeInTheDocument();

    rerender(<ListRangeHeader rangeLabel="1–20 of 100" filling />);
    expect(screen.getByText(/loading more…/)).toBeInTheDocument();
  });
});
