"use client";

import type { ReactNode } from "react";
import { Separator } from "react-resizable-panels";

const baseVertical = "relative w-1 bg-border";
const baseHorizontal = "relative h-1 bg-border";
const interactive =
  "transition-colors hover:bg-accent/60 active:bg-accent/70";

/** Shared styled separator for react-resizable-panels. */
export function PaneSeparator({
  orientation = "horizontal",
  disabled,
  id,
  children,
}: {
  orientation?: "horizontal" | "vertical";
  disabled?: boolean;
  id?: string;
  children?: ReactNode;
}) {
  const base = orientation === "vertical" ? baseVertical : baseHorizontal;
  return (
    <Separator
      id={id}
      disabled={disabled}
      // Library sets cursor:not-allowed when disabled; override that.
      style={disabled ? { cursor: "default" } : undefined}
      className={disabled ? base : `${base} ${interactive}`}
    >
      {children}
    </Separator>
  );
}
