import type { ReactNode } from "react";
import { LIST_TOOLBAR_CLASS } from "./ListRangeHeader";
import { useRightToolbar } from "./RightToolbarContext";

/** Remaining width: toolbar row, then the drawer, selection list, or placeholder. */
export default function RightPane({ children }: { children: ReactNode }) {
  const { toolbar } = useRightToolbar();
  return (
    <div className="flex min-h-0 min-w-0 flex-1 flex-col bg-bg">
      <div className={LIST_TOOLBAR_CLASS}>{toolbar}</div>
      <div className="flex min-h-0 flex-1 flex-col overflow-hidden">{children}</div>
    </div>
  );
}
