"use client";

import { HistoryProvider, HistoryToast } from "@/components/history";
import type { ReactNode } from "react";

/** App-wide undo/redo stack and toast, shared across browse and settings. */
export function HistoryShell({ children }: { children: ReactNode }) {
  return (
    <HistoryProvider>
      {children}
      <HistoryToast />
    </HistoryProvider>
  );
}
