import { useContext } from "react";
import { RightToolbarContext } from "./rightToolbarState";

export function useRightToolbar() {
  return useContext(RightToolbarContext);
}

/** Register the groups or tags menu for the right-pane toolbar. */
export function useSetRightToolbar() {
  return useRightToolbar().setToolbar;
}
