/** @vitest-environment jsdom */

import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import AuthErrorFooter from "./AuthErrorFooter";

afterEach(cleanup);

describe("AuthErrorFooter", () => {
  it("capitalizes the server's lowercase sentence", () => {
    render(<AuthErrorFooter error="invalid username or password" />);
    expect(screen.getByText("Invalid username or password")).toBeInTheDocument();
  });

  it("leaves an already-capitalized message alone", () => {
    render(<AuthErrorFooter error="Passwords do not match." />);
    expect(screen.getByText("Passwords do not match.")).toBeInTheDocument();
  });

  it("reserves its space when there is no message", () => {
    const { container } = render(<AuthErrorFooter error="" />);
    const line = container.firstElementChild;
    expect(line).toHaveClass("min-h-10");
    expect(line).toHaveAttribute("aria-live", "polite");
  });
});
