import { type ReactNode, useCallback, useState } from "react";
import { ColumnResizeSetContext, ColumnResizeStateContext } from "./columnResizeState";

/** Publishes column-drag state so VirtualList can pause row measurement. */
export function ColumnResizeProvider({ children }: { children: ReactNode }) {
  const [resizing, setResizing] = useState(false);
  const report = useCallback((dragging: boolean) => {
    setResizing(dragging);
  }, []);

  return (
    <ColumnResizeSetContext.Provider value={report}>
      <ColumnResizeStateContext.Provider value={resizing}>
        {children}
      </ColumnResizeStateContext.Provider>
    </ColumnResizeSetContext.Provider>
  );
}
