/** @vitest-environment jsdom */

import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import StepProgress, { type Step } from "./StepProgress";

const doneSteps: Step[] = [{ label: "Upload to vault", status: "done" }];

describe("StepProgress completion badge", () => {
  afterEach(() => {
    cleanup();
  });

  it("gives completed-with-issues its own badge, not the canceled/muted one", () => {
    render(<StepProgress steps={doneSteps} completionText="Import completed with issues" />);
    const text = screen.getByText("Import completed with issues");
    const badge = text.previousElementSibling;
    expect(badge).not.toBeNull();
    expect(badge?.className).toContain("bg-warn-soft-bg");
    expect(badge?.className).not.toContain("bg-border");
  });

  it("still gives a canceled/other completion the muted badge", () => {
    render(<StepProgress steps={doneSteps} completionText="Import canceled" />);
    const text = screen.getByText("Import canceled");
    const badge = text.previousElementSibling;
    expect(badge?.className).toContain("bg-border");
  });

  it("keeps the ok badge for a clean completion", () => {
    render(<StepProgress steps={doneSteps} completionText="Import complete" />);
    const text = screen.getByText("Import complete");
    const badge = text.previousElementSibling;
    expect(badge?.className).toContain("bg-ok");
  });
});
