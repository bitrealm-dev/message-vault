"use client";

import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type RefObject,
} from "react";
import type { PanelImperativeHandle, PanelSize } from "react-resizable-panels";

const DEFAULT_GROUPS_MIN_PX = 180;

/**
 * Persist a collapsible groups panel from resize/drag.
 * When groups is at minSize and further drag would steal from list, collapse
 * groups and restore list width so panel 2 is not shrunk first.
 */
export function usePanelCollapse(
  groupsPanelRef: RefObject<PanelImperativeHandle | null>,
  listPanelRef: RefObject<PanelImperativeHandle | null>,
  storageKey: string,
  groupsMinPx = DEFAULT_GROUPS_MIN_PX,
) {
  const prevListPxRef = useRef<number | null>(null);
  const [groupsCollapsed, setGroupsCollapsed] = useState(() => {
    if (typeof window === "undefined") return false;
    return window.localStorage.getItem(storageKey) === "1";
  });

  const persistCollapsed = useCallback(() => {
    const groups = groupsPanelRef.current;
    if (!groups) return;
    const collapsed = groups.isCollapsed();
    window.localStorage.setItem(storageKey, collapsed ? "1" : "0");
    setGroupsCollapsed(collapsed);
  }, [groupsPanelRef, storageKey]);

  const collapseInsteadOfShrinkingList = useCallback(() => {
    const groups = groupsPanelRef.current;
    const list = listPanelRef.current;
    if (!groups || !list || groups.isCollapsed()) return false;
    if (groups.getSize().inPixels > groupsMinPx + 0.5) return false;
    if (prevListPxRef.current == null) return false;
    const listPx = list.getSize().inPixels;
    if (listPx >= prevListPxRef.current - 0.5) return false;

    const restorePx = prevListPxRef.current;
    groups.collapse();
    list.resize(restorePx);
    prevListPxRef.current = list.getSize().inPixels;
    persistCollapsed();
    return true;
  }, [groupsPanelRef, listPanelRef, groupsMinPx, persistCollapsed]);

  useEffect(() => {
    let cancelled = false;
    const apply = () => {
      if (cancelled) return;
      const panel = groupsPanelRef.current;
      if (!panel) {
        requestAnimationFrame(apply);
        return;
      }
      if (
        window.localStorage.getItem(storageKey) === "1" &&
        !panel.isCollapsed()
      ) {
        panel.collapse();
      }
      setGroupsCollapsed(panel.isCollapsed());
      const list = listPanelRef.current;
      if (list) prevListPxRef.current = list.getSize().inPixels;
    };
    apply();
    return () => {
      cancelled = true;
    };
  }, [groupsPanelRef, listPanelRef, storageKey]);

  const onGroupsResize = useCallback(
    (_size: PanelSize) => {
      if (collapseInsteadOfShrinkingList()) return;
      const list = listPanelRef.current;
      if (list) prevListPxRef.current = list.getSize().inPixels;
      persistCollapsed();
    },
    [collapseInsteadOfShrinkingList, listPanelRef, persistCollapsed],
  );

  const onListResize = useCallback(
    (size: PanelSize) => {
      if (collapseInsteadOfShrinkingList()) return;
      prevListPxRef.current = size.inPixels;
    },
    [collapseInsteadOfShrinkingList],
  );

  return { onGroupsResize, onListResize, groupsCollapsed };
}
