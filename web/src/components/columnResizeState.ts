import { createContext, useContext } from "react";

/** True while any column width (nav or list) is being dragged. */
export const ColumnResizeStateContext = createContext(false);

/** Report that a column drag started or ended. */
export const ColumnResizeSetContext = createContext<(dragging: boolean) => void>(() => {});

export function useColumnResizing(): boolean {
  return useContext(ColumnResizeStateContext);
}

/** Stable callback for LeftPanel / ListColumn to publish drag state. */
export function useReportColumnResizing(): (dragging: boolean) => void {
  return useContext(ColumnResizeSetContext);
}
