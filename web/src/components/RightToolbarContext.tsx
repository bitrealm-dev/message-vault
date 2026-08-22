/* eslint-disable react-refresh/only-export-components */
import { createContext, type ReactNode, useContext, useState } from "react";

type RightToolbarContextValue = {
  toolbar: ReactNode;
  setToolbar: (node: ReactNode | null) => void;
};

const RightToolbarContext = createContext<RightToolbarContextValue>({
  toolbar: null,
  setToolbar: () => {},
});

/** Holds the groups/tags control so the right-pane toolbar can render it. */
export function RightToolbarProvider({ children }: { children: ReactNode }) {
  const [toolbar, setToolbar] = useState<ReactNode>(null);
  return (
    <RightToolbarContext.Provider value={{ toolbar, setToolbar }}>
      {children}
    </RightToolbarContext.Provider>
  );
}

export function useRightToolbar(): RightToolbarContextValue {
  return useContext(RightToolbarContext);
}

/** Register the groups or tags menu for the right-pane toolbar. */
export function useSetRightToolbar() {
  return useRightToolbar().setToolbar;
}
