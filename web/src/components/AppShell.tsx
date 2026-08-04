"use client";

import { AppSidebar } from "@/components/AppSidebar";
import { PaneSeparator } from "@/components/PaneSeparator";
import { usePanelLayoutStorage } from "@/components/panelLayoutStorage";
import {
  NAV_RAIL_PX,
  useNavCollapse,
  type NavAnimDirection,
} from "@/components/useNavCollapse";
import { useCallback, useState, type ReactNode } from "react";
import {
  Group,
  Panel,
  useDefaultLayout,
  usePanelRef,
} from "react-resizable-panels";

/** App chrome: resizable nav drawer (compacts to icon rail) | main content. */
export function AppShell({
  active,
  labels = [],
  children,
}: {
  active: string;
  labels?: string[];
  children: ReactNode;
}) {
  const navPanelRef = usePanelRef();
  const [navCollapsed, setNavCollapsed] = useState(false);
  const [navAnim, setNavAnim] = useState<NavAnimDirection>(null);
  const [focusLabelsToken, setFocusLabelsToken] = useState(0);
  const onCollapsedChange = useCallback((c: boolean) => {
    setNavCollapsed(c);
  }, []);
  const { collapsed, collapse, expand } = useNavCollapse(
    navPanelRef,
    onCollapsedChange,
    setNavAnim,
  );
  const navAnimating = navAnim != null;
  const storage = usePanelLayoutStorage();

  const { defaultLayout, onLayoutChanged } = useDefaultLayout({
    id: "mv-nav",
    panelIds: ["nav", "main"],
    storage,
  });

  const onExpandLabels = useCallback(() => {
    void expand();
    setFocusLabelsToken((n) => n + 1);
  }, [expand]);

  const navMinSize = navAnimating ? 0 : 120;

  return (
    <div className="flex h-full w-full overflow-visible">
      <Group
        id="mv-nav"
        orientation="horizontal"
        className="min-h-0 min-w-0 flex-1"
        defaultLayout={defaultLayout}
        onLayoutChanged={navAnimating ? undefined : onLayoutChanged}
      >
        <Panel
          id="nav"
          panelRef={navPanelRef}
          defaultSize={200}
          minSize={navMinSize}
          maxSize={360}
          collapsible
          collapsedSize={NAV_RAIL_PX}
          className="min-h-0 overflow-hidden"
        >
          <AppSidebar
            active={active}
            labels={labels}
            collapsed={collapsed}
            animating={navAnim === "collapse"}
            onHideNav={() => void collapse()}
            onShowNav={() => void expand()}
            onExpandLabels={onExpandLabels}
            focusLabelsToken={focusLabelsToken}
          />
        </Panel>
        <PaneSeparator orientation="vertical" disabled={navCollapsed || navAnimating} />
        <Panel id="main" minSize="30%" className="min-h-0 min-w-0">
          {children}
        </Panel>
      </Group>
    </div>
  );
}
