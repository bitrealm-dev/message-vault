import { type ReactNode, useState } from "react";
import { RightToolbarContext } from "./rightToolbarState";

/** Holds the groups/tags control so the right-pane toolbar can render it. */
export function RightToolbarProvider({ children }: { children: ReactNode }) {
  const [toolbar, setToolbar] = useState<ReactNode>(null);
  return (
    <RightToolbarContext.Provider value={{ toolbar, setToolbar }}>
      {children}
    </RightToolbarContext.Provider>
  );
}
