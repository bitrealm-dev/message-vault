import { createContext, useContext } from "react";

/** True while the list column width is being dragged. */
export const ListColumnResizeContext = createContext(false);

export function useListColumnResizing(): boolean {
  return useContext(ListColumnResizeContext);
}
