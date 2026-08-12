/** @vitest-environment jsdom */
import { describe, it, expect } from "vitest";
import { render, screen } from "@testing-library/react";
import FormField from "./FormField";

describe("FormField", () => {
  it("renders an inline label and control", () => {
    render(
      <FormField label="Source">
        <input aria-label="source-input" />
      </FormField>,
    );
    expect(screen.getByText("Source")).toBeInTheDocument();
    expect(screen.getByLabelText("source-input")).toBeInTheDocument();
  });

  it("renders stacked layout with optional trailing", () => {
    render(
      <FormField label="Password" layout="stacked" trailing={<span>show</span>}>
        <input aria-label="password-input" />
      </FormField>,
    );
    expect(screen.getByText("Password")).toBeInTheDocument();
    expect(screen.getByText("show")).toBeInTheDocument();
    expect(screen.getByLabelText("password-input")).toBeInTheDocument();
  });
});
