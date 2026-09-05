"use client";

import { HistoryProvider } from "@/components/history";
import type { ReactNode } from "react";

/**
 * Undo and redo are not features of web-next. The provider stays mounted so
 * components that call `useHistory()` keep rendering, but nothing pushes onto
 * its stack (every write answers 501), the toast is not shown, and the
 * Undo/Redo entries are gone from the list menus.
 */
export function HistoryShell({ children }: { children: ReactNode }) {
  return <HistoryProvider>{children}</HistoryProvider>;
}
